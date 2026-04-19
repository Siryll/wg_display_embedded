//! Widget rendering loop, executes widgets and displays their information on the screen.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::models::SystemConfiguration;
use defmt::{error, info};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::Drawable;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::geometry::Size;
use embedded_graphics::mono_font::MonoFont;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::iso_8859_1::{FONT_8X13, FONT_9X18_BOLD};
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::primitives::{Line, Primitive, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_framebuf::FrameBuf;

use alloc::format;

use crate::runtime::Runtime;
use crate::util::globals;

/// Constant for the delay between widget update checks
const RENDER_TICK_MS: Duration = Duration::from_millis(1000);
const DISPLAY_WIDTH: u32 = 320;
const DISPLAY_HEIGHT: u32 = 240;
const DISPLAY_PIXELS: usize = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize);
const ACCENT_WIDTH: i32 = 3;
const LEFT_PADDING: i32 = ACCENT_WIDTH + 5;

// change to true to enable larger font, might be changed in the future to be configurable via web ui
const USE_LARGE_FONT: bool = cfg!(feature = "large-font");

fn active_font() -> &'static MonoFont<'static> {
    if USE_LARGE_FONT {
        &FONT_9X18_BOLD
    } else {
        &FONT_8X13
    }
}

fn line_height() -> i32 {
    if USE_LARGE_FONT { 20 } else { 14 }
}

fn header_height() -> i32 {
    if USE_LARGE_FONT { 24 } else { 18 }
}

fn widget_gap() -> i32 {
    if USE_LARGE_FONT { 8 } else { 6 }
}

fn display_width_chars() -> usize {
    (DISPLAY_WIDTH as usize / active_font().character_size.width as usize).saturating_sub(1)
}

struct WasmWidget {
    name: String,
    config_json: String,
    update_cycle_seconds: u32,
    last_run: Option<Instant>,
    last_output: String,
}

