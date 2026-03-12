use crate::util::globals;
use crate::runtime::widget::widget::http;
use alloc::string::String;
use alloc::vec::Vec;
use defmt::{error, info};
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::channel::Channel;
use embassy_time::{Duration, with_timeout};
use esp_hal::time::Duration as HalDuration;
use esp_rtos::CurrentThreadHandle;

// HTTP request/response bridge for sync-to-async communication
#[derive(Clone)]
pub struct HttpRequest {
    pub method: BridgeMethod,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

#[derive(Clone, Copy)]
pub enum BridgeMethod {
    Get,
    Post,
    Put,
    Delete,
    Head,
}

impl BridgeMethod {
    fn as_str(self) -> &'static str {
        match self {
            BridgeMethod::Get => "GET",
            BridgeMethod::Post => "POST",
            BridgeMethod::Put => "PUT",
            BridgeMethod::Delete => "DELETE",
            BridgeMethod::Head => "HEAD",
        }
    }

    fn to_reqwless(self) -> reqwless::request::Method {
        match self {
            BridgeMethod::Get => reqwless::request::Method::GET,
            BridgeMethod::Post => reqwless::request::Method::POST,
            BridgeMethod::Put => reqwless::request::Method::PUT,
            BridgeMethod::Delete => reqwless::request::Method::DELETE,
            BridgeMethod::Head => reqwless::request::Method::HEAD,
        }
    }
}

pub type HttpResponse = Result<http::Response, ()>;

// Channels for request/response communication
// Capacity of 1 ensures only one request can be in-flight at a time
static HTTP_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, HttpRequest, 1> = Channel::new();
static HTTP_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, HttpResponse, 1> = Channel::new();
const ASYNC_BRIDGE_TIMEOUT: Duration = Duration::from_secs(30);

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
///
/// This sends a request to the HTTP handler task and waits for the response.
/// While waiting, it yields the current RTOS thread so Embassy tasks (including
/// `embassy_net::Runner`) can keep running.
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

                        // Crucial: sleep/yield the RTOS thread so the thread-mode
                        // Embassy executor can run net_task + other async tasks.
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
///
/// This should be spawned as a background task on startup
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
            http_client
                .request(
                    request.method.to_reqwless(),
                    &request.url,
                    request.body.as_deref(),
                )
                .await
                .map_err(|e| {
                    defmt::error!("HTTP handler request failed: {:?}", defmt::Debug2Format(&e));
                })
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
