use crate::runtime::WidgetState;
use crate::runtime::widget::widget::clocks::{self, Datetime};
use crate::util::globals;

impl clocks::Host for WidgetState {
    fn now(&mut self) -> clocks::Datetime {
        globals::now().unwrap_or(Datetime {
            seconds: 0,
            nanoseconds: 0,
        })
    }
}
