#![allow(dead_code)]
use crate::common::models::WidgetStoreItem;
use crate::util::globals;
use alloc::string::FromUtf8Error;
use alloc::vec::Vec;

const WIDGET_LISTING_URL: &str =
    "https://raw.githubusercontent.com/eliabieri/wg_display/main/widget_store.json";

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
        let http_client = globals::http_client();
        let response_bytes = http_client.get(WIDGET_LISTING_URL).await?;
        let body = alloc::string::String::from_utf8(response_bytes)?;
        self.store_items = serde_json::from_str(&body)?;
        Ok(())
    }
}
