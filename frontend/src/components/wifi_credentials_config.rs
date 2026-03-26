use common::models::{WifiCredentials, WifiModeResponse};
use gloo_net::http::Request;
use wasm_bindgen::JsCast;
use web_sys::{HtmlInputElement, InputEvent};
use yew::prelude::*;

use crate::components::config_card::ConfigCardComponent;

#[derive(Properties, PartialEq)]
pub struct WifiCredentialsConfigProps {
    pub error: UseStateHandle<Option<String>>,
}

async fn save_wifi_credentials_and_restart(payload: WifiCredentials) -> Result<(), String> {
    let response = Request::post("/wifi_credentials/restart")
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
        return Err(format!("Failed to fetch WiFi mode (HTTP {})", response.status()));
    }

    let mode = response
        .json::<WifiModeResponse>()
        .await
        .map_err(|_| "Failed to parse WiFi mode response".to_string())?;

    Ok(mode.is_ap_mode)
}

#[function_component(WifiCredentialsConfigComponent)]
pub fn wifi_credentials_config_component(props: &WifiCredentialsConfigProps) -> Html {
    let ssid = use_state(String::new);
    let password = use_state(String::new);
    let is_saving = use_state(|| false);
    let show_form = use_state(|| true);

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
                            show_form.set(true);
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

    let on_ssid_input = {
        let ssid = ssid.clone();
        Callback::from(move |event: InputEvent| {
            if let Some(input) = event
                .target()
                .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
            {
                ssid.set(input.value());
            }
        })
    };

    let on_password_input = {
        let password = password.clone();
        Callback::from(move |event: InputEvent| {
            if let Some(input) = event
                .target()
                .and_then(|t| t.dyn_into::<HtmlInputElement>().ok())
            {
                password.set(input.value());
            }
        })
    };

    let on_save_and_restart = {
        let ssid = ssid.clone();
        let password = password.clone();
        let is_saving = is_saving.clone();
        let error = props.error.clone();
        Callback::from(move |_| {
            let ssid_value = (*ssid).clone();
            let password_value = (*password).clone();

            if ssid_value.trim().is_empty() {
                error.set(Some("SSID must not be empty".to_string()));
                return;
            }

            is_saving.set(true);
            error.set(None);

            let is_saving = is_saving.clone();
            let error = error.clone();
            let password = password.clone();

            wasm_bindgen_futures::spawn_local(async move {
                let payload = WifiCredentials {
                    ssid: ssid_value,
                    password: password_value,
                };

                let result = save_wifi_credentials_and_restart(payload).await;
                is_saving.set(false);

                match result {
                    Ok(()) => {
                        password.set(String::new());
                    }
                    Err(message) => {
                        error.set(Some(message));
                    }
                }
            });
        })
    };

    html! {
        <ConfigCardComponent>
            <div class="flex flex-col gap-3">
                <p class="text-slate-300 text-sm">
                    {"Provision WiFi for station mode. Saving will reboot the device."}
                </p>
                <input
                    type="text"
                    class="rounded-sm p-2 bg-zinc-800 text-white"
                    placeholder="SSID"
                    value={(*ssid).clone()}
                    oninput={on_ssid_input}
                    autocomplete="off"
                    autocapitalize="none"
                    disabled={*is_saving}
                />
                <input
                    type="password"
                    class="rounded-sm p-2 bg-zinc-800 text-white"
                    placeholder="Password"
                    value={(*password).clone()}
                    oninput={on_password_input}
                    autocomplete="new-password"
                    disabled={*is_saving}
                />
                <button
                    class="text-gray-300 text-sm font-semibold border border-zinc-500 rounded px-3 py-2 disabled:opacity-50"
                    onclick={on_save_and_restart}
                    disabled={*is_saving}
                >
                    { if *is_saving { "Saving..." } else { "Save and Restart" } }
                </button>
            </div>
        </ConfigCardComponent>
    }
}
