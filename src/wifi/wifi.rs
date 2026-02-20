use embassy_executor::Spawner;
use embassy_time::{Duration, Timer};
use embassy_net::{
    Runner, Stack, StackResources,
};
use esp_alloc as _;
use defmt::info;
use esp_hal::rng::Rng;
use esp_radio::wifi::{
    ClientConfig, ModeConfig, WifiDevice, WifiController, WifiEvent, ScanConfig
};

// When you are okay with using a nightly compiler it's better to use https://docs.rs/static_cell/2.1.0/static_cell/macro.make_static.html
macro_rules! mk_static {
    ($t:ty,$val:expr) => {{
        static STATIC_CELL: static_cell::StaticCell<$t> = static_cell::StaticCell::new();
        #[deny(unused_attributes)]
        let x = STATIC_CELL.uninit().write(($val));
        x
    }};
}

// Struct to hold relevenat states for other modules (http server/client)
pub struct Wifi {
    stack: Stack<'static>,
    tls_seed: u64,
}

impl Wifi {
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
        let (mut controller, interfaces) = esp_radio::wifi::new(radio_init, wifi_peripheral, Default::default()).unwrap();  
        // Set station mode
        let wifi_interface = interfaces.sta;
        // Init dhcp config
        let config = embassy_net::Config::dhcpv4(Default::default()); 

        let rng = Rng::new();
        let seed = (rng.random() as u64) << 32 | rng.random() as u64;

        // Init network stack
        let (stack, runner) = embassy_net::new(
            wifi_interface,
            config,
            mk_static!(StackResources<3>, StackResources::<3>::new()),
            seed,
        );

        // configure wifi with credentials
        let station_config = ModeConfig::Client(
            ClientConfig::default()
                .with_ssid(ssid.into())
                .with_password(password.into()),
        );
        controller.set_config(&station_config).unwrap();
        info!("Wifi configured");

        // spawn wifi connection tasks
        spawner.spawn(connection(controller)).ok();
        spawner.spawn(net_task(runner)).ok();

        Self {
            stack,
            tls_seed: seed,
        }
    }

    pub async fn wait_for_connection(&self) {
        self.stack.wait_config_up().await;
        if let Some(config) = self.stack.config_v4() {
            info!("Got IP: {}", config.address);
        }
    }
}

// Task to handle Wifi connection and reconnection
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("start connection task");

    loop {
        if !matches!(controller.is_started(), Ok(true)) {
            info!("Starting wifi");
            controller.start_async().await.unwrap();
            info!("Wifi started!");

            info!("Scan");
            let scan_config = ScanConfig::default().with_max(10);
            let result = controller
                .scan_with_config_async(scan_config)
                .await
                .unwrap();
            for ap in result {
                info!("{:?}", ap);
            }
        }
        
        info!("About to connect...");

        match controller.connect_async().await {
            Ok(info) => {
                info!("Wifi connected to {:?}", info);

                // wait until we're no longer connected
                let disconnect_reason = controller.wait_for_event(WifiEvent::StaDisconnected).await;
                info!("Disconnected: {:?}", disconnect_reason);
            }
            Err(e) => {
                info!("Failed to connect to wifi: {:?}", e);
            }
        }

        Timer::after(Duration::from_millis(5000)).await
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}