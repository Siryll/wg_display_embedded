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

// TcpClientState lives in .bss — never constructed on the CPU stack.
// Mutex serializes access since only N=1 connection slot is available.
// static TCP_STATE: Mutex<CriticalSectionRawMutex, TcpClientState<1, 4096, 4096>> =
//     Mutex::new(TcpClientState::new());

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
        // Acquire the static TcpClientState via the async mutex.
        //
        // Box::new(TcpClientState::<1,4096,4096>::new()) would construct the ~8 KB struct
        // on the CPU stack before boxing it. On bare-metal (none-elf) the effective stack
        // is the DRAM remaining after BSS + heap; every additional Embassy task shrinks
        // that space by the size of its TaskStorage. Once http_handler_task is present
        // the margin is gone and the 8 KB construction overflows the stack, corrupting
        // the return address and producing an InstrProhibited exception.
        //
        // Using the static mutex avoids any stack construction: TcpClientState already
        // lives in BSS, and lock().await is safe to hold across .await points with
        // embassy_sync::Mutex. The lock also enforces the N=1 socket-slot contract.
        let dns = DnsSocket::new(self.stack);
        // let guard = TCP_STATE.lock().await;
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

        // Send the request and read the response
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
}
