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

use defmt::error;
use defmt::info;
use embassy_executor::Spawner;
use embassy_time::{Duration, Instant, Timer};
use esp_hal::clock::CpuClock;
use esp_hal::gpio::{Input, InputConfig, Pull};
use esp_hal::interrupt::software::SoftwareInterruptControl;
use esp_hal::system::Stack as CoreStack;
use esp_hal::system::software_reset;
use esp_hal::timer::timg::TimerGroup;
use esp_println as _;
use esp_rtos::embassy::Executor;
use static_cell::StaticCell;

mod wifi;
use crate::display::DisplayPeripherals;
use crate::wifi::Wifi;

mod util;
mod widget;
use crate::util::globals;

mod display;
use crate::display::Display;

mod runtime;

mod storage;
use crate::storage::Storage;

mod renderer;

mod http_client;

mod http_server;

use crate::alloc::string::ToString;
use crate::util::esptime::EspTime;

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

    software_reset();
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
    let storage =
        Storage::new(peripherals.FLASH, peripherals.SHA).expect("Failed to initialize storage");
    globals::init_storage(storage).await;

    // -- Display setup --
    let display = Display::new(DisplayPeripherals {
        spi2: peripherals.SPI2,
        dma_ch0: peripherals.DMA_CH0,
        gpio4: peripherals.GPIO4,
        gpio5: peripherals.GPIO5,
        gpio6: peripherals.GPIO6,
        gpio7: peripherals.GPIO7,
        gpio47: peripherals.GPIO47,
        gpio48: peripherals.GPIO48,
    });

    globals::init_display(display).await;

    globals::console_println("WG-Display starting up").await;

    // -- Wifi setup --
    let wifi_creds = globals::with_storage(|storage| storage.get_wifi_credentials()).await;

    // check current wifi mode, if nothing is set (first boot) or wifi connection fails this will be set to ap (Access Point) mode
    // otherwise the device will start in station mode and try to connect to the set wifi network, will switch back to ap mode if this fails
    let wifi_mode = globals::with_storage(|storage| storage.config_get("wifi_mode"))
        .await
        .unwrap_or_else(|_| alloc::string::String::from("ap"));
    let force_ap_mode = wifi_mode == "ap";

    let wifi_peripheral = peripherals.WIFI;
    // start in station mode
    if !force_ap_mode {
        // check if wifi credentials are present, otherwise swicht to ap mode
        match wifi_creds {
            Ok(creds) => {
                start_station_mode(wifi_peripheral, &spawner, creds.ssid, creds.password).await;
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
            }
            Err(_) => {
                info!("WiFi credentials not found in storage, starting in AP mode");
                start_ap_mode(wifi_peripheral, &spawner).await;
            }
        }
    } else {
        info!("WiFi mode is set to AP, starting in AP mode");
        start_ap_mode(wifi_peripheral, &spawner).await;
    }

    let boot_button = Input::new(peripherals.GPIO0, InputConfig::default().with_pull(Pull::Up));
    let mut boot_button_timer = 0;

    loop {
        if boot_button.is_low() {
            boot_button_timer += 1;
            if boot_button_timer >= 100 {
                globals::console_println("Boot button held, resetting WiFi settings").await;
                let _ =globals::with_storage(|storage| storage.config_set("wifi_mode", "ap")).await;
                software_reset();
            }
        }

        if boot_button.is_high() && boot_button_timer != 0 {
            boot_button_timer = 0;
        }

        Timer::after(Duration::from_millis(50)).await;
    }
}

async fn start_ap_mode(wifi_peripheral: esp_hal::peripherals::WIFI<'static>, spawner: &Spawner) {
    globals::console_println("No wifi configured, starting in AP mode").await;
    let _ = globals::with_storage(|storage| storage.config_set("wifi_mode", "ap")).await;
    let wifi = Wifi::start_station(wifi_peripheral, spawner, "".into(), "".into(), true);

    // -- Server setup --
    http_server::start(wifi.stack(), wifi.tls_seed(), spawner);
    globals::console_println("Connect to 'WG-Display-AP'").await;
    globals::console_println("and open 192.168.2.1").await;
}

async fn start_station_mode(
    wifi_peripheral: esp_hal::peripherals::WIFI<'static>,
    spawner: &Spawner,
    ssid: alloc::string::String,
    password: alloc::string::String,
) {
    globals::console_println("Starting in station mode").await;
    let _ = globals::with_storage(|storage| storage.config_set("wifi_mode", "station")).await;
    let wifi = Wifi::start_station(wifi_peripheral, spawner, ssid, password, false);
    let ip = wifi.wait_for_connection().await;
    let _ = globals::with_storage(|storage| storage.config_set("device_ip", &ip.to_string())).await;

    // -- Spawn HTTP handler task for widget runtime --
    spawner
        .spawn(runtime::http_sync::http_handler_task(
            wifi.stack(),
            wifi.tls_seed(),
        ))
        .expect("Failed to spawn HTTP handler task");
    info!("HTTP handler task spawned on core0 executor");

    // init widget store
    let mut widget_store = widget::store::WidgetStore::new();
    widget_store
        .fetch_from_store()
        .await
        .expect("Failed to fetch widget store");
    globals::init_store(widget_store).await;

    // -- Server setup --
    http_server::start(wifi.stack(), wifi.tls_seed(), spawner);

    let mut esp_time = EspTime::new();
    esp_time.fetch_time().await;
    globals::init_time(esp_time);
    info!("Global time synced from time API");
}

/// Renderer task look that is spawned on the second core
#[embassy_executor::task]
async fn widget_runner() {
    info!("Widget runner task started");

    let mut renderer = renderer::Renderer::new();
    // will loop forever
    renderer.run().await;
}
