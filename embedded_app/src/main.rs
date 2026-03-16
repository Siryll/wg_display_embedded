#![no_std]
#![no_main]
#![feature(impl_trait_in_assoc_type)]
#![deny(
    clippy::mem_forget,
    reason = "mem::forget is generally not safe to do with esp_hal types, especially those \
    holding buffers for the duration of a data transfer."
)]
#![deny(clippy::large_stack_frames)]
#![recursion_limit = "256"]

use defmt::info;
use defmt::error;
use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::system::Stack as CoreStack;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;

mod wifi;
use crate::wifi::Wifi;

mod util;
mod widget;
use crate::util::globals;

mod display;
use crate::display::Display;

mod runtime;

mod storage;
use crate::storage::Storage;

mod http_client;

mod http_server;

use embedded_graphics::pixelcolor::Rgb565;
use embedded_graphics::prelude::{Point, RgbColor};
use embedded_graphics::{
    mono_font::{MonoTextStyle, ascii::FONT_8X13},
    text::Text,
};

use embedded_graphics::Drawable;

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    if let Some(location) = info.location() {
        error!(
            "Panic occurred at {=str}:{=u32}",
            location.file(),
            location.line()
        );
    } else {
        error!("Panic occurred at unknown location");
    }

    esp_println::println!("panic info: {}", info);

    loop {
        core::hint::spin_loop();
    }
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

    // initalizeing PSRAM before heap fixes widget http host function access for some reason
    esp_alloc::psram_allocator!(peripherals.PSRAM, esp_hal::psram);
    esp_alloc::heap_allocator!(#[esp_hal::ram(reclaimed)] size: 73744);
    //  esp_alloc::heap_allocator!(size: 73 * 1024);

    // Setup software interrupts for executors
    let sw_int = SoftwareInterruptControl::new(peripherals.SW_INTERRUPT);

    let timg0 = TimerGroup::new(peripherals.TIMG0);
    // Note: On Xtensa, esp_rtos::start doesn't take a software interrupt parameter
    esp_rtos::start(timg0.timer0);

    info!("Embassy initialized!");

    // -- Storage setup --
    let storage = Storage::new(peripherals.FLASH, peripherals.SHA).expect("Failed to initialize storage");
    globals::init_storage(storage).await;

    // Set ssid and pw on first compile, until configuration via UI is possible
    // globals::with_storage(|storage| {
    //     storage.config_set("ssid", "").expect("Failed to write config");
    //     storage.config_set("pw", "").expect("Failed to write config");
    // }).await;

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

    let wifi_peripheral = peripherals.WIFI;

    let wifi = Wifi::start_station(wifi_peripheral, &spawner, ssid, password);
    wifi.wait_for_connection().await;
    globals::init_network(wifi.stack(), wifi.tls_seed());

    // Test HTTPS client first
    info!("Testing direct HTTPS request...");
    let http_client = globals::http_client();
    let response = http_client
        .get("https://jsonplaceholder.typicode.com/todos/1")
        .await
        .expect("Failed to make GET request");
    match core::str::from_utf8(&response) {
        Ok(s) => info!("Direct HTTPS Response: {}", s),
        Err(_) => info!(
            "Direct HTTPS Response: [binary data, {} bytes]",
            response.len()
        ),
    }

    // -- Spawn HTTP handler task for widget runtime --
    spawner
        .spawn(runtime::http_sync::http_handler_task())
        .expect("Failed to spawn HTTP handler task");
    info!("HTTP handler task spawned on core0 executor");

    // -- Init and start widget runner on second core --
    static APP_CORE_STACK: StaticCell<CoreStack<32768>> = StaticCell::new();
    let app_core_stack = APP_CORE_STACK.init(CoreStack::new());

    esp_rtos::start_second_core(
        peripherals.CPU_CTRL,
        sw_int.software_interrupt0,
        sw_int.software_interrupt1,
        app_core_stack,
        || {
            static CORE1_EXECUTOR: StaticCell<Executor> = StaticCell::new();
            let executor = CORE1_EXECUTOR.init(Executor::new());

            executor.run(|core1_spawner| {
                core1_spawner
                    .spawn(widget_runner())
                    .expect("Failed to spawn widget runner on core1");
                info!("Widget runner task spawned on core1");
            });
        },
    );
    // -- Server setup --
    http_server::start(wifi.stack(), wifi.tls_seed(), &spawner);

    // TODO: Spawn some tasks
    let _ = spawner;

    loop {
        info!("Hello world!");
        Timer::after(Duration::from_secs(10)).await;
    }

    // for inspiration have a look at the examples at https://github.com/esp-rs/esp-hal/tree/esp-hal-v1.0.0/examples
}

// This is a sample function that will be remove in the final version and replaced by the renderer
#[embassy_executor::task]
async fn widget_runner() {
    info!("Widget runner task started");

    // Initialize Wasmtime runtime
    info!("Initializing Wasmtime runtime");
    let mut runtime = runtime::Runtime::new();
    unsafe {
        let component = runtime
            .load_module(include_bytes!("runtime/test_widget_new.compiled"))
            .expect("Failed to load WASM module");
        let widget = runtime
            .instantiate(&component)
            .expect("Failed to instantiate component");
        let name = runtime
            .get_widget_name(&widget)
            .expect("Failed to get widget name");
        info!("Widget name: {}", name.as_str());
        let config = runtime
            .get_config_schema(&widget)
            .expect("Failed to get config schema");

        info!("Starting widget execution...");
        runtime.run(&widget, config).expect("Failed to run widget");
        info!("Widget execution completed");
    }
}
