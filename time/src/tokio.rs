use std::time::Duration;

use tokio::time as tokio;
pub use tokio_stream::wrappers::IntervalStream as Interval;

use self::tokio::interval_at;
pub use self::tokio::{sleep, timeout, Sleep, Timeout};
use crate::Instant;

pub fn interval(period: Duration) -> Interval {
    #[allow(clippy::disallowed_methods)]
    Interval::new(interval_at(tokio::Instant::now() + period, period))
}

pub fn sleep_until(deadline: Instant) -> Sleep {
    #[allow(clippy::disallowed_methods)]
    tokio::sleep_until(tokio::Instant::from_std(deadline))
}
