use crate::util::globals;
use core::net::Ipv4Addr;
use core::str::FromStr;
use alloc::string::ToString;
use defmt::{info, warn};
use embassy_executor::Spawner;
use embassy_net::{Ipv4Cidr, Runner, Stack, StackResources, StaticConfigV4};
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_hal::rng::Rng;
use esp_hal::system::software_reset;
use esp_radio::wifi::{
    AccessPointConfig, ClientConfig, ModeConfig, WifiController, WifiDevice, WifiEvent,
    WifiStaState,
};

const AP_GATEWAY_IP: &str = "192.168.2.1";
const MAX_STATION_CONNECT_RETRIES: u8 = 8;

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
        use_ap: bool,
    ) -> Self {
        // init radio wifi
        let radio_init = &*mk_static!(
            esp_radio::Controller<'static>,
            esp_radio::init().expect("Failed to initialize Wi-Fi/BLE controller")
        );
        let (mut controller, interfaces) =
            esp_radio::wifi::new(radio_init, wifi_peripheral, Default::default()).unwrap();

        let rng = Rng::new();
        let net_seed = rng.random() as u64 | ((rng.random() as u64) << 32);
        let tls_seed = rng.random() as u64 | ((rng.random() as u64) << 32);

        let (wifi_interface, mode_config, net_config, log_msg) = if use_ap {
            // Access Point mode
            (
                interfaces.ap,
                ModeConfig::AccessPoint(AccessPointConfig::default().with_ssid("WG Display AP".to_string())),
                embassy_net::Config::ipv4_static(StaticConfigV4 {
                    address: Ipv4Cidr::new(Ipv4Addr::new(192, 168, 2, 1), 24),
                    gateway: Some(Ipv4Addr::new(192, 168, 2, 1)),
                    dns_servers: Default::default(),
                }),
                "WiFi configured in AP mode",
            )
        } else {
            // Station mode
            (
                interfaces.sta,
                ModeConfig::Client(
                    ClientConfig::default()
                        .with_ssid(ssid.clone())
                        .with_password(password),
                ),
                embassy_net::Config::dhcpv4(Default::default()),
                "WiFi configured in station mode",
            )
        };

        // Init network stack
        let (stack, runner) = embassy_net::new(
            wifi_interface,
            net_config,
            mk_static!(StackResources<8>, StackResources::<8>::new()),
            net_seed,
        );

        // Configure WiFi with selected mode
        controller.set_config(&mode_config).unwrap();
        info!("{}", log_msg);

        // Spawn wifi connection tasks
        if use_ap {
            spawner.spawn(connection_ap(controller)).ok();
            spawner.spawn(run_dhcp(stack, AP_GATEWAY_IP)).ok();
        } else {
            spawner.spawn(connection(controller)).ok();
        }
        spawner.spawn(net_task(runner)).ok();

        Self { stack, tls_seed }
    }

    pub fn stack(&self) -> Stack<'static> {
        self.stack
    }

    pub fn tls_seed(&self) -> u64 {
        self.tls_seed
    }

    pub async fn wait_for_connection(&self) -> Ipv4Cidr {
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
                return config.address;
            }
            Timer::after(Duration::from_millis(500)).await;
        }
    }
}

// Task to handle Wifi connection and reconnection
#[embassy_executor::task]
async fn connection(mut controller: WifiController<'static>) {
    info!("start connection task");
    let mut failed_connect_attempts: u8 = 0;

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
        }
        info!("About to connect...");

        globals::console_println("Connecting to WiFi").await;

        match controller.connect_async().await {
            Ok(_) => {
                failed_connect_attempts = 0;
                info!("Wifi connected!");
            }
            Err(e) => {
                failed_connect_attempts = failed_connect_attempts.saturating_add(1);
                warn!("Failed to connect to wifi: {:?}", e);

                if failed_connect_attempts >= MAX_STATION_CONNECT_RETRIES {
                    warn!(
                        "Failed to connect {} times, switching to AP mode and rebooting",
                        MAX_STATION_CONNECT_RETRIES
                    );

                    globals::console_println("Failed to connect, rebooting in AP mode").await;

                    let mode_set =
                        globals::with_storage(|storage| storage.config_set("wifi_mode", "ap"))
                            .await;

                    if mode_set.is_ok() {
                        info!("Rebooting into AP mode...");
                        Timer::after(Duration::from_millis(250)).await;
                        software_reset();
                    }

                    warn!("Failed to persist AP fallback mode; continuing retries");
                }

                Timer::after(Duration::from_millis(5000)).await
            }
        }
    }
}

#[embassy_executor::task]
async fn run_dhcp(stack: Stack<'static>, gw_ip_addr: &'static str) {
    use core::net::{Ipv4Addr, SocketAddrV4};

    use edge_dhcp::{
        io::{self, DEFAULT_SERVER_PORT},
        server::{Server, ServerOptions},
    };
    use edge_nal::UdpBind;
    use edge_nal_embassy::{Udp, UdpBuffers};

    let ip = Ipv4Addr::from_str(gw_ip_addr).expect("dhcp task failed to parse gw ip");

    let mut buf = [0u8; 1500];

    let mut gw_buf = [Ipv4Addr::UNSPECIFIED];

    let buffers = UdpBuffers::<3, 1024, 1024, 10>::new();
    let unbound_socket = Udp::new(stack, &buffers);
    let mut bound_socket = unbound_socket
        .bind(core::net::SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::UNSPECIFIED,
            DEFAULT_SERVER_PORT,
        )))
        .await
        .unwrap();

    loop {
        _ = io::server::run(
            &mut Server::<_, 64>::new_with_et(ip),
            &ServerOptions::new(ip, Some(&mut gw_buf)),
            &mut bound_socket,
            &mut buf,
        )
        .await
        .inspect_err(|_| warn!("DHCP server error"));
        Timer::after(Duration::from_millis(500)).await;
    }
}

#[embassy_executor::task]
async fn connection_ap(mut controller: WifiController<'static>) {
    info!("start AP connection task");

    if !matches!(controller.is_started(), Ok(true)) {
        info!("Starting WiFi AP");
        controller.start_async().await.unwrap();
        info!("WiFi AP started");
    }

    loop {
        let events = controller
            .wait_for_events(
                WifiEvent::ApStaConnected | WifiEvent::ApStaDisconnected,
                true,
            )
            .await;

        if events.contains(WifiEvent::ApStaConnected) {
            info!("Station connected to AP");
        }

        if events.contains(WifiEvent::ApStaDisconnected) {
            info!("Station disconnected from AP");
        }

        Timer::after(Duration::from_millis(200)).await;
    }
}

#[embassy_executor::task]
async fn net_task(mut runner: Runner<'static, WifiDevice<'static>>) {
    runner.run().await
}
