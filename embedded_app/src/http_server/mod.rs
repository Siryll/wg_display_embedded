use defmt::{error, info};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc as _;
use picoserve::{
    AppBuilder, AppRouter, Router,
    response::{File, IntoResponse, Json},
    routing::{self, parse_path_segment},
};

use common::models::WidgetStoreItem;
use crate::widget::manager::WidgetManager;
use common::models::{InstallAction, SystemConfiguration};
use crate::{
    util::globals,
    widget::store::WidgetStore,
};
use crate::runtime::Runtime;
use alloc::string::String;

mod frontend;

pub const WEB_TASK_POOL_SIZE: usize = 2;
const TCP_BUFFER_SIZE: usize = 8192;
const HTTP_BUFFER_SIZE: usize = 16384;
const STATIC_CACHE_HEADER: (&str, &str) = (
    "Cache-Control",
    "public, max-age=3600, stale-while-revalidate=86400",
);

struct HtmlResponse(String);

// implementation for the get_widget_config to avoid memory leak
impl IntoResponse for HtmlResponse {
    async fn write_to<R: picoserve::io::Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        (("Content-Type", "text/html; charset=utf-8"), self.0.as_str())
            .write_to(connection, response_writer)
            .await
    }
}

struct JsonStringResponse(String);

impl IntoResponse for JsonStringResponse {
    async fn write_to<R: picoserve::io::Read, W: picoserve::response::ResponseWriter<Error = R::Error>>(
        self,
        connection: picoserve::response::Connection<'_, R>,
        response_writer: W,
    ) -> Result<picoserve::ResponseSent, W::Error> {
        (("Content-Type", "application/json"), self.0.as_str())
            .write_to(connection, response_writer)
            .await
    }
}

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route("/get_store_items", routing::get(get_store_items))
            .route("/install_widget", routing::post(post_install_widget))
            .route("/system_config", routing::get(get_system_config).post(post_system_config))
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
            // "/widget_configuration/<widget_name>"
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
                routing::get_service(File::html(frontend::INDEX_HTML)),
            )
            .route(
                "/frontend.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::FRONTEND_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/frontend_bg.wasm",
                routing::get_service(File::with_content_type_and_headers(
                    "application/wasm",
                    frontend::FRONTEND_WASM_GZ,
                    &[("Content-Encoding", "gzip"), STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/output.css",
                routing::get_service(File::with_content_type_and_headers(
                    "text/css",
                    frontend::OUTPUT_CSS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/logo.png",
                routing::get_service(File::with_content_type_and_headers(
                    "image/png",
                    frontend::LOGO_PNG,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/css/bootstrap.css",
                routing::get_service(File::with_content_type_and_headers(
                    "text/css",
                    frontend::BOOTSTRAP_CSS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/js/jquery.min.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JQUERY_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/js/underscore.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::UNDERSCORE_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform-defaults.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_DEFAULTS_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/js/jsonform-split.js",
                routing::get_service(File::with_content_type_and_headers(
                    "application/javascript",
                    frontend::JSONFORM_SPLIT_JS,
                    &[STATIC_CACHE_HEADER],
                )),
            )
            .route(
                "/assets/html/widget_config.html",
                routing::get_service(File::html(frontend::WIDGET_CONFIG_HTML)),
            )
    }
}

// TODO: create WidetStore instance in globals and init the store on boot that unnecessary wait time can be avoided
async fn get_store_items() -> impl IntoResponse {
    let mut store = WidgetStore::new();
    store
        .fetch_from_store()
        .await
        .map_err(|err| {
            error!("Failed to fetch widget store: {:?}", err);
        })
        .ok();
    let json = serde_json::to_string(store.get_items()).unwrap_or_else(|_| "[]".into());
    info!("Serving store items: {}", json.as_str());
    JsonStringResponse(json)
}

async fn get_system_config() -> impl IntoResponse {
    let res = globals::with_storage(|storage| storage.get_widget_config()).await;
    match res {
        Ok(config) => Json(config),
        Err(err) => {
            error!("Failed to load system config: {:?}", err);
            Json(SystemConfiguration::default())
        }
    }
}

async fn post_install_widget(Json(action): Json<InstallAction>) -> impl IntoResponse {
    let (download_url, description) = match action {
        InstallAction::FromUrl(url) => (url, alloc::string::String::from("No description")),
        InstallAction::FromStoreItemName(name) => {
            let mut store = WidgetStore::new();
            if let Err(err) = store.fetch_from_store().await {
                error!("Failed to fetch widget store before install: {:?}", err);
                return Err(());
            }
            let item = match store
                .get_items()
                .iter()
                .find(|item: &&WidgetStoreItem| item.name == name)
            {
                Some(item) => item,
                None => {
                    error!("Widget '{}' not found in fetched widget store", name.as_str());
                    return Err(());
                }
            };
            (item.get_download_url(), item.description.clone())
        }
    };
    info!("Installing widget from URL {}", download_url.as_str());
    let result = WidgetManager::install_widget(download_url.as_str(), &description).await;
    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            error!("{}", err);
            // TODO: add proper errors
            Err(())
        }
    }
}

async fn post_system_config(Json(config): Json<SystemConfiguration>) -> impl IntoResponse {
    let result = globals::with_storage(|storage| storage.save_widget_config(&config)).await;
    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            error!("Failed to save system config: {:?}", err);
            Err(())
        }
    }
}

async fn deinstall_widget(widget_name: alloc::string::String) -> impl IntoResponse {
    let result = WidgetManager::deinstall_widget(widget_name.as_str()).await;
    match result {
        Ok(_) => Ok(()),
        Err(err) => {
            error!(
                "Failed to deinstall widget {}: {:?}",
                widget_name.as_str(),
                err
            );
            Err(())
        }
    }
}

async fn get_config_schema(widget_name: alloc::string::String) -> impl IntoResponse {
    let mut runtime = Runtime::new();
    let widget_binary = match globals::with_storage(|storage| storage.wasm_read(&widget_name)).await {
        Ok(binary) => binary,
        Err(err) => {
            error!(
                "Failed to read widget binary for '{}': {:?}",
                widget_name.as_str(),
                err
            );
            return Json(String::from("{}"));
        }
    };

    let config = unsafe {
        let component = match runtime.load_module(&widget_binary) {
            Ok(component) => component,
            Err(_) => {
                error!("Failed to load WASM module for '{}'", widget_name.as_str());
                return Json(String::from("{}"));
            }
        };

        let widget = match runtime.instantiate(&component) {
            Ok(widget) => widget,
            Err(_) => {
                error!("Failed to instantiate widget '{}'", widget_name.as_str());
                return Json(String::from("{}"));
            }
        };

        match runtime.get_config_schema(&widget) {
            Ok(config) => config,
            Err(_) => {
                error!("Failed to get config schema for '{}'", widget_name.as_str());
                return Json(String::from("{}"));
            }
        }
    };

    Json(config)
}

async fn post_widget_config(
    widget_name: alloc::string::String,
    Json(config): Json<alloc::string::String>,
) -> impl IntoResponse {
    let mut system_config = match globals::with_storage(|storage| storage.get_widget_config()).await
    {
        Ok(system_config) => system_config,
        Err(err) => {
            error!(
                "Could not load system config to save widget config for widget {}: {}",
                widget_name.as_str(),
                err
            );
            return Err(());
        }
    };

    system_config.widgets.iter_mut().for_each(|widget| {
        if widget.name == widget_name {
            widget.json_config = config.clone();
        }
    });

    match globals::with_storage(|storage| storage.save_widget_config(&system_config)).await {
        Ok(_) => Ok(()),
        Err(err) => {
            error!(
                "Could not save widget config for widget {}: {}",
                widget_name.as_str(),
                err
            );
            Err(())
        }
    }
}

async fn get_widget_config(widget_name: alloc::string::String) -> impl IntoResponse {
    info!("Serving widget configuration page for: {}", widget_name.as_str());
    
    // Replace {{WIDGET_NAME}} placeholder with actual widget name
    let html = frontend::WIDGET_CONFIG_HTML.replace("{{WIDGET_NAME}}", widget_name.as_str());
    
    // Return custom HTML response that owns the string (no leak needed)
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
    // Use vec![] to allocate directly on heap (PSRAM), avoiding stack temporaries
    let mut tcp_rx_buffer = alloc::vec![0u8; TCP_BUFFER_SIZE].into_boxed_slice();
    let mut tcp_tx_buffer = alloc::vec![0u8; TCP_BUFFER_SIZE].into_boxed_slice();
    let mut http_buffer = alloc::vec![0u8; HTTP_BUFFER_SIZE].into_boxed_slice();

    picoserve::Server::new(router, config, &mut http_buffer)
        .listen_and_serve(task_id, stack, port, &mut tcp_rx_buffer, &mut tcp_tx_buffer)
        .await
        .into_never()
}
