use alloc::vec::Vec;
use defmt::{info, warn};
use embassy_net::{
    Stack,
    dns::DnsSocket,
    tcp::client::{TcpClient, TcpClientState},
};
use reqwless::{
    client::{HttpClient, TlsConfig},
    request::RequestBuilder,
};

#[derive(Clone, Copy)]
pub struct EspHttpClient {
    stack: Stack<'static>,
    tls_seed: u64,
}

#[allow(dead_code)]
impl EspHttpClient {
    pub fn new(stack: Stack<'static>, tls_seed: u64) -> Self {
        Self { stack, tls_seed }
    }

    pub async fn get(&self, url: &str) -> Result<Vec<u8>, &'static str> {
        self.request(reqwless::request::Method::GET, url, None)
            .await
    }

    pub async fn post(&self, url: &str, body: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
        self.request(reqwless::request::Method::POST, url, body)
            .await
    }

    pub async fn put(&self, url: &str, body: Option<&[u8]>) -> Result<Vec<u8>, &'static str> {
        self.request(reqwless::request::Method::PUT, url, body)
            .await
    }

    pub async fn delete(&self, url: &str) -> Result<Vec<u8>, &'static str> {
        self.request(reqwless::request::Method::DELETE, url, None)
            .await
    }

    pub async fn head(&self, url: &str) -> Result<Vec<u8>, &'static str> {
        self.request(reqwless::request::Method::HEAD, url, None)
            .await
    }

    async fn request(
        &self,
        method: reqwless::request::Method,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<Vec<u8>, &'static str> {
        // buffers
        let mut rx_buffer = [0u8; 4096];
        let mut tx_buffer = [0u8; 4096];
        let mut response_buffer = [0u8; 4096];

        // create dns and tcp clients
        let dns = DnsSocket::new(self.stack);
        let tcp_state = TcpClientState::<1, 4096, 4096>::new();
        let tcp = TcpClient::new(self.stack, &tcp_state);

        // setup TLS for https
        let tls = TlsConfig::new(
            self.tls_seed,
            &mut rx_buffer,
            &mut tx_buffer,
            reqwless::client::TlsVerify::None,
        );

        let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);

        // Create and send request
        let mut request = client
            .request(method, url)
            .await
            .map_err(|_| "Failed to create request")?
            .body(body);

        let response = request
            .send(&mut response_buffer)
            .await
            .map_err(|_| "Failed to send request")?;

        let status = response.status.0;
        info!("Response status: {}", status);

        if !(200..300).contains(&status) {
            warn!("HTTP request failed with status {}", status);
            // return Err("HTTP error status");
        }

        let body_bytes = response
            .body()
            .read_to_end()
            .await
            .map_err(|_| "Failed to read response body")?;

        // Convert to Vec (uses heap allocation)
        let result = body_bytes.to_vec();

        Ok(result)
    }
}
