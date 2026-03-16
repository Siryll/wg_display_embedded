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
        let dns = DnsSocket::new(self.stack);
        let tcp_state = alloc::boxed::Box::new(TcpClientState::<1, 4096, 4096>::new());
        let tcp = TcpClient::new(self.stack, &*tcp_state);

        let mut rx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
        let mut tx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
        let mut response_buffer = alloc::vec![0u8; 16640].into_boxed_slice();

        info!("Creating TLS config for HTTP client...");
        let tls = TlsConfig::new(
            self.tls_seed,
            &mut rx_buffer,
            &mut tx_buffer,
            reqwless::client::TlsVerify::None,
        );

        info!("Creating HTTP client...");
        let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
        info!("HTTP client created, building request...");
        let mut request = client.request(method, url).await?.body(body);

        info!("Sending HTTP request to {}", url);
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

    /// Download a file from `url`, following up to 5 redirects.
    /// Uses a 512KB response buffer suitable for WASM widget binaries stored in PSRAM.
    /// This is also the max size data can have when using NVS storage.
    pub async fn download(&self, url: &str) -> Result<Vec<u8>, Error> {
        const MAX_REDIRECTS: u8 = 5;
        let mut redirect_count: u8 = 0;
        let mut current_url = alloc::string::String::from(url);

        loop {
            let url_this_iter = current_url.clone();

            let dns = DnsSocket::new(self.stack);
            let tcp_state = alloc::boxed::Box::new(TcpClientState::<1, 4096, 4096>::new());
            let tcp = TcpClient::new(self.stack, &*tcp_state);

            let mut rx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
            let mut tx_buffer = alloc::vec![0u8; 16640].into_boxed_slice();
            let mut response_buffer = alloc::vec![0u8; 524288].into_boxed_slice();

            let tls = TlsConfig::new(
                self.tls_seed,
                &mut rx_buffer,
                &mut tx_buffer,
                reqwless::client::TlsVerify::None,
            );

            let mut client = HttpClient::new_with_tls(&tcp, &dns, tls);
            info!("Downloading {}", url_this_iter.as_str());
            let mut request = client
                .request(reqwless::request::Method::GET, url_this_iter.as_str())
                .await?
                .body(None::<&[u8]>);

            let response = request.send(&mut response_buffer).await?;
            let status = response.status.0;
            info!("Response status: {}", status);

            if (300u16..400).contains(&status) {
                if redirect_count >= MAX_REDIRECTS {
                    warn!("Max redirects ({}) reached", MAX_REDIRECTS);
                    return Err(Error::Codec);
                }
                let mut location: Option<alloc::string::String> = None;
                for (name, value) in response.headers() {
                    if name.eq_ignore_ascii_case("location") {
                        if let Ok(s) = core::str::from_utf8(value) {
                            location = Some(alloc::string::String::from(s));
                        }
                        break;
                    }
                }
                match location {
                    Some(new_url) => {
                        info!("Following redirect {} -> {}", status, new_url.as_str());
                        current_url = new_url;
                        redirect_count += 1;
                        continue;
                    }
                    None => {
                        warn!("Redirect response missing Location header");
                        return Err(Error::Codec);
                    }
                }
            }

            if !(200u16..300).contains(&status) {
                warn!("Download failed with status {}", status);
            }

            let body_bytes = response.body().read_to_end().await?;
            info!("Download complete: {} bytes", body_bytes.len());
            return Ok(body_bytes.to_vec());
        }
    }
}
