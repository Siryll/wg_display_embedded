//! Web UI and REST API server using [picoserve](https://github.com/sammhicks/picoserve).
//!
//! ## Configuration
//!
//! [`WEB_TASK_POOL_SIZE`] sets the number of concurrent web task. Greatly increased RAM usage with each task.
//! [`TCP_BUFFER_SIZE`] sets the size of the TCP buffer for each connection.
//! [`HTTP_BUFFER_SIZE`] sets the size of the HTTP buffer. This greatly impact the upload speed of assets and files.
//!
//! ## Routed Endpoints
//!
//! | Route | Method | Backing handler or asset |
//! |---|---|---|
//! | `/get_store_items` | GET | [`get_store_items`] |
//! | `/install_widget` | POST | [`post_install_widget`] |
//! | `/wifi_mode` | GET | [`get_wifi_mode`] |
//! | `/wifi_credentials` | POST | [`post_wifi_credentials`] |
//! | `/system_config` | GET | [`get_system_config`] |
//! | `/system_config` | POST | [`post_system_config`] |
//! | `/deinstall_widget/<widget_name>` | GET | [`deinstall_widget`] |
//! | `/config_schema/<widget_name>` | GET | [`get_config_schema`] |
//! | `/widget_config/<widget_name>` | POST | [`post_widget_config`] |
//! | `/widget_configuration/<widget_name>` | GET | [`get_widget_config`] |
//! | `/` | GET | [`frontend::INDEX_HTML`]  |
//! | `/frontend.js` | GET | [`frontend::FRONTEND_JS`]  |
//! | `/frontend_bg.wasm` | GET | [`frontend::FRONTEND_WASM_GZ`]  |
//! | `/output.css` | GET | [`frontend::OUTPUT_CSS`]  |
//! | `/assets/logo.png` | GET | [`frontend::LOGO_PNG`]  |
//! | `/assets/css/bootstrap.css` | GET | [`frontend::BOOTSTRAP_CSS`]  |
//! | `/assets/js/jquery.min.js` | GET | [`frontend::JQUERY_JS`]  |
//! | `/assets/js/underscore.js` | GET | [`frontend::UNDERSCORE_JS`]  |
//! | `/assets/js/jsonform.js` | GET | [`frontend::JSONFORM_JS`]  |
//! | `/assets/js/jsonform-defaults.js` | GET | [`frontend::JSONFORM_DEFAULTS_JS`]  |
//! | `/assets/js/jsonform-split.js` | GET | [`frontend::JSONFORM_SPLIT_JS`]  |
//! | `/assets/html/widget_config.html` | GET | [`frontend::WIDGET_CONFIG_HTML`]  |
//! | `/assets/fonts/glyphicons-halflings-regular.eot` | GET | [`frontend::FONT_GLYPHS_EOT`]  |
//! | `/assets/fonts/glyphicons-halflings-regular.svg` | GET | [`frontend::FONT_GLYPHS_SVG`]  |
//! | `/assets/fonts/glyphicons-halflings-regular.ttf` | GET | [`frontend::FONT_GLYPHS_TTF`]  |
//! | `/assets/fonts/glyphicons-halflings-regular.woff` | GET | [`frontend::FONT_GLYPHS_WOFF`]  |
//! | `/assets/fonts/glyphicons-halflings-regular.woff2` | GET | [`frontend::FONT_GLYPHS_WOFF2`]  |
//!
//!
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
    response::{File, Json},
    routing::{self, parse_path_segment},
};

use crate::util::globals;
use crate::widget::manager::WidgetManager;
use common::models::WidgetStoreItem;
use common::models::{InstallAction, SystemConfiguration, WifiCredentials, WifiModeResponse};

mod custom_types;
mod frontend;

use custom_types::{ConfigWrapper, Error, HandlerResult, JsonStringResponse};

