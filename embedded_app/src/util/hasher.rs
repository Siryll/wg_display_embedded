//! SHA-256 hashing with hardware SHA peripheral.
use esp_hal::{
    peripherals::SHA,
    sha::{Sha, Sha256},
};
use nb::block;

/// Wraps the ESP32 hardware SHA peripheral for SHA-256 computation.
pub struct Hasher<'d> {
    sha: Sha<'d>,
}

impl<'d> Hasher<'d> {
    /// Creates a new [`Hasher`] from the `SHA` peripheral.
    pub fn new(sha_peripherals: SHA<'d>) -> Self {
        Self {
            sha: Sha::new(sha_peripherals),
        }
    }

    /// Returns the first 14 bytes of the digest
    pub fn hash(&mut self, input: &str) -> [u8; 14] {
        let mut hasher = self.sha.start::<Sha256>();
        let mut remaining = input.as_bytes();
        let mut output = [0u8; 14];

        while !remaining.is_empty() {
            // update() returns the not-yet-consumed tail until the HW block is processed.
            remaining = block!(hasher.update(remaining)).unwrap();
        }

        // Provide a shorter output buffer to get a truncated digest.
        block!(hasher.finish(output.as_mut_slice())).unwrap();
        output
    }
}
