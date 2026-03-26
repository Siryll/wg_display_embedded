#[cfg(not(feature = "yew"))]
extern crate alloc;

#[cfg(not(feature = "yew"))]
use alloc::string::String;
#[cfg(not(feature = "yew"))]
use alloc::vec::Vec;

#[cfg(feature = "yew")]
use std::rc::Rc;

use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct WidgetInstallationData {
    pub name: String,
    pub description: String,
    pub version: String,
    pub json_config: String,
    pub update_cycle_seconds: u32,
}
#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct SystemConfiguration {
    #[serde(default)]
    pub background_color: String,
    #[serde(default)]
    pub widgets: Vec<WidgetInstallationData>,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct WifiCredentials {
    pub ssid: String,
    pub password: String,
}

#[derive(Deserialize, Serialize, Clone, PartialEq, Default, Debug)]
pub struct WifiModeResponse {
    pub is_ap_mode: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum InstallAction {
    FromUrl(String),
    FromStoreItemName(String),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct WidgetStoreItem {
    pub name: String,
    pub description: String,
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
