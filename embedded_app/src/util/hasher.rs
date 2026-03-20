use esp_hal::{
    peripherals::SHA,
    sha::{Sha, Sha256},
};
use nb::block;

pub struct Hasher<'d> {
    sha: Sha<'d>,
}

impl<'d> Hasher<'d> {
    pub fn new(sha_peripherals: SHA<'d>) -> Self {
        Self {
            sha: Sha::new(sha_peripherals),
        }
    }

    // create a 12 bit hash for the wasm binary nvs storage
    // based on the example from https://docs.rs/esp32-hal/latest/esp32_hal/sha/index.html
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
