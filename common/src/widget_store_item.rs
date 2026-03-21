#[cfg(not(feature = "yew"))]
extern crate alloc;

#[cfg(not(feature = "yew"))]
use alloc::format;
#[cfg(not(feature = "yew"))]
use alloc::string::String;

use crate::models::WidgetStoreItem;

impl WidgetStoreItem {
    pub fn new(name: String, description: String, repository: String) -> Self {
        Self {
            name,
            description,
            repository,
        }
    }

    pub fn get_download_url(&self) -> String {
        const SUFFIX: &str = "releases/latest/download/widget.precompiled.wasm";
        if self.repository.ends_with('/') {
            return format!("{}{}", self.repository, SUFFIX);
        }
        format!("{}/{}", self.repository, SUFFIX)
    }
}
