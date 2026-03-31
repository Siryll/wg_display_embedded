#![doc = "Implementation of the `types` WIT interface.\n\n```wit"]
#![doc = include_str!("wit/types.wit")]
#![doc = "```"]

use crate::runtime::WidgetState;
use crate::runtime::widget::widget::types::Host;

impl Host for WidgetState {}
