use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::boxed::Box;
use core::cmp;

use common::models::SystemConfiguration;
use defmt::{error, info, warn};
use embassy_time::{Duration, Instant, Timer};
use embedded_graphics::Drawable;
use embedded_graphics::draw_target::DrawTarget;
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
const DISPLAY_HEIGHT_I32: i32 = 240;
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

    // fully reload all widget on any config change, could be optimized to only reload changed widgets
    fn update_widget_information(config: &SystemConfiguration) -> Vec<WasmWidget> {
        let mut widgets = Vec::new();

        for widget_config in config.widgets.iter() {
            widgets.push(WasmWidget {
                name: widget_config.name.clone(),
                config_json: widget_config.json_config.clone(),
                update_cycle_seconds: cmp::max(widget_config.update_cycle_seconds, 1),
                last_run: None,
                last_output: "-".to_string(),
            });
        }

        widgets
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

        for widget in self.widgets.iter_mut() {
            let run_interval = Duration::from_secs(u64::from(widget.update_cycle_seconds));
            // check if widget information needs to be updated
            let should_run = match widget.last_run {
                Some(last_run) => now.duration_since(last_run) >= run_interval,
                None => true,
            };

            if !should_run {
                continue;
            }

            widget.last_run = Some(now);

            // get widget binary from storage
            let wasm_bytes = globals::with_storage(|storage| storage.wasm_read(widget.name.as_str())).await;
            let wasm_bytes = match wasm_bytes {
                Ok(wasm_bytes) => wasm_bytes,
                Err(err) => {
                    widget.last_output = "Widget binary missing".to_string();
                    warn!("Could not read widget binary '{}': {:?}", widget.name.as_str(), err);
                    continue;
                }
            };

            let mut runtime = Runtime::new();
            // instatiate component
            let component = unsafe { runtime.load_module(&wasm_bytes) };
            let component = match component {
                Ok(component) => component,
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

            let instance = runtime.instantiate(&component);
            let instance = match instance {
                Ok(instance) => instance,
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

            match runtime.run(&instance, widget.config_json.clone()) {
                Ok(Some(result)) => {
                    widget.last_output = result.data;
                }
                Ok(None) => {
                    widget.last_output = "No output".to_string();
                }
                Err(err) => {
                    widget.last_output = "Widget execution failed".to_string();
                    error!(
                        "Widget '{}' execution failed: {:?}",
                        widget.name.as_str(),
                        defmt::Debug2Format(&err)
                    );
                }
            }
        }
    }

    async fn render_layout(&mut self) {
        {
            let mut framebuffer =
                FrameBuf::new(self.framebuffer.as_mut(), DISPLAY_WIDTH as usize, DISPLAY_HEIGHT as usize);
            let title_style = MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE);
            let name_style = MonoTextStyle::new(&FONT_8X13, Rgb565::CYAN);
            let output_style = MonoTextStyle::new(&FONT_8X13, Rgb565::YELLOW);

            let _ = framebuffer.clear(Rgb565::BLACK);

            let mut y = FIRST_LINE_Y;
            Self::draw_line(&mut framebuffer, Self::get_title().as_str(), y, &title_style);
            y += LINE_HEIGHT;

            for widget in self.widgets.iter() {
                if y >= DISPLAY_HEIGHT_I32 {
                    break;
                }
                Self::draw_line(&mut framebuffer, widget.name.as_str(), y, &name_style);
                y += LINE_HEIGHT;

                if y >= DISPLAY_HEIGHT_I32 {
                    break;
                }
                let output = Self::first_line(widget.last_output.as_str());
                Self::draw_line(&mut framebuffer, output.as_str(), y, &output_style);
                y += LINE_HEIGHT;
            }
        }

        let pixel_iterator = self.framebuffer.iter().copied();

        globals::with_display(|display| {
            let target = display.display_mut();
            let _ = target.set_pixels(0, 0, (DISPLAY_WIDTH - 1) as u16, (DISPLAY_HEIGHT - 1) as u16, pixel_iterator);
        })
        .await;
    }

    fn draw_line<T>(target: &mut T, line: &str, y: i32, style: &MonoTextStyle<'_, Rgb565>)
    where
        T: DrawTarget<Color = Rgb565>,
    {
        let text = Self::truncate_line(line, DISPLAY_WIDTH_CHARS);
        let _ = Text::new(text.as_str(), Point::new(LEFT_PADDING, y), *style).draw(target);
    }

    fn truncate_line(value: &str, max_chars: usize) -> String {
        let len = value.chars().count();
        if len <= max_chars {
            return value.to_string();
        }

        let mut truncated = String::new();
        for ch in value.chars().take(max_chars.saturating_sub(3)) {
            truncated.push(ch);
        }
        truncated.push_str("...");
        truncated
    }

    fn first_line(value: &str) -> String {
        value.lines().next().unwrap_or("-").to_string()
    }

    fn get_title() -> String {
        "Embedded App".to_string()
    }
}