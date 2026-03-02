use crate::runtime::widget::widget::http;
use crate::runtime::WidgetState;
use alloc::string::String;
use alloc::vec::Vec;
use crate::globals;
use defmt::info;

impl http::Host for WidgetState {
    fn request(
        &mut self,
        method: http::Method,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<http::Response, ()> {
        info!("HTTP host function called");
        let response = globals::http_request_sync(method, url, body);

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