pub const WEB_TASK_POOL_SIZE: usize = 5;
const TCP_BUFFER_SIZE: usize = 8192;
const HTTP_BUFFER_SIZE: usize = 16384;
const INDEX_CACHE_HEADER: (&str, &str) = ("Cache-Control", "no-cache, no-store, must-revalidate");
/// Asset http headers
const ASSET_HEADER: (&str, &str) = ("Cache-Control", "public, max-age=86400, immutable");

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    /// creates all routes, including static frontend assets
    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route("/get_store_items", routing::get(get_store_items))
            .route("/install_widget", routing::post(post_install_widget))
            .route("/wifi_mode", routing::get(get_wifi_mode))
            .route("/wifi_credentials", routing::post(post_wifi_credentials))
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
                routing::get_service(File::with_content_type_and_headers(
                    "text/html",
                    frontend::WIDGET_CONFIG_HTML,
                    &[INDEX_CACHE_HEADER],
                )),
            )
            // routes to serve frontend files
            .route(
                "/",
                routing::get_service(File::with_content_type_and_headers(
                    "text/html",
                    frontend::INDEX_HTML,
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
            // .route(
            //     "/assets/html/widget_config.html",
            //     routing::get_service(File::with_content_type_and_headers(
            //         "text/html",
            //         frontend::WIDGET_CONFIG_HTML,
            //         &[INDEX_CACHE_HEADER],
            //     )),
            // )
            .route(
                "/assets/fonts/glyphicons-halflings-regular.eot",
                routing::get_service(File::with_content_type_and_headers(
                    "application/vnd.ms-fontobject",
                    frontend::FONT_GLYPHS_EOT,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/fonts/glyphicons-halflings-regular.svg",
                routing::get_service(File::with_content_type_and_headers(
                    "image/svg+xml",
                    frontend::FONT_GLYPHS_SVG,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/fonts/glyphicons-halflings-regular.ttf",
                routing::get_service(File::with_content_type_and_headers(
                    "font/ttf",
                    frontend::FONT_GLYPHS_TTF,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/fonts/glyphicons-halflings-regular.woff",
                routing::get_service(File::with_content_type_and_headers(
                    "font/woff",
                    frontend::FONT_GLYPHS_WOFF,
                    &[ASSET_HEADER],
                )),
            )
            .route(
                "/assets/fonts/glyphicons-halflings-regular.woff2",
                routing::get_service(File::with_content_type_and_headers(
                    "font/woff2",
                    frontend::FONT_GLYPHS_WOFF2,
                    &[ASSET_HEADER],
                )),
            )
    }
}

// TODO: create WidetStore instance in globals and init the store on boot that unnecessary wait time can be avoided
/// gets and returns all widget store items as JSON.
async fn get_store_items() -> HandlerResult<JsonStringResponse> {
    let json = serde_json::to_string(&globals::get_store_items().await)
        .map_err(|_| Error::new("Failed to serialize widget store"))?;
    info!("Serving store items: {}", json.as_str());
    Ok(JsonStringResponse(json))
}

/// gets and returns the currently stored system config, create a new default config if none is present (first boot)
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

/// gets the current wifi mode (AP or Station) to let the frontend decide which components to show
async fn get_wifi_mode() -> HandlerResult<Json<WifiModeResponse>> {
    let mode = globals::with_storage(|storage| storage.config_get("wifi_mode"))
        .await
        .unwrap_or_else(|_| alloc::string::String::from("ap"));

    Ok(Json(WifiModeResponse {
        is_ap_mode: mode == "ap",
    }))
}

/// receives wifi credentials from the frontend, saves them to storage and reboots
async fn post_wifi_credentials(Json(credentials): Json<WifiCredentials>) -> HandlerResult<()> {
    let ssid = credentials.ssid.clone();

    if ssid.trim().is_empty() {
        return Err(Error::new("SSID must not be empty"));
    }

    info!("Received WiFi credentials for SSID '{}'", ssid.as_str());

    globals::with_storage(|storage| {
        storage.set_wifi_credentials_and_mode(credentials, "station")?;
        Ok::<(), crate::storage::StorageError>(())
    })
    .await
    .map_err(|e| Error::new(format!("Failed to save WiFi credentials: {:?}", e)))?;

    info!("Rebooting device due to Wifi config change");
    Timer::after(Duration::from_millis(250)).await;
    software_reset();
}

/// Installs a widget from a given URL or from the widget store, determined by the `InstallAction` payload.
async fn post_install_widget(Json(action): Json<InstallAction>) -> HandlerResult<()> {
    let (download_url, description) = match action {
        InstallAction::FromUrl(url) => (url, alloc::string::String::from("No description")),
        InstallAction::FromStoreItemName(name) => {
            let store_items = globals::get_store_items().await;
            let item = store_items
                .iter()
                .find(|item: &&WidgetStoreItem| item.name == name)
                .ok_or_else(|| Error::new(format!("Widget '{}' not found", name)))?;
            (item.get_download_url(), item.description.clone())
        }
    };
    info!("Installing widget from URL {}", download_url.as_str());
    WidgetManager::install_widget(download_url.as_str(), &description)
        .await
        .map_err(|e| Error::new(format!("Failed to install widget: {:?}", e)))
}

/// receives an updated system configuration from the frontend and saves it to NVS. The config is only saved if there are changes to avoid flash wear.
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

/// Remove widget from system config and storage
async fn deinstall_widget(widget_name: alloc::string::String) -> HandlerResult<()> {
    WidgetManager::deinstall_widget(widget_name.as_str())
        .await
        .map_err(|e| Error::new(format!("Failed to deinstall widget: {:?}", e)))
}

/// gets the JSON config schema for a given widget
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

/// update the JSON config for a given widget
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
        return Err(Error::new(format!(
            "Widget '{}' not found",
            widget_name.as_str()
        )));
    }

    globals::with_storage(|storage| storage.save_system_config(&system_config))
        .await
        .map_err(|e| Error::new(format!("Failed to save widget config: {:?}", e)))
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
