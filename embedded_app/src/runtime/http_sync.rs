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
use crate::runtime::widget::widget::http;
use crate::util::globals;
use alloc::string::String;
use alloc::vec::Vec;
use defmt::{error, info};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, with_timeout};
use esp_hal::time::Duration as HalDuration;
use esp_rtos::CurrentThreadHandle;

// HTTP request/response bridge for sync-to-async communication
/// An HTTP request queued from the synchronous widget side to the async handler.
#[derive(Clone)]
pub struct HttpRequest {
    pub method: BridgeMethod,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

/// HTTP method variants used by the bridge
#[derive(Clone, Copy, PartialEq)]
pub enum BridgeMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
    /// uses [`EspHttpClient::download`](crate::http_client::EspHttpClient::download) for large file downloads with automatic redirects.
    Download, // special method to allow for large file download and automatic re-direction
}

impl BridgeMethod {
    fn as_str(self) -> &'static str {
        match self {
            BridgeMethod::Get => "GET",
            BridgeMethod::Post => "POST",
            BridgeMethod::Put => "PUT",
            BridgeMethod::Delete => "DELETE",
            BridgeMethod::Head => "HEAD",
            BridgeMethod::Download => "DOWNLOAD",
        }
    }

    fn to_reqwless(self) -> reqwless::request::Method {
        match self {
            BridgeMethod::Get | BridgeMethod::Download => reqwless::request::Method::GET,
            BridgeMethod::Post => reqwless::request::Method::POST,
            BridgeMethod::Put => reqwless::request::Method::PUT,
            BridgeMethod::Delete => reqwless::request::Method::DELETE,
            BridgeMethod::Head => reqwless::request::Method::HEAD,
        }
    }
}

pub type HttpResponse = Result<http::Response, ()>;

static HTTP_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, HttpRequest, 1> = Channel::new();
static HTTP_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, HttpResponse, 1> = Channel::new();
const ASYNC_BRIDGE_TIMEOUT: Duration = Duration::from_secs(30);

/// Asynchronous HTTP request function callable from async context.
pub async fn http_request_async(
    method: BridgeMethod,
    url: String,
    body: Option<Vec<u8>>,
) -> Result<http::Response, ()> {
    let request = HttpRequest { method, url, body };

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
pub fn http_request_sync(
    method: http::Method,
    url: String,
    body: Option<Vec<u8>>,
) -> Result<http::Response, ()> {
    let method = match method {
        http::Method::Get => BridgeMethod::Get,
        http::Method::Post => BridgeMethod::Post,
        http::Method::Put => BridgeMethod::Put,
        http::Method::Delete => BridgeMethod::Delete,
        http::Method::Head => BridgeMethod::Head,
    };

    info!(
        "http_request_sync: sending {} request to {}",
        method.as_str(),
        url.as_str()
    );

    let request = HttpRequest { method, url, body };

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
pub async fn http_handler_task() {
    defmt::info!("HTTP handler task started");

    loop {
        defmt::debug!("HTTP handler: waiting for request...");

        // Wait for incoming request
        let request = HTTP_REQUEST_CHANNEL.receive().await;

        defmt::info!(
            "Processing HTTP {} request to: {=str}",
            request.method.as_str(),
            request.url.as_str()
        );

        // Get HTTP client and execute request
        defmt::info!("Created HTTP request object, executing...");
        let http_client = globals::http_client();

        defmt::info!("HTTP handler: executing request");
        let response_result = async {
            match request.method {
                BridgeMethod::Download => http_client.download(&request.url).await.map_err(|e| {
                    defmt::error!(
                        "HTTP handler download failed: {:?}",
                        defmt::Debug2Format(&e)
                    );
                }),
                _ => http_client
                    .request(
                        request.method.to_reqwless(),
                        &request.url,
                        request.body.as_deref(),
                    )
                    .await
                    .map_err(|e| {
                        defmt::error!("HTTP handler request failed: {:?}", defmt::Debug2Format(&e));
                    }),
            }
        }
        .await;

        // Convert to WIT response type
        let response = response_result.map(|response_bytes| {
            defmt::info!(
                "HTTP request succeeded, {} bytes received",
                response_bytes.len()
            );
            http::Response {
                status: 200, // TODO: get actual status from reqwless
                content_length: Some(u64::try_from(response_bytes.len()).unwrap_or(0)),
                bytes: response_bytes,
            }
        });

        // Send response back
        HTTP_RESPONSE_CHANNEL.send(response).await;
        defmt::info!("HTTP response sent back to caller");
    }
}
