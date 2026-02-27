#![no_std]
#![no_main]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]

use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;

mod wifi;
use crate::wifi::Wifi;

mod common;
mod util;
mod widget;
use crate::util::globals;

mod display;
use crate::display::Display;

mod storage;
use crate::storage::Storage;

mod http_client;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_8X13},
    text::Text,
};

use embedded_graphics::Drawable;

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

extern crate alloc;

// This creates a default app-descriptor required by the esp-idf bootloader.
// For more information see: <https://docs.espressif.com/projects/esp-idf/en/stable/esp32/api-reference/system/app_image_format.html#application-description>
esp_bootloader_esp_idf::esp_app_desc!();

#[allow(
    clippy::large_stack_frames,
    reason = "it's not unusual to allocate larger buffers etc. in main"
)]
#[esp_rtos::main]
async fn main(spawner: Spawner) -> ! {
    // generator version: 1.2.0

    let config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(config);

    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // -- Storage setup --
    let storage = Storage::new(peripherals.FLASH).expect("Failed to initialize storage");
    globals::init_storage(storage).await;

    // Set ssid and pw on first compile, until configuration via UI is possible
    // storage.config_set("ssid", "").expect("Failed to write config");
    // storage.config_set("pw", "").expect("Failed to write config");

    // -- Display setup --
    let mut display = Display::new(
        peripherals.SPI2,
        peripherals.DMA_CH0,
        peripherals.GPIO4,
        peripherals.GPIO5,
        peripherals.GPIO6,
        peripherals.GPIO7,
        peripherals.GPIO47,
        peripherals.GPIO48,
    );
    Text::new(
        "Hello ESP32!",
        Point::new(100, 60),
        MonoTextStyle::new(&FONT_8X13, Rgb565::WHITE),
    )
    .draw(display.display_mut())
    .unwrap();

    // -- Wifi setup --
    let ssid = globals::with_storage(|storage| storage.config_get("ssid").unwrap()).await;
    let password = globals::with_storage(|storage| storage.config_get("pw").unwrap()).await;
    // let ssid = storage.config_get("ssid").unwrap();
    // let password = storage.config_get("pw").unwrap();

    let wifi = Wifi::start_station(peripherals.WIFI, &spawner, ssid, password);
    wifi.wait_for_connection().await;
    globals::init_network(wifi.stack(), wifi.tls_seed());

    // -- HTTP client setup --
    let http_client = globals::http_client();
    let response = http_client
        .get("https://jsonplaceholder.typicode.com/posts/1")
        .await
        .expect("Failed to make GET request");
    match core::str::from_utf8(&response) {
        Ok(s) => info!("Response: {}", s),
        Err(_) => info!("Response: [binary data, {} bytes]", response.len()),
    }

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(10)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}
