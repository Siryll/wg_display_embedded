#[cfg(not(feature = "yew"))]
extern crate alloc;

#[cfg(not(feature = "yew"))]
use alloc::string::String;
#[cfg(not(feature = "yew"))]
use alloc::vec::Vec;

#[cfg(feature = "yew")]
use std::rc::Rc;

use serde::{Deserialize, Serialize};

/// widget information, part of [`SystemConfiguration`].
#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct WidgetInstallationData {
    /// Widget name as returned by the widget's `get-name` WIT export.
    pub name: String,
    /// Widget description if installed from widget store, otherwise null when installed via URL.
    pub description: String,
    /// Version string returned by the widget's `get-version` WIT export.
    pub version: String,
    /// Widget config stored as JSON string since model is not known at compile time.
    /// Is set to "{}" on installation and updated once when the widget is configured via the UI.
    pub json_config: String,
    /// In what interval the widget should be re-run, only relevant for the embedded runtime since the [`SystemConfiguration`] is stored on the device
    /// Set by the widget's `get-run-update-cycle-seconds` WIT export.
    pub update_cycle_seconds: u32,
}
/// Top-level device configuration, persisted to NVS flash and served via `GET /system_config`.
#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct SystemConfiguration {
    /// Display background color as a CSS value (e.g. `"#000000"`).
    #[serde(default)]
    pub background_color: String,
    /// list of installed widgets.
    #[serde(default)]
    pub widgets: Vec<WidgetInstallationData>,
}

/// Action payload for `POST /install_widget`. Determines the source of the widget binary.
#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum InstallAction {
    /// Install from direct URL pointing to a precompiled wasm file.
    FromUrl(String),
    /// Install via store. The download URL is derived via
    /// [`WidgetStoreItem::get_download_url`](crate::widget_store_item).
    FromStoreItemName(String),
}

/// Widget store item entry.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetStoreItem {
    /// Display name
    pub name: String,
    /// Widget description
    pub description: String,
    /// GitHub repository URL. The precompiled WASM download URL is derived from this
    /// by [`get_download_url`](crate::widget_store_item).
    pub repository: String,
}

#[cfg(feature = "yew")]
mod yew_support {
    use super::*;
    use gloo_console::log;
    use gloo_net::http::Request;
    use yew::functional::Reducible;

    fn persist_system_config(config: SystemConfiguration) {
        log!("Persisting new system config..");
        wasm_bindgen_futures::spawn_local(async move {
            Request::post("/system_config")
                .json(&config)
                .expect("Could not serialize config")
                .send()
                .await
                .expect("Could not transmit config");
        });
    }

    #[derive(PartialEq)]
    pub enum SystemConfigurationAction {
        SetInitialConfig(SystemConfiguration),
        SetBackgroundColor(String),
        AddWidget(WidgetInstallationData),
    }

    impl Reducible for SystemConfiguration {
        type Action = SystemConfigurationAction;

        fn reduce(self: Rc<Self>, action: Self::Action) -> Rc<Self> {
            let config = match action {
                SystemConfigurationAction::SetInitialConfig(new_config) => new_config,
                SystemConfigurationAction::SetBackgroundColor(background_color) => Self {
                    background_color,
                    ..(*self).clone()
                },
                SystemConfigurationAction::AddWidget(new_config) => Self {
                    widgets: {
                        let mut widget_config = self.widgets.clone();
                        widget_config.push(new_config);
                        widget_config
                    },
                    ..(*self).clone()
                },
            };
            persist_system_config(config.clone());
            config.into()
        }
    }
}

#[cfg(feature = "yew")]
pub use yew_support::SystemConfigurationAction;
