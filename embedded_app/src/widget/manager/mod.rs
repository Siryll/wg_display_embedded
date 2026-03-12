// use crate::widgets::running::runtime::{CompiledWidget, Runtime};
#![allow(dead_code)]
use alloc::vec::Vec;

use crate::storage::StorageError;
use crate::util::globals;
use crate::runtime::http_sync::{self, BridgeMethod};

#[derive(Debug, defmt::Format)]
pub enum WidgetManagerError {
    Storage(StorageError),
    HttpError(&'static str),
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
            BridgeMethod::Get,
            alloc::string::String::from(download_url),
            None,
        )
        .await
        .map_err(|_| WidgetManagerError::HttpError("HTTP bridge request failed"))?;

        // TODO: will be implemented when runtime is ready
        // let mut runtime = Runtime::new();
        // let compiled_widget = runtime.compile_widget(&bytes)?;
        // let widget = runtime.instantiate_widget(&compiled_widget)?;
        // let widget_name = runtime.get_widget_name(&widget)?;
        // let version = runtime.get_widget_version(&widget)?;
        // let json_config = runtime.get_config_schema(&widget)?;
        let widget_name = "example_widget";
        let version = "0.1.0";
        let json_config = "{}";

        // simplify storage by just having one call that handles everything
        globals::with_storage(|storage| {
            storage.save_compiled_widget(widget_name, description, version, json_config, &response.bytes)
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
        globals::with_storage(|storage| storage.wasm_delete(widget_name)).await?;
        Ok(())
    }

    /// Get a previously installed widget
    /// # Arguments
    /// * `widget_name` - The name of the widget to get
    /// # Returns
    /// The compiled widget
    /// TODO: return Vec for now, could use CompiledWidget when runtime is ready
    pub async fn get_widget(widget_name: &str) -> Result<Vec<u8>, WidgetManagerError> {
        let widget = globals::with_storage(|storage| storage.wasm_read(widget_name)).await?;
        Ok(widget)
    }

    // Get all installed and configured widgets
    // Returns: A vector of compiled widgets
    // TODO: implemented but might be too expensive to use since all the binaries together might be too large to fit in RAM
    // pub async fn get_widgets() -> Vec<Vec<u8>> {
    //     let mut widgets = Vec::new();
    //     globals::with_storage(|storage| {
    //         for widget in storage.get_widget_config().unwrap().widgets {
    //             if let Ok(bytes) = storage.wasm_read(&widget.name) {
    //                 widgets.push(bytes);
    //             }
    //         }
    //     }).await;

    //     widgets
    // }
}
