use alloc::vec::Vec;
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

#[allow(dead_code)]
impl EspHttpClient {
    pub fn new(stack: Stack<'static>, tls_seed: u64) -> Self {
        Self { stack, tls_seed }
    }

    pub async fn get(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.request(reqwless::request::Method::GET, url, None)
            .await
    }

    pub async fn post(&self, url: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        self.request(reqwless::request::Method::POST, url, body)
            .await
    }

    pub async fn put(&self, url: &str, body: Option<&[u8]>) -> Result<Vec<u8>, Error> {
        self.request(reqwless::request::Method::PUT, url, body)
            .await
    }

    pub async fn delete(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.request(reqwless::request::Method::DELETE, url, None)
            .await
    }

    pub async fn head(&self, url: &str) -> Result<Vec<u8>, Error> {
        self.request(reqwless::request::Method::HEAD, url, None)
            .await
    }

    pub async fn request(
        &self,
        method: reqwless::request::Method,
        url: &str,
        body: Option<&[u8]>,
    ) -> Result<Vec<u8>, Error> {
        // create dns and tcp clients
        let dns = DnsSocket::new(self.stack);
        let tcp_state = alloc::boxed::Box::new(TcpClientState::<1, 4096, 4096>::new());
        let tcp = TcpClient::new(self.stack, &*tcp_state);

        let mut rx_buffer = alloc::vec![0u8; 4096].into_boxed_slice();
        let mut tx_buffer = alloc::vec![0u8; 4096].into_boxed_slice();
        let mut response_buffer = alloc::vec![0u8; 4096].into_boxed_slice();

        let tls = TlsConfig::new(
            self.tls_seed,
            &mut rx_buffer,
            &mut tx_buffer,
            reqwless::client::TlsVerify::None,
        );

        let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
        let mut request = client.request(method, url).await?.body(body);

        let response = request.send(&mut response_buffer).await?;

        let status = response.status.0;
        info!("Response status: {}", status);

        if !(200..300).contains(&status) {
            warn!("HTTP request failed with status {}", status);
        }

        let body_bytes = response.body().read_to_end().await?;

        info!("HTTP body read succeeded: {} bytes", body_bytes.len());
        Ok(body_bytes.to_vec())
    }
}
