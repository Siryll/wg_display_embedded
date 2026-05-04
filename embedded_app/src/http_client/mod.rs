//! Async HTTPS client using `reqwless` and `embassy-net`.
//!
//! ## Buffer sizes
//! - TLS RX/TX buffers: 16,640 bytes each (allocated in PSRAM)
//! - Response buffer: 524,288 bytes ≈512 KB (PSRAM) — determines the maximum
//!   downloadable file size, including widget WASM binaries. This also is the max size of a storeable widget with [`Storage`](crate::storage::Storage).
//!
//! ## Security
//! TLS certificate verification is **disabled** (`TlsVerify::None`).
//!
//! ## Usage
//!
//! Call via
//! [`http_sync::http_request_async`](crate::runtime::http_sync::http_request_async)
//! from async contexts, or
//! [`http_sync::http_request_sync`](crate::runtime::http_sync::http_request_sync)
//! from sync contexts (e.g. widget WASM host functions). Do not construct
//! [`EspHttpClient`] directly.
use crate::runtime::widget::widget::http;
use defmt::{info, warn};
use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    Error,
    client::{HttpClient, TlsConfig},
    request::RequestBuilder,
};

pub struct EspHttpClient {
    stack: Stack<'static>,
    tls_seed: u64,
}

const MAX_RESPONSE_SIZE: usize = 1024 * 1024; // 1 MiB
const MAX_REDIRECTS: u8 = 5;

impl EspHttpClient {
    /// Creates a new client from a network stack and TLS seed.
    pub fn new(stack: Stack<'static>, tls_seed: u64) -> Self {
        Self { stack, tls_seed }
    }

    /// Sends an HTTP request to `url` with provided [`reqwless::request::Method`], following up to 5 redirects automatically (defined by [`MAX_REDIRECTS`]).
    /// Max response body is defined by [`MAX_RESPONSE_SIZE`] (currently 1 MiB).
    ///
    /// # Errors
    /// Returns [`Error::Codec`] if the redirect limit is exceeded or a redirect
    /// response is missing the `Location` header.
    pub async fn request(
        &self,
        method: reqwless::request::Method,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<http::Response, Error> {
        let mut current_url = alloc::string::String::from(url);

        for _ in 0..=MAX_REDIRECTS {
            let url_this_iter = current_url.clone();

            let dns = DnsSocket::new(self.stack);
            let tcp_state = alloc::boxed::Box::new(TcpClientState::<1, 4096, 4096>::new());
            let tcp = TcpClient::new(self.stack, &*tcp_state);

            let mut rx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
            let mut tx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
            let mut response_buffer = alloc::vec![0u8; MAX_RESPONSE_SIZE].into_boxed_slice();

            let tls = TlsConfig::new(
                self.tls_seed,
                &mut rx_buffer,
                &mut tx_buffer,
                reqwless::client::TlsVerify::None,
            );

            let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
            info!("HTTP {} {}", method as u8, url_this_iter.as_str());
            let mut request = client
                .request(method, url_this_iter.as_str())
                .await?
                .body(body);

            let response = request.send(&mut response_buffer).await?;
            let status = response.status.0;
            info!("Response status: {}", status);

            // check code and return, otherwise follow redirect
            if !(300u16..400).contains(&status) {
                if !(200u16..300).contains(&status) {
                    warn!("Request failed with status {}", status);
                }
                let body_bytes = response.body().read_to_end().await?;
                info!("Response complete: {} bytes", body_bytes.len());
                return Ok(http::Response {
                    status,
                    bytes: body_bytes.to_vec(),
                    content_length: Some(u64::try_from(body_bytes.len()).unwrap_or(0)),
                });
            }

            // Follow redirect
            let location = response
                .headers()
                .find(|(name, _)| name.eq_ignore_ascii_case("location"))
                .and_then(|(_, value)| core::str::from_utf8(value).ok())
                .map(alloc::string::String::from);

            match location {
                Some(new_url) => {
                    info!("Following redirect {} -> {}", status, new_url.as_str());
                    current_url = new_url;
                }
                None => {
                    warn!("Redirect response missing Location header");
                    return Err(Error::Codec);
                }
            }
        }

        warn!("Max redirects ({}) reached", MAX_REDIRECTS);
        Err(Error::Codec)
    }
}
