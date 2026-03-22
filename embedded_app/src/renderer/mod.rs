use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::boxed::Box;

use common::models::SystemConfiguration;
use defmt::{error, info, warn};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::Drawable;
use embedded_graphics::mono_font::MonoTextStyle;
use embedded_graphics::mono_font::ascii::FONT_8X13;
use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::text::Text;
use embedded_graphics_framebuf::FrameBuf;

use crate::runtime::Runtime;
use crate::util::globals;

const RENDER_TICK_MS: u64 = 1000;
const DISPLAY_WIDTH: u32 = 320;
const DISPLAY_HEIGHT: u32 = 240;
const DISPLAY_PIXELS: usize = (DISPLAY_WIDTH as usize) * (DISPLAY_HEIGHT as usize);
const DISPLAY_WIDTH_CHARS: usize = 39;
const LEFT_PADDING: i32 = 4;
const LINE_HEIGHT: i32 = 14;
const FIRST_LINE_Y: i32 = 14;

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
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            widgets: Vec::new(),
            framebuffer: Box::new([Rgb565::BLACK; DISPLAY_PIXELS]),
        }
    }

    fn update_widget_information(config: &SystemConfiguration) -> Vec<WasmWidget> {
        config.widgets.iter().map(|wc| WasmWidget {
            name: wc.name.clone(),
            config_json: wc.json_config.clone(),
            update_cycle_seconds: if wc.update_cycle_seconds > 0 { wc.update_cycle_seconds } else { 1 },
            last_run: None,
            last_output: "-".to_string(),
        }).collect()
    }

    pub async fn run(&mut self) {
        let mut config = globals::with_storage(|storage| {
            let config = storage.get_widget_config();
            let config = match config {
                Ok(config) => config,
                Err(err) => {
                    error!("Failed to get widget config: {:?}", err);
                    SystemConfiguration::default()
                }
            };
            config
        })
        .await;

        self.widgets = Self::update_widget_information(&config);
        info!("Renderer initialized {} widgets", self.widgets.len());
        self.render_layout().await;

        loop {
            // update config if changes were made in the web ui
            if let Some(new_config) =
                globals::with_storage(|storage| storage.get_system_config_change()).await
            {
                config = new_config;
                self.widgets = Self::update_widget_information(&config);
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
                Some(last) => now.duration_since(last) >= Duration::from_secs(u64::from(widget.update_cycle_seconds)),
            };
            if !should_run {
                continue;
            }

            widget.last_run = Some(now);

            let wasm_bytes = match globals::with_storage(|s| s.wasm_read(widget.name.as_str())).await {
                Ok(bytes) => bytes,
                Err(err) => {
                    widget.last_output = "Widget binary missing".to_string();
                    warn!("Could not read widget '{}': {:?}", widget.name.as_str(), err);
                    continue;
                }
            };

            let mut runtime = Runtime::new();
            let component = match unsafe { runtime.load_module(&wasm_bytes) } {
                Ok(c) => c,
                Err(err) => {
                    widget.last_output = "Widget component invalid".to_string();
                    error!("Could not deserialize widget '{}': {:?}", widget.name.as_str(), defmt::Debug2Format(&err));
                    continue;
                }
            };

            let instance = match runtime.instantiate(&component) {
                Ok(i) => i,
                Err(err) => {
                    widget.last_output = "Widget instantiate failed".to_string();
                    error!("Could not instantiate widget '{}': {:?}", widget.name.as_str(), defmt::Debug2Format(&err));
                    continue;
                }
            };

            widget.last_output = match runtime.run(&instance, widget.config_json.clone()) {
                Ok(Some(result)) => result.data,
                Ok(None) => "No output".to_string(),
                Err(err) => {
                    error!("Widget '{}' execution failed: {:?}", widget.name.as_str(), defmt::Debug2Format(&err));
                    "Widget execution failed".to_string()
                }
            };
        }
    }

    async fn render_layout(&mut self) {
        let mut framebuffer =
            FrameBuf::new(self.framebuffer.as_mut(), DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);

        let _ = framebuffer.clear(Rgb565::BLACK);

        let white = MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE);
        let cyan = MonoTextStyle::new(&FONT_8X13, Rgb565::CYAN);
        let yellow = MonoTextStyle::new(&FONT_8X13, Rgb565::YELLOW);

        let mut y = FIRST_LINE_Y;
        draw_text(&mut framebuffer, "Embedded App", y, &white);
        y += LINE_HEIGHT;

        for widget in self.widgets.iter() {
            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }
            draw_text(&mut framebuffer, &widget.name, y, &cyan);
            y += LINE_HEIGHT;

            if y >= DISPLAY_HEIGHT as i32 {
                break;
            }
            let output = widget.last_output.lines().next().unwrap_or("-");
            draw_text(&mut framebuffer, output, y, &yellow);
            y += LINE_HEIGHT;
        }

        let pixel_iter = self.framebuffer.iter().copied();
        globals::with_display(|display| {
            let _ = display.display_mut().set_pixels(
                0, 0,
                (DISPLAY_WIDTH - 1) as u16,
                (DISPLAY_HEIGHT - 1) as u16,
                pixel_iter,
            );
        }).await;
    }

}

fn draw_text<T>(target: &mut T, text: &str, y: i32, style: &MonoTextStyle<'_, Rgb565>)
where
    T: DrawTarget<Color = Rgb565>,
{
    let truncated = if text.len() > DISPLAY_WIDTH_CHARS {
        let mut s = text.chars().take(DISPLAY_WIDTH_CHARS.saturating_sub(3)).collect::<String>();
        s.push_str("...");
        s
    } else {
        text.to_string()
    };
    let _ = Text::new(&truncated, Point::new(LEFT_PADDING, y), *style).draw(target);
}