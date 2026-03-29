use alloc::string::String;
use alloc::vec::Vec;
use common::models::SystemConfiguration;

use crate::runtime::Runtime;
use crate::runtime::http_sync::{self, BridgeMethod};
use crate::storage::StorageError;
use crate::util::globals;
use defmt::error;

#[derive(Debug, defmt::Format)]
pub enum WidgetManagerError {
    Storage(StorageError),
    HttpError(&'static str),
    WasmError(&'static str),
    AlreadyInstalled(&'static str),
}

impl From<StorageError> for WidgetManagerError {
    fn from(e: StorageError) -> Self {
        WidgetManagerError::Storage(e)
    }
}

impl From<&'static str> for WidgetManagerError {
    fn from(e: &'static str) -> Self {
        WidgetManagerError::HttpError(e)
    }
}

impl From<reqwless::Error> for WidgetManagerError {
    fn from(_: reqwless::Error) -> Self {
        WidgetManagerError::HttpError("HTTP request failed")
    }
}

pub struct WidgetManager;

impl WidgetManager {
    /// Download and persist a widget
    /// Also adds a default configuration for the widget
    /// # Arguments
    /// * `download_url` - The URL to download the widget from
    /// * `description` - A description of the widget
    /// # Returns
    /// An error if the download or persisting failed
    pub async fn install_widget(
        download_url: &str,
        description: &str,
    ) -> Result<(), WidgetManagerError> {
        // let http_client = globals::http_client();
        let response = http_sync::http_request_async(
            BridgeMethod::Download,
            alloc::string::String::from(download_url),
            None,
        )
        .await
        .map_err(|_| WidgetManagerError::HttpError("HTTP bridge request failed"))?;

        let mut runtime = Runtime::new();

        let widget_metadata_result = unsafe { runtime.get_widget_metadata(&response.bytes).await };

        let mut widget_metadata = match widget_metadata_result {
            Ok(config) => config,
            Err(_) => {
                error!("Failed to get config schema for '{}'", download_url);
                return Err(WidgetManagerError::WasmError(
                    "Failed to get widget config schema",
                ));
            }
        };

        // check if widget has already been installed
        let system_config: SystemConfiguration = globals::with_storage(|storage| storage.get_system_config())
            .await
            .unwrap_or_default();

        if system_config.widgets.iter().any(|w| w.name == widget_metadata.name) {
            return Err(WidgetManagerError::AlreadyInstalled(
                "Widget with same name has already been installed",
            ));
        }

        widget_metadata.description = String::from(description);

        // simplify storage by just having one call that handles everything
        globals::with_storage(|storage| {
            storage.save_compiled_widget(widget_metadata, &response.bytes)
        })
        .await?;
        Ok(())
    }

    /// Deinstall a widget
    /// # Arguments
    /// * `widget_name` - The name of the widget to deinstall
    /// # Returns
    /// An error if the deinstallation failed
    pub async fn deinstall_widget(widget_name: &str) -> Result<(), WidgetManagerError> {
        globals::with_storage(|storage| storage.deinstall_widget(widget_name)).await?;
        Ok(())
    }

    /// Get a previously installed widget
    /// # Arguments
    /// * `widget_name` - The name of the widget to get
    /// # Returns
    /// The compiled widget
    /// TODO: return Vec for now, could use CompiledWidget when runtime is ready
    #[allow(dead_code)]
    pub async fn get_widget(widget_name: &str) -> Result<Vec<u8>, WidgetManagerError> {
        let widget = globals::with_storage(|storage| storage.wasm_read(widget_name)).await?;
        Ok(widget)
    }

    // Get all names of installed widgets
    // Returns: A vector of widget names
    // Will be used by the rendere for gettin all widgets to display them in the UI
    // TODO: maybe use to check if system config is allinged with actual stored binaries
    #[allow(dead_code)]
    pub async fn get_widgets() -> Result<Vec<String>, WidgetManagerError> {
        let widget_names = globals::with_storage(|storage| {
            storage
                .get_system_config()
                .map(|config| config.widgets.into_iter().map(|w| w.name).collect())
        })
        .await?;
        Ok(widget_names)
    }
}
