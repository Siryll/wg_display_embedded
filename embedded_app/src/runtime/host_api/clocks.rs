#![doc = "Implementation of the `clocks` WIT interface.\n\n```wit"]
#![doc = include_str!("wit/clocks.wit")]
#![doc = "```"]
use crate::runtime::WidgetState;
use crate::runtime::widget::widget::clocks::{self, Datetime};
use crate::util::globals;

impl clocks::Host for WidgetState {
    /// returns the current UTC time as [`Datetime`] since the Unix epoch.
    fn now(&mut self) -> clocks::Datetime {
        globals::now().unwrap_or(Datetime {
            seconds: 0,
            nanoseconds: 0,
        })
    }
}
