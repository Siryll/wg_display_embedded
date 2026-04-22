#![doc = "Implementation of the `http` WIT interface.\n\n```wit"]
#![doc = include_str!("wit/http.wit")]
#![doc = "```"]

use crate::runtime::WidgetState;
use crate::runtime::http_sync::HttpRequest;
use crate::runtime::http_sync::http_request_sync;
use crate::runtime::widget::widget::http;
use alloc::string::String;
use alloc::vec::Vec;
use defmt::info;

impl http::Host for WidgetState {
    /// Handles HTTP request from widgets, request is sent to the async handler task [`runtime::http_sync::http_handler_task`](crate::runtime::http_sync::http_handler_task) via `http_sync::http_request_sync` and the response is returned to the widget.
    fn request(
        &mut self,
        method: http::Method,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<http::Response, ()> {
        info!("HTTP host function called");

        let request_method = match method {
            http::Method::Get => reqwless::request::Method::GET,
            http::Method::Post => reqwless::request::Method::POST,
            http::Method::Put => reqwless::request::Method::PUT,
            http::Method::Delete => reqwless::request::Method::DELETE,
            http::Method::Head => reqwless::request::Method::HEAD,
        };

        let response = http_request_sync(HttpRequest {
            method: request_method,
            url,
            body,
        });

        match response {
            Ok(resp) => {
                info!(
                    "HTTP request successful: status={}, bytes={}",
                    resp.status,
                    resp.bytes.len()
                );
                Ok(resp)
            }
            Err(_) => {
                info!("HTTP request failed");
                Err(())
            }
        }
    }
}
