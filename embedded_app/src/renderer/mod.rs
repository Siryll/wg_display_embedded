use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use common::models::SystemConfiguration;
use defmt::{error, info, warn};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::Drawable;
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_8X13;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::geometry::Size;
use embedded_graphics::primitives::{Line, Primitive, PrimitiveStyle, Rectangle};
use embedded_graphics::text::Text;
use embedded_graphics_framebuf::FrameBuf;

use alloc::format;

use crate::runtime::Runtime;
use crate::util::globals;

const RENDER_TICK_MS: u64 = 1000;
const DISPLAY_WIDTH: u32 = 320;
const DISPLAY_HEIGHT: u32 = 240;
const DISPLAY_PIXELS: usize = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize);
const DISPLAY_WIDTH_CHARS: usize = 39;
const HEADER_HEIGHT: i32 = 18;
const ACCENT_WIDTH: i32 = 3;
const LEFT_PADDING: i32 = ACCENT_WIDTH + 5; // text x start after accent bar + gap
const LINE_HEIGHT: i32 = 14;
const WIDGET_GAP: i32 = 6; // extra vertical space between widgets (houses the separator)

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
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            framebuffer: Box::new([Rgb565::BLACK; DISPLAY_PIXELS]),
            background_color: Rgb565::BLACK,
        }
    }

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

    pub async fn run(&mut self) {
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

        loop {
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

            Timer::after(Duration::from_millis(RENDER_TICK_MS)).await;
        }
    }

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

            let wasm_bytes =
                match globals::with_storage(|s| s.wasm_read(widget.name.as_str())).await {
                    Ok(bytes) => bytes,
                    Err(err) => {
                        widget.last_output = "Widget binary missing".to_string();
                        warn!(
                            "Could not read widget '{}': {:?}",
                            widget.name.as_str(),
                            err
                        );
                        continue;
                    }
                };

            let mut runtime = Runtime::new();
            let component = match unsafe { runtime.load_module(&wasm_bytes) } {
                Ok(c) => c,
                Err(err) => {
                    widget.last_output = "Widget component invalid".to_string();
                    error!(
                        "Could not deserialize widget '{}': {:?}",
                        widget.name.as_str(),
                        defmt::Debug2Format(&err)
                    );
                    continue;
                }
            };

            let instance = match runtime.instantiate(&component) {
                Ok(i) => i,
                Err(err) => {
                    widget.last_output = "Widget instantiate failed".to_string();
                    error!(
                        "Could not instantiate widget '{}': {:?}",
                        widget.name.as_str(),
                        defmt::Debug2Format(&err)
                    );
                    continue;
                }
            };

            widget.last_output = match runtime.run(&instance, widget.config_json.clone()) {
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

    async fn render_layout(&mut self) {
        let mut fb = FrameBuf::new(
            self.framebuffer.as_mut(),
            DISPLAY_WIDTH as usize,
            DISPLAY_HEIGHT as usize,
        );
        let _ = fb.clear(self.background_color);

        // ---- Header bar ----
        Rectangle::new(Point::new(0, 0), Size::new(DISPLAY_WIDTH, HEADER_HEIGHT as u32))
            .into_styled(PrimitiveStyle::with_fill(Rgb565::new(1, 8, 16)))
            .draw(&mut fb)
            .ok();

        let ip_str = globals::with_storage(|storage| {
            storage
                .config_get("device_ip")
                .unwrap_or_else(|_| "IP unknown".to_string())
        })
        .await;

        let header_style = MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE);
        draw_text(&mut fb, &format!("WG Display  {}", ip_str), 4, HEADER_HEIGHT - 4, &header_style);

        // Cyan divider under header
        Line::new(
            Point::new(0, HEADER_HEIGHT),
            Point::new(DISPLAY_WIDTH as i32 - 1, HEADER_HEIGHT),
        )
        .into_styled(PrimitiveStyle::with_stroke(Rgb565::CYAN, 1))
        .draw(&mut fb)
        .ok();

        // ---- Widgets ----
        let name_style = MonoTextStyle::new(&FONT_8X13, Rgb565::CYAN);
        let output_style = MonoTextStyle::new(&FONT_8X13, Rgb565::YELLOW);

        let mut y = HEADER_HEIGHT + LINE_HEIGHT + 2;
        let widget_count = self.widgets.len();

        for (i, widget) in self.widgets.iter().enumerate() {
            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }

            // Accent bar on the left edge of the widget name line
            Rectangle::new(Point::new(0, y - 11), Size::new(ACCENT_WIDTH as u32, 13))
                .into_styled(PrimitiveStyle::with_fill(Rgb565::CYAN))
                .draw(&mut fb)
                .ok();

            draw_text(&mut fb, &widget.name, LEFT_PADDING, y, &name_style);
            y += LINE_HEIGHT;

            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }

            for line in widget.last_output.lines() {
                if y >= DISPLAY_HEIGHT as i32 {
                    break;
                }
                draw_text(&mut fb, line, LEFT_PADDING, y, &output_style);
                y += LINE_HEIGHT;
            }

            // Thin separator between widgets, placed inside the WIDGET_GAP.
            // After the output loop, y is at the next name's baseline.
            // Last output bottom = y - LINE_HEIGHT + 2 = y - 12
            // With WIDGET_GAP added, next name top = (y + WIDGET_GAP) - 11 = y + 1
            // sep_y = y - (LINE_HEIGHT - 6) = y - 8 sits between those two bounds.
            if i + 1 < widget_count {
                let sep_y = y - LINE_HEIGHT + 6; // = y - 8: below last output, above next title
                if sep_y > HEADER_HEIGHT && sep_y < DISPLAY_HEIGHT as i32 {
                    Line::new(
                        Point::new(LEFT_PADDING, sep_y),
                        Point::new(DISPLAY_WIDTH as i32 - LEFT_PADDING, sep_y),
                    )
                    .into_styled(PrimitiveStyle::with_stroke(Rgb565::new(4, 8, 4), 1))
                    .draw(&mut fb)
                    .ok();
                }
                y += WIDGET_GAP;
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
    let truncated = if text.len() > DISPLAY_WIDTH_CHARS {
        let mut s = text
            .chars()
            .take(DISPLAY_WIDTH_CHARS.saturating_sub(3))
            .collect::<String>();
        s.push_str("...");
        s
    } else {
        text.to_string()
    };
    let _ = Text::new(&truncated, Point::new(x, y), *style).draw(target);
}
