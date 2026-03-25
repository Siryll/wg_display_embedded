//! WiFi station mode driver.
//!
//! Call [`Wifi::start_station`] once from `main()` to configure the radio,
//! spawn the connection and network tasks, then await
//! [`Wifi::wait_for_connection`] before starting any network-dependent tasks.
use defmt::{debug, info, warn};
use embassy_executor::Spawner;
use embassy_net::{Runner, Stack, StackResources};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_hal::rng::Rng;
use esp_radio::wifi::{
    ClientConfig, ModeConfig, ScanConfig, WifiController, WifiDevice, WifiEvent, WifiStaState,
};

#[allow(
    clippy::large_stack_frames,
    reason = "wifi module is allowd to have large stack frames"
)]
// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write($val);
        x
    }};
}

/// Struct to hold relevenat states for other modules (http server/client)
pub struct Wifi {
    stack: Stack<'static>,
    tls_seed: u64,
}

impl Wifi {
    /// Initialises the WiFi radio.
    ///
    /// Spawns two embassy tasks:
    /// - `connection()` — connects to the AP and reconnects on disconnect (5 s retry)
    /// - `net_task()` — runs the smoltcp network stack
    ///
    /// Call [`wait_for_connection`](Self::wait_for_connection) to wait until connection is fully established.
    pub fn start_station(
        wifi_peripheral: esp_hal::peripherals::WIFI<'static>,
        spawner: &Spawner,
        ssid: alloc::string::String,
        password: alloc::string::String,
    ) -> Self {
        // init radio wifi
        let radio_init = &*mk_static!(
            esp_radio::Controller<'static>,
            esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
        );
        let (mut controller, interfaces) =
            esp_radio::wifi::new(radio_init, wifi_peripheral, Default::default()).unwrap();
        // Set station mode
        let wifi_interface = interfaces.sta;
        // Init dhcp config
        let config = embassy_net::Config::dhcpv4(Default::default());

        let rng = Rng::new();
        let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
        let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

        // Init network stack
        let (stack, runner) = embassy_net::new(
            wifi_interface,
            config,
            mk_static!(StackResources<8>, StackResources::<8>::new()),
            net_seed,
        );

        // configure wifi with credentials
        let station_config = ModeConfig::Client(
            ClientConfig::default()
                .with_ssid(ssid)
                .with_password(password),
        );
        controller.set_config(&station_config).unwrap();
        info!("Wifi configured");

        // spawn wifi connection tasks
        spawner.spawn(connection(controller)).ok();
        spawner.spawn(net_task(runner)).ok();

        Self { stack, tls_seed }
    }

    /// Returns the embassy-net stack. Pass to [`EspHttpClient::new`](crate::http_client::EspHttpClient::new).
    pub fn stack(&self) -> Stack<'static> {
        self.stack
    }

    /// Returns the TLS seed. Pass to [`EspHttpClient::new`](crate::http_client::EspHttpClient::new).
    pub fn tls_seed(&self) -> u64 {
        self.tls_seed
    }

    /// Waits asynchronously until the WiFi link is up.
    pub async fn wait_for_connection(&self) {
        info!("Waiting for link to be up");
        loop {
            if self.stack.is_link_up() {
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }

        info!("Waiting to get IP address...");
        loop {
            if let Some(config) = self.stack.config_v4() {
                info!("Got IP: {}", config.address);
                break;
            }
            Timer::after(Duration::from_millis(500)).await;
        }
    }
}

// Task to handle Wifi connection and reconnection
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("start connection task");
    loop {
        if esp_radio::wifi::sta_state() == WifiStaState::Connected {
            // wait until we're no longer connected
            controller.wait_for_event(WifiEvent::StaDisconnected).await;
            Timer::after(Duration::from_millis(5000)).await
        }
        if !matches!(controller.is_started(), Ok(true)) {
            info!("Starting wifi");
            controller.start_async().await.unwrap();
            info!("Wifi started!");

            debug!("Scan");
            let scan_config = ScanConfig::default().with_max(10);
            let result = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            for ap in result {
                debug!("{:?}", ap);
            }
        }
        info!("About to connect...");

        match controller.connect_async().await {
            Ok(_) => info!("Wifi connected!"),
            Err(e) => {
                warn!("Failed to connect to wifi: {:?}", e);
                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
