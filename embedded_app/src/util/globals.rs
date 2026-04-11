//! Application-wide shared states for:
//! - [`Storage`]
//! - [`Display`]
//! - [`EspTime`]
//! - [`WidgetStore`]
//!
//! All `init_*` functions **must be called exactly once** during startup in `main()`, a seccond call to any init function will cause a panic.
use crate::display::Display;
use crate::runtime::widget::widget::clocks::Datetime;
use crate::storage::Storage;
use crate::util::esptime::EspTime;
use crate::widget::store::WidgetStore;
use alloc::vec::Vec;
use common::models::WidgetStoreItem;
use core::cell::RefCell;
use critical_section::Mutex as CsMutex;
use defmt::info;
use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;

type GlobalMutex<T> = Mutex<CriticalSectionRawMutex, Option<T>>;

static STORAGE: GlobalMutex<Storage<'static>> = Mutex::new(None);
static DISPLAY: GlobalMutex<Display> = Mutex::new(None);
static ESP_TIME: CsMutex<RefCell<Option<EspTime>>> = CsMutex::new(RefCell::new(None));
static WIDGET_STORE: GlobalMutex<WidgetStore> = Mutex::new(None);

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

pub async fn init_display(display: Display) {
    let mut guard = DISPLAY.lock().await;
    if guard.is_some() {
        panic!("Display already initialized!");
    }
    *guard = Some(display);
    info!("Global display initialized");
}

pub async fn with_display<F, R>(f: F) -> R
where
    F: FnOnce(&mut Display) -> R,
{
    let mut guard = DISPLAY.lock().await;
    let display = guard
        .as_mut()
        .expect("Display not initialized! Call init_display() first");
    f(display)
}

pub async fn console_println(text: &str) {
    with_display(|display| display.console_println(text)).await;
}

pub fn init_time(time: EspTime) {
    critical_section::with(|cs| {
        let mut guard = ESP_TIME.borrow_ref_mut(cs);
        if guard.is_some() {
            panic!("Time already initialized! Call init_time() only once");
        }
        *guard = Some(time);
    });
    info!("Global time initialized");
}

pub fn with_time<F, R>(f: F) -> R
where
    F: FnOnce(&EspTime) -> R,
{
    critical_section::with(|cs| {
        let guard = ESP_TIME.borrow_ref(cs);
        let time = guard
            .as_ref()
            .expect("Time not initialized! Call init_time() first");
        f(time)
    })
}

pub fn now() -> Option<Datetime> {
    with_time(EspTime::now)
}

pub async fn init_store(store: WidgetStore) {
    let mut guard = WIDGET_STORE.lock().await;
    if guard.is_some() {
        panic!("WidgetStore already initialized! Call init_store() only once");
    }
    *guard = Some(store);
    info!("Global widget store initialized");
}

pub async fn with_store<F, R>(f: F) -> R
where
    F: FnOnce(&WidgetStore) -> R,
{
    let guard = WIDGET_STORE.lock().await;
    let store = guard
        .as_ref()
        .expect("WidgetStore not initialized! Call init_store() first");
    f(store)
}

pub async fn get_store_items() -> Vec<WidgetStoreItem> {
    with_store(|store| store.get_items().clone()).await
}
