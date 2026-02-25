use crate::runtime::widget::widget::http;
use crate::runtime::http_bridge::{self, HttpMethod};
use crate::runtime::WidgetState;
use alloc::string::String;
use alloc::vec::Vec;

impl http::Host for WidgetState {
    fn request(
        &mut self,
        method: http::Method,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<http::Response, ()> {
        // Convert WIT method to bridge method
        let bridge_method = match method {
            http::Method::Get => HttpMethod::Get,
            http::Method::Head => HttpMethod::Head,
            http::Method::Post => HttpMethod::Post,
            http::Method::Put => HttpMethod::Put,
            http::Method::Delete => HttpMethod::Delete,
        };
        
        // Execute HTTP request via bridge (this blocks until response arrives)
        let response = http_bridge::execute_http_request_blocking(
            bridge_method,
            url,
            body,
        )?;
        
        // Convert bridge response to WIT response type
        Ok(http::Response {
            status: response.status,
            content_length: response.content_length,
            bytes: response.bytes,
        })
    }
}