pub struct Renderer {
    widgets: Vec<WasmWidget>,
    framebuffer: Box<[Rgb565; DISPLAY_PIXELS]>,
    background_color: Rgb565,
    runtime: Runtime,
    ip_address: String,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            // TODO: maybe make static  or use vec!
            framebuffer: Box::new([Rgb565::BLACK; DISPLAY_PIXELS]),
            background_color: Rgb565::BLACK,
            runtime: Runtime::new(),
            ip_address: "IP unknown".to_string(),
        }
    }

    /// Update the information stored in [Renderer::widgets] based on the provided [`SystemConfiguration`].
    fn update_widget_information(&mut self, config: &SystemConfiguration) {
        self.background_color = parse_background_color(config.background_color.as_str());
        self.widgets = config
            .widgets
            .iter()
            .map(|wc| WasmWidget {
                name: wc.name.clone(),
                config_json: wc.json_config.clone(),
                update_cycle_seconds: if wc.update_cycle_seconds > 0 {
                    wc.update_cycle_seconds
                } else {
                    1
                },
                last_run: None,
                last_output: "-".to_string(),
            })
            .collect();
    }

    /// Renderer loop, stores copy of [`SystemConfiguration`] and updates it only if changes are detected via [`Storage::get_system_config_change`](crate::storage::Storage::get_system_config_change).
    /// This function will never return and run indefinitly.
    /// Runs on the seccond core due to [Runtime::run()](crate::runtime::Runtime::run()) being blocking due to Wasmtime's host functions.
    /// See [`runtime::http_sync`](crate::runtime::http_sync) for details.
    /// Only loads and runs widgets once their update cycle has passed.
    pub async fn run(&mut self) {
        self.ip_address = globals::with_storage(|storage| {
            storage
                .config_get("device_ip")
                .unwrap_or_else(|_| "IP unknown".to_string())
        })
        .await;

        let mut config = globals::with_storage(|storage| {
            let config = storage.get_system_config();
            match config {
                Ok(config) => config,
                Err(err) => {
                    error!("Failed to get system config: {:?}", err);
                    SystemConfiguration::default()
                }
            }
        })
        .await;

        self.update_widget_information(&config);
        info!("Renderer initialized {} widgets", self.widgets.len());
        self.render_layout().await;

        let mut loop_time: Instant;

        loop {
            // set loop time
            loop_time = Instant::now();
            // update config if changes were made in the web ui
            if let Some(new_config) =
                globals::with_storage(|storage| storage.get_system_config_change()).await
            {
                config = new_config;
                self.update_widget_information(&config);
                info!("Renderer reloaded {} widgets", self.widgets.len());
            }

            self.update_widgets().await;
            self.render_layout().await;

            // wait for 1 second minus the elapes time in this loop
            // will skip if loop took longer than 1 second
            if let Some(loop_delay) = RENDER_TICK_MS.checked_sub(loop_time.elapsed()) {
                Timer::after(loop_delay).await;
            }
        }
    }

    /// Checks if any widget needs to be run this cycle, runs them, and updates their last output.
    async fn update_widgets(&mut self) {
        let now = Instant::now();
        for widget in &mut self.widgets {
            let should_run = match widget.last_run {
                None => true,
                Some(last) => {
                    now.duration_since(last)
                        >= Duration::from_secs(u64::from(widget.update_cycle_seconds))
                }
            };
            if !should_run {
                continue;
            }

            widget.last_run = Some(now);

            let widget_result = unsafe {
                self.runtime
                    .run_widget(widget.name.clone(), widget.config_json.clone())
                    .await
            };

            widget.last_output = match widget_result {
                Ok(Some(result)) => result.data,
                Ok(None) => "No output".to_string(),
                Err(err) => {
                    error!(
                        "Widget '{}' execution failed: {:?}",
                        widget.name.as_str(),
                        defmt::Debug2Format(&err)
                    );
                    "Widget execution failed".to_string()
                }
            };
        }
    }

    /// Renders the screen layout, writes into a framebuffer to avoid screen flickering.
    async fn render_layout(&mut self) {
        let font = active_font();
        let line_height = line_height();
        let header_height = header_height();
        let widget_gap = widget_gap();

        let mut fb = FrameBuf::new(
            self.framebuffer.as_mut(),
            DISPLAY_WIDTH as usize,
            DISPLAY_HEIGHT as usize,
        );
        let _ = fb.clear(self.background_color);

        // header bar
        Rectangle::new(
            Point::new(0, 0),
            Size::new(DISPLAY_WIDTH, header_height as u32),
        )
        .into_styled(PrimitiveStyle::with_fill(Rgb565::new(1, 8, 16)))
        .draw(&mut fb)
        .ok();

        let header_style = MonoTextStyle::new(font, Rgb565::WHITE);
        draw_text(
            &mut fb,
            &format!("WG Display  {}", self.ip_address),
            4,
            header_height - 4,
            &header_style,
        );

        // divider
        Line::new(
            Point::new(0, header_height),
            Point::new(DISPLAY_WIDTH as i32 - 1, header_height),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 1))
        .draw(&mut fb)
        .ok();

        // widgets
        let name_style = MonoTextStyle::new(font, Rgb565::CYAN);
        let output_style = MonoTextStyle::new(font, Rgb565::YELLOW);

        let mut y = header_height + line_height + 2;
        let widget_count = self.widgets.len();

        for (i, widget) in self.widgets.iter().enumerate() {
            // stop if no space left on screen
            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }

            // accent bar
            Rectangle::new(
                Point::new(0, y - (line_height - 3)),
                Size::new(ACCENT_WIDTH as u32, (line_height - 1) as u32),
            )
            .into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN))
            .draw(&mut fb)
            .ok();

            // widget name
            draw_text(&mut fb, &widget.name, LEFT_PADDING, y, &name_style);
            y += line_height;

            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }

            // draw each output line of widget
            for line in widget.last_output.lines() {
                if y >= DISPLAY_HEIGHT as i32 {
                    break;
                }
                draw_text(&mut fb, line, LEFT_PADDING, y, &output_style);
                y += line_height;
            }

            // thin separator between widgets
            if i + 1 < widget_count {
                let sep_y = y - line_height + (line_height / 2 - 1);
                if sep_y > header_height && sep_y < DISPLAY_HEIGHT as i32 {
                    Line::new(
                        Point::new(LEFT_PADDING, sep_y),
                        Point::new(DISPLAY_WIDTH as i32 - LEFT_PADDING, sep_y),
                    )
                    .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(4, 8, 4), 1))
                    .draw(&mut fb)
                    .ok();
                }
                y += widget_gap;
            }
        }

        let pixel_iter = self.framebuffer.iter().copied();
        globals::with_display(|display| {
            let _ = display.display_mut().set_pixels(
                0,
                0,
                (DISPLAY_WIDTH - 1) as u16,
                (DISPLAY_HEIGHT - 1) as u16,
                pixel_iter,
            );
        })
        .await;
    }
}

fn parse_background_color(color: &str) -> Rgb565 {
    let hex = color.strip_prefix('#').unwrap_or(color);
    if hex.len() != 6 {
        return Rgb565::BLACK;
    }

    let r = u8::from_str_radix(&hex[0..2], 16).ok();
    let g = u8::from_str_radix(&hex[2..4], 16).ok();
    let b = u8::from_str_radix(&hex[4..6], 16).ok();

    match (r, g, b) {
        (Some(r), Some(g), Some(b)) => Rgb565::new(r >> 3, g >> 2, b >> 3),
        _ => Rgb565::BLACK,
    }
}

fn draw_text<T>(target: &mut T, text: &str, x: i32, y: i32, style: &MonoTextStyle<'_, Rgb565>)
where
    T: DrawTarget<Color = Rgb565>,
{
    let max_chars = display_width_chars();
    let truncated = if text.len() > max_chars {
        let mut s = text
            .chars()
            .take(max_chars.saturating_sub(3))
            .collect::<String>();
        s.push_str("...");
        s
    } else {
        text.to_string()
    };
    let _ = Text::new(&truncated, Point::new(x, y), *style).draw(target);
}
