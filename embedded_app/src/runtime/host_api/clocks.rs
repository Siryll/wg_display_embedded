use crate::runtime::WidgetState;
use crate::runtime::widget::widget::clocks;
use crate::util::globals;

impl clocks::Host for WidgetState {
    fn now(&mut self) -> clocks::Datetime {
        let (seconds, nanoseconds) = globals::now_parts().unwrap_or((0, 0));

        clocks::Datetime {
            seconds,
            nanoseconds,
        }
    }
}
