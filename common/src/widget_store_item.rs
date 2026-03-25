#[cfg(not(feature = "yew"))]
extern crate alloc;

#[cfg(not(feature = "yew"))]
use alloc::format;
#[cfg(not(feature = "yew"))]
use alloc::string::String;

use crate::models::WidgetStoreItem;

impl WidgetStoreItem {
    /// Creates a new [`WidgetStoreItem`].
    pub fn new(name: String, description: String, repository: String) -> Self {
        Self {
            name,
            description,
            repository,
        }
    }

    /// Returns the direct download URL for this widget's precompiled WASM binary.
    ///
    /// Appends `/releases/latest/download/widget.precompiled.wasm` to [`Self::repository`].
    /// Widgets are automatically build and released under this path when using the widget template: <https://github.com/Siryll/wg_display_embedded_widget_template>
    pub fn get_download_url(&self) -> String {
        const SUFFIX: &str = "releases/latest/download/widget.precompiled.wasm";
        if self.repository.ends_with('/') {
            return format!("{}{}", self.repository, SUFFIX);
        }
        format!("{}/{}", self.repository, SUFFIX)
    }
}
