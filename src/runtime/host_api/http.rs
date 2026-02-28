use crate::runtime::widget::widget::http;
use crate::runtime::WidgetState;
use alloc::string::String;
use alloc::vec::Vec;
use crate::globals;

impl http::Host for WidgetState {
    fn request(
        &mut self,
        method: http::Method,
        url: String,
        body: Option<Vec<u8>>,
    ) -> Result<http::Response, ()> {
        // Use the global HTTP request bridge
        globals::http_request_sync(method, url, body)
    }
}
