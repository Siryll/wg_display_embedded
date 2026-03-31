#![doc = "Implementation of the `http` WIT interface.\n\n```wit"]
#![doc = include_str!("wit/http.wit")]
#![doc = "```"]

use crate::runtime::WidgetState;
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
        let response = http_request_sync(method, url, body);

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
