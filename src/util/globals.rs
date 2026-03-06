use crate::http_client::EspHttpClient;
use crate::storage::Storage;
use core::sync::atomic::{AtomicBool, Ordering};
use defmt::info;
use embassy_net::Stack;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

type GlobalMutex<T> = Mutex<CriticalSectionRawMutex, Option<T>>;

static STORAGE: GlobalMutex<Storage<'static>> = Mutex::new(None);

static mut NETWORK_STACK: Option<Stack<'static>> = None;

static mut TLS_SEED: Option<u64> = None;

static NETWORK_READY: AtomicBool = AtomicBool::new(false);

pub async fn init_storage(storage: Storage<'static>) {
    let mut guard = STORAGE.lock().await;
    if guard.is_some() {
        panic!("Storage already initialized!");
    }
    *guard = Some(storage);
    info!("Global storage initialized");
}

pub async fn with_storage<F, R>(f: F) -> R
where
    F: FnOnce(&mut Storage<'static>) -> R,
{
    let mut guard = STORAGE.lock().await;
    let storage = guard
        .as_mut()
        .expect("Storage not initialized! Call init_storage() first");
    f(storage)
}

pub fn init_network(stack: Stack<'static>, tls_seed: u64) {
    unsafe {
        NETWORK_STACK = Some(stack);
        TLS_SEED = Some(tls_seed);
    }
    NETWORK_READY.store(true, Ordering::Release);
    info!("Global network stack initialized");
}

pub fn network_stack() -> Stack<'static> {
    unsafe { NETWORK_STACK.expect("Network not initialized! Call init_network() first") }
}

pub fn tls_seed() -> u64 {
    unsafe { TLS_SEED.expect("Network not initialized! Call init_network() first") }
}

pub fn http_client() -> EspHttpClient {
    EspHttpClient::new(network_stack(), tls_seed())
}
