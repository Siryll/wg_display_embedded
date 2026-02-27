use crate::runtime::widget::widget::http;
use crate::runtime::WidgetState;
use alloc::string::String;
use alloc::vec::Vec;
use crate::globals;
use reqwless::request::Method;
use embassy_time::Duration;
use embassy_futures::block_on;

impl http::Host for WidgetState {
    fn request(
        &mut self,
        method: http::Method,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<http::Response, ()> {
        let http_client = globals::http_client();

        // Create the async future for the HTTP request
        let future = async {
            let response_future = match method {
                http::Method::Get => http_client.request(Method::GET, &url, None),
                http::Method::Head => http_client.request(Method::HEAD, &url, None),
                http::Method::Post => http_client.request(Method::POST, &url, body.as_deref()),
                http::Method::Put => http_client.request(Method::PUT, &url, body.as_deref()),
                http::Method::Delete => http_client.request(Method::DELETE, &url, None),
            };
            
            // Add timeout and await the response
            match embassy_time::with_timeout(Duration::from_secs(10), response_future).await {
                Ok(result) => result.map_err(|_| ()), // Convert &str error to ()
                Err(_) => Err(()), // Timeout error
            }
        };
        
        // Block on the future using embassy_futures::block_on
        // Note: This uses a busy loop - acceptable for infrequent widget HTTP calls
        let response_bytes: Vec<u8> = block_on(future)?;

        // Convert to WIT response type
        // TODO: handle errors correctly and return appropriate HTTP status codes
        Ok(http::Response {
            status: 200,
            content_length: Some(u64::try_from(response_bytes.len()).unwrap_or(0)),
            bytes: response_bytes,
        })
    }
}
