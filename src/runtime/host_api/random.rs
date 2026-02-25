use crate::runtime::widget::widget::random;
use crate::runtime::WidgetState;
use esp_hal::rng::Rng;

fn get_random() -> u64 {
    let rng = Rng::new();
    let low = rng.random() as u64;
    let high = rng.random() as u64;
    (high << 32) | low
}

impl random::Host for WidgetState {
    fn get_random(&mut self) -> u64 {
        get_random()
    }
}
