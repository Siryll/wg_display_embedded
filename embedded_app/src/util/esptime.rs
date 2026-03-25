//! UTC wall-clock time via a HTTP sync from `timeapi.io`.
//!
//! Time is fetched once at startup. Subsequent calls to [`EspTime::now_parts`] compute
//! the current time by adding the elapsed duration since the fetch.
use crate::runtime::http_sync;
use crate::runtime::http_sync::BridgeMethod;
use esp_hal::time::Instant;
use serde::Deserialize;

#[derive(Clone, Debug)]
pub struct EspTime {
    fetch_time_offset: Option<Instant>,
    fetched_time_epoch: Option<u64>,
}

/// TimeAPI response format for easy parsing.
#[derive(Deserialize)]
struct TimeApiResponse {
    unix_timestamp: u64,
}

impl EspTime {
    /// Creates a new, **unsynced** [`EspTime`]. Call [`fetch_time`](Self::fetch_time) before use.
    pub fn new() -> Self {
        Self {
            fetch_time_offset: None,
            fetched_time_epoch: None,
        }
    }

    /// Fetches the current Unix timestamp from `timeapi.io` and stores the monotonic offset.
    /// 
    /// # Panics
    /// Panics if the HTTP request fails or the response cannot be parsed.
    pub async fn fetch_time(&mut self) {
        // https://timeapi.io/swagger/index.html
        // returns unix time as json: { "unix_timestamp": 1774290895 }
        let response = http_sync::http_request_async(
            BridgeMethod::Get,
            alloc::string::String::from("https://timeapi.io/api/v1/time/current/unix"),
            None,
        )
        .await
        .expect("Failed to fetch time API response");

        self.fetch_time_offset = Some(Instant::now());

        // Parse response
        let body = alloc::string::String::from_utf8(response.bytes)
            .expect("Failed to parse response as UTF-8");
        let parsed: TimeApiResponse =
            serde_json::from_str(body.trim()).expect("Failed to parse time API JSON");

        self.fetched_time_epoch = Some(parsed.unix_timestamp);
    }

    /// Returns the current UTC time as `(seconds, nanoseconds)` since the Unix epoch.
    ///
    /// Used by the host function `runtime::host_api::clock_get`.
    pub fn now_parts(&self) -> Option<(u64, u32)> {
        match (self.fetch_time_offset, self.fetched_time_epoch) {
            (Some(offset), Some(epoch)) => {
                let elapsed = Instant::now() - offset;
                let seconds = epoch.saturating_add(elapsed.as_secs());
                let nanoseconds = ((elapsed.as_micros() % 1_000_000) * 1_000) as u32;
                Some((seconds, nanoseconds))
            }
            _ => None,
        }
    }
}
