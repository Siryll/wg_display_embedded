use crate::runtime::widget::widget::logging;
use crate::runtime::WidgetState;
use alloc::string::String;

use defmt::{debug, error, info, warn};

impl logging::Host for WidgetState {
    fn log(
        &mut self,
        level: logging::Level,
        context: String,
        message: String,
    )  {
        const PREFIX: &str = "WIDGET";
        match level {
            logging::Level::Debug => debug!("{}: {}: {}",PREFIX, context.as_str(), message.as_str()),
            logging::Level::Info => info!("{}: {}: {}", PREFIX, context.as_str(), message.as_str()),
            logging::Level::Warn => warn!("{}: {}: {}", PREFIX, context.as_str(), message.as_str()),
            logging::Level::Error => error!("{}: {}: {}", PREFIX, context.as_str(), message.as_str()),
        }
    }
}
