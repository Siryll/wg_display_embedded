use common::models::{WifiCredentials, WifiModeResponse};
use gloo_net::http::Request;
use web_sys::{HtmlInputElement, SubmitEvent};
use yew::prelude::*;

use crate::components::config_card::ConfigCardComponent;

#[derive(Properties, PartialEq)]
pub struct WifiCredentialsConfigProps {
    pub error: UseStateHandle<Option<String>>,
}

async fn save_wifi_credentials_and_restart(payload: WifiCredentials) -> Result<(), String> {
    let response = Request::post("/wifi_credentials")
        .json(&payload)
        .expect("Failed to serialize WiFi credentials")
        .send()
        .await
        .map_err(|err| format!("Failed to save WiFi credentials: {err}"))?;

    if response.status() == 200 {
        return Ok(());
    }

    let status = response.status();
    let body = response
        .text()
        .await
        .unwrap_or_else(|_| "Failed to save WiFi credentials".to_string());

    if body.trim().is_empty() {
        Err(format!("Failed to save WiFi credentials (HTTP {status})"))
    } else {
        Err(body)
    }
}

async fn is_ap_mode() -> Result<bool, String> {
    let response = Request::get("/wifi_mode")
        .send()
        .await
        .map_err(|err| format!("Failed to fetch WiFi mode: {err}"))?;

    if response.status() != 200 {
        return Err(format!(
            "Failed to fetch WiFi mode (HTTP {})",
            response.status()
        ));
    }

    let mode = response
        .json::<WifiModeResponse>()
        .await
        .map_err(|_| "Failed to parse WiFi mode response".to_string())?;

    Ok(mode.is_ap_mode)
}

#[function_component(WifiCredentialsConfigComponent)]
pub fn wifi_credentials_config_component(props: &WifiCredentialsConfigProps) -> Html {
    let ssid_ref = use_node_ref();
    let password_ref = use_node_ref();
    let show_form = use_state(|| false);

    {
        let show_form = show_form.clone();
        let error = props.error.clone();
        use_effect_with_deps(
            move |_| {
                wasm_bindgen_futures::spawn_local(async move {
                    match is_ap_mode().await {
                        Ok(is_ap) => show_form.set(is_ap),
                        Err(message) => {
                            error.set(Some(message));
                            show_form.set(false);
                        }
                    }
                });
                || {}
            },
            (),
        );
    }

    if !*show_form {
        return html! {};
    }

    let on_save_and_restart = {
        let ssid_ref = ssid_ref.clone();
        let password_ref = password_ref.clone();
        let error = props.error.clone();
        Callback::from(move |event: SubmitEvent| {
            event.prevent_default();

            let Some(ssid_input) = ssid_ref.cast::<HtmlInputElement>() else {
                error.set(Some("SSID input is not available".to_string()));
                return;
            };
            let Some(password_input) = password_ref.cast::<HtmlInputElement>() else {
                error.set(Some("Password input is not available".to_string()));
                return;
            };

            let ssid_value = ssid_input.value();
            let password_value = password_input.value();

            if ssid_value.trim().is_empty() {
                error.set(Some("SSID must not be empty".to_string()));
                return;
            }

            error.set(None);

            let error = error.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let payload = WifiCredentials {
                    ssid: ssid_value,
                    password: password_value,
                };

                if let Err(message) = save_wifi_credentials_and_restart(payload).await {
                    error.set(Some(message));
                }
            });
        })
    };

    html! {
        <ConfigCardComponent>
            <form class="flex flex-col gap-3" onsubmit={on_save_and_restart}>
                <p class="text-slate-300 text-sm">
                    {"Provision WiFi for station mode. Saving will reboot the device."}
                </p>
                <input
                    ref={ssid_ref}
                    type="text"
                    class="rounded-sm p-2 bg-zinc-800 text-white"
                    placeholder="SSID"
                    autocomplete="off"
                    autocapitalize="none"
                />
                <input
                    ref={password_ref}
                    type="password"
                    class="rounded-sm p-2 bg-zinc-800 text-white"
                    placeholder="Password"
                    autocomplete="new-password"
                />
                <button
                    class="text-gray-300 text-sm font-semibold border border-zinc-500 rounded px-3 py-2 disabled:opacity-50"
                    type="submit"
                >
                    {"Save and Restart"}
                </button>
            </form>
        </ConfigCardComponent>
    }
}
