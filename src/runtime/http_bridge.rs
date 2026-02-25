//! HTTP Bridge Module
//!
//! Bridges synchronous WASM host functions with async Embassy HTTP client.
//! 
//! Since WASM host functions must be synchronous but the HTTP client is async,
//! this module provides a request/response channel system with a background
//! worker task that handles the actual async HTTP operations.
//!
//! ## Architecture
//!
//! 1. Host function sends request to REQUEST_CHANNEL (non-blocking)
//! 2. Host function busy-waits polling RESPONSE_CHANNEL
//! 3. Worker task receives request, executes async HTTP call
//! 4. Worker task sends response back via RESPONSE_CHANNEL
//! 5. Host function receives response and returns to WASM
//!
//! ## Single Widget Execution
//!
//! This design assumes only one widget runs at a time, so:
//! - Channel capacity of 1 is sufficient
//! - No request queuing needed
//! - No timeout complexity
//! - Simple linear flow: request → wait → response

use crate::http_client::EspHttpClient;
use alloc::string::String;
use alloc::vec::Vec;
use embassy_sync::channel::Channel;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;

/// HTTP request sent from host function to worker task
#[derive(Debug)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub body: Option<Vec<u8>>,
}

/// HTTP method types matching the WIT interface
#[derive(Debug, Clone, Copy)]
pub enum HttpMethod {
    Get,
    Head,
    Post,
    Put,
    Delete,
}

/// HTTP response sent from worker task back to host function
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub content_length: Option<u64>,
    pub bytes: Vec<u8>,
}

/// Type alias for the channel mutex (critical section based for no_std)
type HttpChannel<T> = Channel<CriticalSectionRawMutex, T, 1>;

/// Global request channel: host function → worker task
/// Capacity of 1 ensures only one request in flight at a time
static HTTP_REQUEST_CHANNEL: HttpChannel<HttpRequest> = Channel::new();

/// Global response channel: worker task → host function
/// Capacity of 1 matches the single-request model
static HTTP_RESPONSE_CHANNEL: HttpChannel<Result<HttpResponse, ()>> = Channel::new();

/// HTTP worker task - runs in background processing requests
///
/// This task loops forever:
/// 1. Wait for request on REQUEST_CHANNEL
/// 2. Execute async HTTP call
/// 3. Send response to RESPONSE_CHANNEL
///
/// Should be spawned once at application startup.
///
/// # Arguments
///
/// * `http_client` - The Embassy HTTP client for making requests
#[embassy_executor::task]
pub async fn http_worker_task(http_client: EspHttpClient) {
    defmt::info!("HTTP worker task started");
    
    loop {
        // Wait for the next request (blocks until available)
        let request = HTTP_REQUEST_CHANNEL.receive().await;
        
        defmt::debug!("HTTP worker received request: {} {}", 
            match request.method {
                HttpMethod::Get => "GET",
                HttpMethod::Head => "HEAD",
                HttpMethod::Post => "POST",
                HttpMethod::Put => "PUT",
                HttpMethod::Delete => "DELETE",
            },
            request.url.as_str()
        );
        
        // Execute the async HTTP request
        let result = match request.method {
            HttpMethod::Get => http_client.get(&request.url).await,
            HttpMethod::Head => http_client.head(&request.url).await,
            HttpMethod::Post => http_client.post(&request.url, request.body.as_deref()).await,
            HttpMethod::Put => http_client.put(&request.url, request.body.as_deref()).await,
            HttpMethod::Delete => http_client.delete(&request.url).await,
        };
        
        // Convert result to HttpResponse or error
        let response = result.map(|bytes| {
            let status = 200; // reqwless returns bytes on success, assume 200
            let content_length = Some(bytes.len() as u64);
            HttpResponse {
                status,
                content_length,
                bytes,
            }
        }).map_err(|_| ());
        
        // Send response back to waiting host function
        HTTP_RESPONSE_CHANNEL.send(response).await;
        
        defmt::debug!("HTTP worker sent response");
    }
}

/// Execute HTTP request from synchronous context (blocking)
///
/// This function bridges the sync/async gap by:
/// 1. Sending request to worker task via channel
/// 2. Busy-waiting until response is available
/// 3. Returning the response
///
/// The busy-wait uses `core::hint::spin_loop()` to hint the CPU
/// can do other work, making it more efficient than a tight loop.
///
/// # Arguments
///
/// * `method` - HTTP method (GET, POST, etc.)
/// * `url` - Request URL
/// * `body` - Optional request body (for POST/PUT)
///
/// # Returns
///
/// Result containing the HTTP response or error
///
/// # Panics
///
/// Panics if the request channel is full (shouldn't happen with single widget)
pub fn execute_http_request_blocking(
    method: HttpMethod,
    url: String,
    body: Option<Vec<u8>>,
) -> Result<HttpResponse, ()> {
    defmt::debug!("Sending HTTP request to worker");
    
    // Create request
    let request = HttpRequest { method, url, body };
    
    // Send to worker task (should always succeed with capacity 1 and single widget)
    HTTP_REQUEST_CHANNEL.try_send(request)
        .map_err(|_| {
            defmt::error!("Failed to send HTTP request - channel full");
            ()
        })?;
    
    // Busy-wait for response
    // Since we're in a sync context with no async executor access,
    // we poll the channel repeatedly with CPU yield hints
    defmt::debug!("Waiting for HTTP response...");
    
    let response = loop {
        match HTTP_RESPONSE_CHANNEL.try_receive() {
            Ok(resp) => break resp,
            Err(_) => {
                // Yield CPU with spin loop hint
                // This tells the CPU we're waiting, allowing it to optimize
                for _ in 0..1000 {
                    core::hint::spin_loop();
                }
            }
        }
    };
    
    defmt::debug!("Received HTTP response");
    response
}
