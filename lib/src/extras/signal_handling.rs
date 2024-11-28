use std::time::Duration;

use nimiq_time::sleep;
use nimiq_utils::spawn;
use signal_hook::{consts::SIGINT, iterator::Signals};

pub fn initialize_signal_handler() {
    let signals = Signals::new([SIGINT]);

    if let Ok(mut signals) = signals {
        spawn(async move {
            if signals.forever().next().is_some() {
                log::warn!("Received Ctrl+C. Closing client");
                // Add some delay for the log message to propagate into loki
                sleep(Duration::from_millis(200)).await;
                std::process::exit(0);
            }
        });
    } else {
        log::error!("Could not obtain SIGINT signal");
    }
}
