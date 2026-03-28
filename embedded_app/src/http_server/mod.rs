use alloc::format;
use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::{Duration, Timer};
use esp_alloc as _;
use esp_hal::system::software_reset;
use picoserve::{
    AppBuilder, AppRouter, Router,
    extract::JsonWithUnescapeBufferSize,
    response::{File, IntoResponse, Json},
    routing::{self, parse_path_segment},
};

use crate::widget::manager::WidgetManager;
use crate::{util::globals, widget::store::WidgetStore};
use common::models::WidgetStoreItem;
use common::models::{InstallAction, SystemConfiguration, WifiCredentials, WifiModeResponse};

mod custom_types;
mod frontend;

use custom_types::{ConfigWrapper, Error, HandlerResult, HtmlResponse, JsonStringResponse};

pub const WEB_TASK_POOL_SIZE: usize = 2;
const TCP_BUFFER_SIZE: usize = 8192;
const HTTP_BUFFER_SIZE: usize = 16384;
const INDEX_CACHE_HEADER: (&str, &str) = (
    "Cache-Control",
    "no-cache, no-store, must-revalidate",
);
const ASSET_HEADER: (&str, &str) = (
    "Cache-Control",
    "no-cache, must-revalidate",
);

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route("/get_store_items", routing::get(get_store_items))
            .route("/install_widget", routing::post(post_install_widget))
            .route("/wifi_mode", routing::get(get_wifi_mode))
            .route(
                "/wifi_credentials",
                routing::post(post_wifi_credentials),
            )
            .route(
                "/system_config",
                routing::get(get_system_config).post(post_system_config),
            )
            // "/deinstall_widget/<widget_name>"
            .route(
                (
                    "/deinstall_widget",
                    parse_path_segment::<alloc::string::String>(),
                ),
                routing::get(deinstall_widget),
            )
            // "/config_schema/<widget_name>"
            .route(
                (
                    "/config_schema",
                    parse_path_segment::<alloc::string::String>(),
                ),
                routing::get(get_config_schema),
            )
            // "/widget_config/<widget_name>"
            .route(
                (
                    "/widget_config",
                    parse_path_segment::<alloc::string::String>(),
                ),
                routing::post(post_widget_config),
            )
            // "/widget_configuration/<widget_name>"
            .route(
                (
                    "/widget_configuration",
                    parse_path_segment::<alloc::string::String>(),
                ),
                routing::get(get_widget_config),
            )
            // routes to serve frontend files
            .route(
                "/",
                routing::get_service(File::with_content_type_and_headers(
                    "text/html",
                    frontend::INDEX_HTML.as_bytes(),
                    &[INDEX_CACHE_HEADER],
                )),
            )
            .route(
                "/frontend.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::FRONTEND_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/frontend_bg.wasm",
                routing::get_service(File::with_content_type_and_headers(
                    "application/wasm",
                    frontend::FRONTEND_WASM_GZ,
                    &[("Content-Encoding", "gzip"), ASSET_HEADER],
                )),
            )
            .route(
                "/output.css",
                routing::get_service(File::with_content_type_and_headers(
                    "text/css",
                    frontend::OUTPUT_CSS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/logo.png",
                routing::get_service(File::with_content_type_and_headers(
                    "image/png",
                    frontend::LOGO_PNG,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/css/bootstrap.css",
                routing::get_service(File::with_content_type_and_headers(
                    "text/css",
                    frontend::BOOTSTRAP_CSS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/js/jquery.min.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JQUERY_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/js/underscore.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::UNDERSCORE_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform-defaults.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_DEFAULTS_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform-split.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_SPLIT_JS,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/html/widget_config.html",
                routing::get_service(File::html(frontend::WIDGET_CONFIG_HTML)),
            )
    }
}

// TODO: create WidetStore instance in globals and init the store on boot that unnecessary wait time can be avoided
async fn get_store_items() -> HandlerResult<JsonStringResponse> {
    let mut store = WidgetStore::new();
    store
        .fetch_from_store()
        .await
        .map_err(|e| Error::new(format!("Failed to fetch widget store: {:?}", e)))?;
    let json = serde_json::to_string(store.get_items())
        .map_err(|_| Error::new("Failed to serialize widget store"))?;
    info!("Serving store items: {}", json.as_str());
    Ok(JsonStringResponse(json))
}

async fn get_system_config() -> HandlerResult<Json<SystemConfiguration>> {
    match globals::with_storage(|storage| storage.get_system_config()).await {
        Ok(config) => Ok(Json(config)),
        Err(_) => {
            // try to create default config
            let default_config = SystemConfiguration::default();
            globals::with_storage(|storage| storage.save_system_config(&default_config))
                .await
                .map_err(|e| {
                    Error::new(format!("Failed to save default system config: {:?}", e))
                })?;
            Ok(Json(default_config))
        }
    }
}

async fn get_wifi_mode() -> HandlerResult<Json<WifiModeResponse>> {
    let mode = globals::with_storage(|storage| storage.config_get("wifi_mode"))
        .await
        .unwrap_or_else(|_| alloc::string::String::from("ap"));

    Ok(Json(WifiModeResponse {
        is_ap_mode: mode == "ap",
    }))
}

async fn post_wifi_credentials(
    Json(credentials): Json<WifiCredentials>,
) -> HandlerResult<()> {
    let ssid = credentials.ssid;
    let password = credentials.password;

    if ssid.trim().is_empty() {
        return Err(Error::new("SSID must not be empty"));
    }

    info!("Received WiFi credentials for SSID '{}'", ssid.as_str());

    globals::with_storage(|storage| {
        storage.config_set("ssid", ssid.as_str())?;
        storage.config_set("pw", password.as_str())?;
        storage.config_set("wifi_mode", "station")?;
        Ok::<(), crate::storage::StorageError>(())
    })
    .await
    .map_err(|e| Error::new(format!("Failed to save WiFi credentials: {:?}", e)))?;

    info!("Rebooting device due to Wifi config change");
    Timer::after(Duration::from_millis(250)).await;
    software_reset();
}

async fn post_install_widget(Json(action): Json<InstallAction>) -> HandlerResult<()> {
    let (download_url, description) = match action {
        InstallAction::FromUrl(url) => (url, alloc::string::String::from("No description")),
        InstallAction::FromStoreItemName(name) => {
            let mut store = WidgetStore::new();
            store
                .fetch_from_store()
                .await
                .map_err(|e| Error::new(format!("Failed to fetch widget store: {:?}", e)))?;
            let item = store
                .get_items()
                .iter()
                .find(|item: &&WidgetStoreItem| item.name == name)
                .ok_or_else(|| Error::new(format!("Widget '{}' not found", name)))?;
            (item.get_download_url(), item.description.clone())
        }
    };
    info!("Installing widget from URL {}", download_url.as_str());
    WidgetManager::install_widget(download_url.as_str(), &description)
        .await
        .map_err(|e| Error::new(format!("Failed to install widget: {:?}", e)))?;
    Ok(())
}

async fn post_system_config(
    JsonWithUnescapeBufferSize(config): JsonWithUnescapeBufferSize<
        SystemConfiguration,
        HTTP_BUFFER_SIZE,
    >,
) -> HandlerResult<()> {
    info!(
        "Received new system config: {:?}",
        defmt::Debug2Format(&config)
    );
    globals::with_storage(|storage| storage.save_system_config(&config))
        .await
        .map_err(|e| Error::new(format!("Failed to save system config: {:?}", e)))
}

async fn deinstall_widget(widget_name: alloc::string::String) -> HandlerResult<()> {
    WidgetManager::deinstall_widget(widget_name.as_str())
        .await
        .map_err(|e| Error::new(format!("Failed to deinstall widget: {:?}", e)))
}

async fn get_config_schema(
    widget_name: alloc::string::String,
) -> HandlerResult<JsonStringResponse> {
    let system_config = globals::with_storage(|storage| storage.get_system_config())
        .await
        .map_err(|e| Error::new(format!("Failed to get system config: {:?}", e)))?;

    let widget = system_config
        .widgets
        .iter()
        .find(|w| w.name == widget_name.as_str())
        .ok_or_else(|| Error::new(format!("Widget '{}' not found", widget_name.as_str())))?;

    let config = widget.json_config_schema.clone();

    serde_json::from_str::<serde_json::Value>(&config)
        .map_err(|_| Error::new("Widget config schema is not valid JSON"))?;

    Ok(JsonStringResponse(config))
}

async fn post_widget_config(
    widget_name: alloc::string::String,
    JsonWithUnescapeBufferSize(config): JsonWithUnescapeBufferSize<ConfigWrapper, HTTP_BUFFER_SIZE>,
) -> HandlerResult<()> {
    info!(
        "POST /widget_config/{} - received config",
        widget_name.as_str()
    );

    let config_string = config.config;

    let mut system_config = globals::with_storage(|storage| storage.get_system_config())
        .await
        .map_err(|e| Error::new(format!("Failed to get system config: {:?}", e)))?;

    if let Some(widget) = system_config
        .widgets
        .iter_mut()
        .find(|w| w.name == widget_name.as_str())
    {
        info!("Updating widget config for: {}", widget_name.as_str());
        widget.json_config = config_string;
    } else {
        error!("Widget not found: {}", widget_name.as_str());
    }

    globals::with_storage(|storage| storage.save_system_config(&system_config))
        .await
        .map_err(|e| Error::new(format!("Failed to save widget config: {:?}", e)))
}

async fn get_widget_config(widget_name: alloc::string::String) -> impl IntoResponse {
    info!(
        "Serving widget configuration page for: {}",
        widget_name.as_str()
    );
    let html = frontend::WIDGET_CONFIG_HTML.replace("{{WIDGET_NAME}}", widget_name.as_str());
    HtmlResponse(html)
}

pub struct WebApp {
    pub router: &'static Router<<Application as AppBuilder>::PathRouter>,
    pub config: &'static picoserve::Config,
}

impl Default for WebApp {
    fn default() -> Self {
        let router = picoserve::make_static!(AppRouter<Application>, Application.build_app());

        let config = picoserve::make_static!(
            picoserve::Config,
            picoserve::Config::new(picoserve::Timeouts {
                start_read_request: Duration::from_secs(5),
                read_request: Duration::from_secs(5),
                write: Duration::from_secs(15),
                persistent_start_read_request: Duration::from_secs(5),
            })
            .keep_connection_alive()
        );

        Self { router, config }
    }
}

pub fn start(stack: Stack<'static>, _tls_seed: u64, spawner: &Spawner) {
    info!(
        "Starting web server with {} concurrent tasks...",
        WEB_TASK_POOL_SIZE
    );

    let web_app = WebApp::default();

    // Spawn multiple web tasks to handle concurrent connections
    for task_id in 0..WEB_TASK_POOL_SIZE {
        spawner.must_spawn(web_task(task_id, stack, web_app.router, web_app.config));
    }

    info!(
        "Web server started on port 80 with {} tasks",
        WEB_TASK_POOL_SIZE
    );
}

#[embassy_executor::task(pool_size = WEB_TASK_POOL_SIZE)]
pub async fn web_task(
    task_id: usize,
    stack: Stack<'static>,
    router: &'static AppRouter<Application>,
    config: &'static picoserve::Config,
) -> ! {
    let port = 80;
    // Useing vec![] to avoid stack temporaries
    let mut tcp_rx_buffer = alloc::vec![0u8; TCP_BUFFER_SIZE].into_boxed_slice();
    let mut tcp_tx_buffer = alloc::vec![0u8; TCP_BUFFER_SIZE].into_boxed_slice();
    let mut http_buffer = alloc::vec![0u8; HTTP_BUFFER_SIZE].into_boxed_slice();

    picoserve::Server::new(router, config, &mut http_buffer)
        .listen_and_serve(task_id, stack, port, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}
