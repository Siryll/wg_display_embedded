use crate::runtime::widget::widget::clocks;
use crate::runtime::WidgetState;
use esp_hal::time::Instant;

impl clocks::Host for WidgetState {
    fn now(&mut self) -> clocks::Datetime {
        let now = Instant::now();
        let duration_since_boot = now.duration_since_epoch();
        let total_millis = duration_since_boot.as_millis();
        let seconds = total_millis / 1000;
        
        let millis_within_second = total_millis % 1000;
        let nanoseconds = (millis_within_second * 1_000_000) as u32;
        
        clocks::Datetime {
            seconds,
            nanoseconds,
        }
    }
}
