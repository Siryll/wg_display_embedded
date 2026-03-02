use crate::http_client::EspHttpClient;
use crate::storage::Storage;
use crate::runtime::widget::widget::http;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use esp_rtos::CurrentThreadHandle;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use embassy_sync::channel::Channel;
use esp_hal::time::Duration as HalDuration;
use defmt::{info, error};

type GlobalMutex<T> = Mutex<CriticalSectionRawMutex, Option<T>>;

static STORAGE: GlobalMutex<Storage<'static>> = Mutex::new(None);

static mut NETWORK_STACK: Option<Stack<'static>> = None;

static mut TLS_SEED: Option<u64> = None;

static NETWORK_READY: AtomicBool = AtomicBool::new(false);

// HTTP request/response bridge for sync-to-async communication
#[derive(Clone)]
pub struct HttpRequest {
    pub method: http::Method,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

pub type HttpResponse = Result<http::Response, ()>;

// Channels for request/response communication
// Capacity of 1 ensures only one request can be in-flight at a time
static HTTP_REQUEST_CHANNEL: Channel<CriticalSectionRawMutex, HttpRequest, 1> = Channel::new();
static HTTP_RESPONSE_CHANNEL: Channel<CriticalSectionRawMutex, HttpResponse, 1> = Channel::new();

pub async fn init_storage(storage: Storage<'static>) {
    let mut guard = STORAGE.lock().await;
    if guard.is_some() {
        panic!("Storage already initialized!");
    }
    *guard = Some(storage);
    defmt::info!("Global storage initialized");
}

pub async fn with_storage<F, R>(f: F) -> R
where
    F: FnOnce(&mut Storage<'static>) -> R,
{
    let mut guard = STORAGE.lock().await;
    let storage = guard
        .as_mut()
        .expect("Storage not initialized! Call init_storage() first");
    f(storage)
}

pub fn init_network(stack: Stack<'static>, tls_seed: u64) {
    unsafe {
        NETWORK_STACK = Some(stack);
        TLS_SEED = Some(tls_seed);
    }
    NETWORK_READY.store(true, Ordering::Release);
    defmt::info!("Global network stack initialized");
}

pub fn network_stack() -> Stack<'static> {
    unsafe { NETWORK_STACK.expect("Network not initialized! Call init_network() first") }
}

pub fn tls_seed() -> u64 {
    unsafe { TLS_SEED.expect("Network not initialized! Call init_network() first") }
}

pub fn http_client() -> EspHttpClient {
    EspHttpClient::new(network_stack(), tls_seed())
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
    info!("http_request_sync: sending {} request to {}",
        match method {
            http::Method::Get => "GET",
            http::Method::Post => "POST",
            http::Method::Put => "PUT",
            http::Method::Delete => "DELETE",
            http::Method::Head => "HEAD",
        },
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
                        if iterations % 20 == 0 {
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
pub async fn http_handler_task() {
    defmt::info!("HTTP handler task started");

    loop {
        defmt::debug!("HTTP handler: waiting for request...");
        
        // Wait for incoming request
        let request = HTTP_REQUEST_CHANNEL.receive().await;
        
        defmt::info!("Processing HTTP {} request to: {=str}", 
            match request.method {
                http::Method::Get => "GET",
                http::Method::Post => "POST",
                http::Method::Put => "PUT",
                http::Method::Delete => "DELETE",
                http::Method::Head => "HEAD",
            },
            request.url.as_str()
        );
        
        // Get HTTP client and execute request
        defmt::info!("Created HTTP request object, executing...");
        let http_client = http_client();
        
        defmt::info!("HTTP handler: executing request");
        let response_result = async {
            let method = match request.method {
                http::Method::Get => reqwless::request::Method::GET,
                http::Method::Post => reqwless::request::Method::POST,
                http::Method::Put => reqwless::request::Method::PUT,
                http::Method::Delete => reqwless::request::Method::DELETE,
                http::Method::Head => reqwless::request::Method::HEAD,
            };

            http_client
                .request(method, &request.url, request.body.as_deref())
                .await
                .map_err(|e| {
                    defmt::error!(
                        "HTTP handler request failed: {:?}",
                        defmt::Debug2Format(&e)
                    );
                    ()
                })
        }.await;
        
        // Convert to WIT response type
        let response = response_result.map(|response_bytes| {
            defmt::info!("HTTP request succeeded, {} bytes received", response_bytes.len());
            http::Response {
                status: 200,  // TODO: get actual status from reqwless
                content_length: Some(u64::try_from(response_bytes.len()).unwrap_or(0)),
                bytes: response_bytes,
            }
        });
        
        // Send response back
        HTTP_RESPONSE_CHANNEL.send(response).await;
        defmt::info!("HTTP response sent back to caller");
    }
}
