//! Remote widget store catalog fetched from GitHub.
use crate::runtime::http_sync::{self};
use alloc::string::FromUtf8Error;
use alloc::vec::Vec;
use common::models::WidgetStoreItem;
use defmt::info;

const WIDGET_LISTING_URL: &str = "https://siryll.github.io/wg_display_embedded/widget_store.json";

#[derive(Debug, defmt::Format)]
pub enum WidgetStoreError {
    Http(&'static str),
    Utf8,
    Json,
}

impl From<&'static str> for WidgetStoreError {
    fn from(e: &'static str) -> Self {
        WidgetStoreError::Http(e)
    }
}

impl From<FromUtf8Error> for WidgetStoreError {
    fn from(_: FromUtf8Error) -> Self {
        WidgetStoreError::Utf8
    }
}

impl From<serde_json::Error> for WidgetStoreError {
    fn from(_: serde_json::Error) -> Self {
        WidgetStoreError::Json
    }
}

impl From<reqwless::Error> for WidgetStoreError {
    fn from(_: reqwless::Error) -> Self {
        WidgetStoreError::Http("HTTP request failed")
    }
}

#[derive(Clone)]
pub struct WidgetStore {
    store_items: Vec<WidgetStoreItem>,
}

impl WidgetStore {
    pub fn new() -> Self {
        Self {
            store_items: Vec::new(),
        }
    }

    /// Get all items in the store
    /// Use `fetch_from_store` to fetch the store before
    /// # Returns
    /// A vector of all items in the store
    pub fn get_items(&self) -> &Vec<WidgetStoreItem> {
        &self.store_items
    }

    /// Fetch the store from the internet
    /// # Returns
    /// An error if the fetch failed
    pub async fn fetch_from_store(&mut self) -> Result<(), WidgetStoreError> {
        info!("Fetching widget store from {}", WIDGET_LISTING_URL);
        let response = http_sync::http_request_async(http_sync::HttpRequest {
            method: reqwless::request::Method::GET,
            url: alloc::string::String::from(WIDGET_LISTING_URL),
            body: None,
        })
        .await
        .map_err(|_| WidgetStoreError::Http("HTTP bridge request failed"))?;

        let body = alloc::string::String::from_utf8(response.bytes)?;
        self.store_items = serde_json::from_str(&body)?;
        Ok(())
    }
}
