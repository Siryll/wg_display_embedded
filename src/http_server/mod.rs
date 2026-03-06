use defmt::{error, info, warn};
use embassy_executor::Spawner;
use embassy_net::Stack;
use embassy_time::Duration;
use esp_alloc as _;
use picoserve::{
    AppBuilder, AppRouter, Router,
    response::{File, IntoResponse, Json},
    routing::{self, parse_path_segment},
};

use crate::common::models::WidgetStoreItem;
use crate::widget::manager::WidgetManager;
use crate::{
    common::models::{InstallAction, SystemConfiguration},
    util::globals,
    widget::store::WidgetStore,
};

pub const WEB_TASK_POOL_SIZE: usize = 1;

pub struct Application;

impl AppBuilder for Application {
    type PathRouter = impl routing::PathRouter;

    fn build_app(self) -> picoserve::Router<Self::PathRouter> {
        picoserve::Router::new()
            .route(
                "/",
                routing::get_service(File::html(include_str!("test.html"))),
            )
            .route("/get_store_items", routing::get(get_store_items))
            .route("/install_widget", routing::post(post_install_widget))
            .route("/system_config", routing::post(post_system_config))
            .route("/system_config", routing::get(get_system_config))
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
    }
}

// endpoint implementations (stubs until common is implemented)

async fn get_store_items() -> impl IntoResponse {
    let mut store = WidgetStore::new();
    store
        .fetch_from_store()
        .await
        .map_err(|err| {
            error!("Failed to fetch widget store: {:?}", err);
        })
        .ok();
    Json(store.get_items().to_vec())
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
            let item: &WidgetStoreItem = store
                .get_items()
                .iter()
                .find(|item: &&WidgetStoreItem| item.name == name)
                .unwrap();
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
    // TODO: requires runtime implementation
    warn!(
        "get_config_schema endpoint called for widget {}, not yet implemented",
        widget_name.as_str()
    );
    "Not implemented yet"
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
    // TODO: requires frontend implementation
    warn!(
        "get_widget_config endpoint called for widget {}, not yet implemented",
        widget_name.as_str()
    );
    "Not implemented yet"
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
                read_request: Duration::from_secs(1),
                write: Duration::from_secs(1),
                persistent_start_read_request: Duration::from_secs(1),
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
    let mut tcp_rx_buffer = alloc::vec![0u8; 1024].into_boxed_slice();
    let mut tcp_tx_buffer = alloc::vec![0u8; 1024].into_boxed_slice();
    let mut http_buffer = alloc::vec![0u8; 2048].into_boxed_slice();

    picoserve::Server::new(router, config, &mut *http_buffer)
        .listen_and_serve(
            task_id,
            stack,
            port,
            &mut *tcp_rx_buffer,
            &mut *tcp_tx_buffer,
        )
        .await
        .into_never()
}
