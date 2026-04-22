//! Channel-based bridge allowing synchronous HTTP calls from Wasmtime host function [runtime::host_api::http](crate::runtime::host_api::http).
//!
//! Due to potenial memory corruption issues with multiple [EspHttpClient](crate::http_client::EspHttpClient) instances, all http request are processed by [http_handler_task] even for async functions.
//!
//!
//! ```text
//! Core 1 (widget)          Core 0 (embassy)
//! ───────────────          ────────────────
//! http_request_sync()  ──► HTTP_REQUEST_CHANNEL
//!   polls with 10ms         http_handler_task() dequeues
//!   RTOS yields       ◄──  HTTP_RESPONSE_CHANNEL
//! ```
//!
//! Timeout on both sides: **30 seconds**.
use crate::{http_client::EspHttpClient, runtime::widget::widget::http};
use alloc::string::String;
use alloc::vec::Vec;
use defmt::{error, info};
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, with_timeout};
use esp_hal::time::Duration as HalDuration;
use esp_rtos::CurrentThreadHandle;

// HTTP request/response bridge for sync-to-async communication
/// An HTTP request queued from the synchronous widget side to the async handler.
#[derive(Clone)]
pub struct HttpRequest {
    pub method: reqwless::request::Method,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

pub type HttpResponse = Result<http::Response, ()>;

static HTTP_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, HttpRequest, 1> = Channel::new();
static HTTP_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, HttpResponse, 1> = Channel::new();
const ASYNC_BRIDGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Asynchronous HTTP request function callable from async context.
pub async fn http_request_async(request: HttpRequest) -> Result<http::Response, ()> {
    match with_timeout(ASYNC_BRIDGE_TIMEOUT, HTTP_REQUEST_CHANNEL.send(request)).await {
        Ok(()) => {}
        Err(_) => {
            error!("HTTP async bridge timed out while enqueueing request");
            return Err(());
        }
    }

    match with_timeout(ASYNC_BRIDGE_TIMEOUT, HTTP_RESPONSE_CHANNEL.receive()).await {
        Ok(response) => response,
        Err(_) => {
            error!("HTTP async bridge timed out while waiting for response");
            Err(())
        }
    }
}

/// Synchronous HTTP request function called from WIT host functions
/// Widget execution is halted until this function returns or times out.
pub fn http_request_sync(request: HttpRequest) -> Result<http::Response, ()> {
    info!(
        "http_request_sync: sending request to {}",
        request.url.as_str()
    );

    // Send request to async handler task.
    match HTTP_REQUEST_CHANNEL.try_send(request) {
        Ok(_) => {
            info!("HTTP request sent to channel, waiting for response...");

            let mut iterations = 0u32;
            let current_thread = CurrentThreadHandle::get();
            const MAX_WAIT_ITERATIONS: u32 = 3000; // ~30s @ 10ms delay

            loop {
                match HTTP_RESPONSE_CHANNEL.try_receive() {
                    Ok(response) => {
                        info!("HTTP response received after {} iterations", iterations);
                        return response;
                    }
                    Err(_) => {
                        iterations += 1;
                        if iterations >= MAX_WAIT_ITERATIONS {
                            error!("HTTP request timed out while waiting for handler response");
                            return Err(());
                        }
                        if iterations.is_multiple_of(20) {
                            info!("Still waiting... iteration {}", iterations);
                        }
                        current_thread.delay(HalDuration::from_millis(10));
                    }
                }
            }
        }
        Err(_) => {
            error!("HTTP request channel full");
            Err(())
        }
    }
}

/// Async task that handles HTTP requests from the channel
/// Run on Core 0.
#[embassy_executor::task]
pub async fn http_handler_task(stack: Stack<'static>, _tls_seed: u64) {
    defmt::info!("HTTP handler task started");
    let http_client = EspHttpClient::new(stack, _tls_seed);

    loop {
        defmt::debug!("HTTP handler: waiting for request...");

        // Wait for incoming request
        let request = HTTP_REQUEST_CHANNEL.receive().await;

        defmt::info!("Processing HTTP request to: {=str}", request.url.as_str());

        // execute request
        defmt::info!("HTTP handler: executing request");
        let response_result = http_client
            .request(request.method, &request.url, request.body.as_deref())
            .await
            .map_err(|e| {
                defmt::error!("HTTP handler request failed: {:?}", defmt::Debug2Format(&e));
            });

        // Send response back
        HTTP_RESPONSE_CHANNEL.send(response_result).await;
        defmt::info!("HTTP response sent back to caller");
    }
}
