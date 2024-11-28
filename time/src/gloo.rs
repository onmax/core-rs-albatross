use std::{
    convert::TryInto,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use gloo_timers::future::{IntervalStream, TimeoutFuture};
use instant::Instant;
use pin_project_lite::pin_project;
use send_wrapper::SendWrapper;

pub type Interval = SendWrapper<IntervalStream>;

pub fn interval(period: Duration) -> Interval {
    SendWrapper::new(IntervalStream::new(millis(period)))
}

pub type Sleep = SendWrapper<TimeoutFuture>;

pub fn sleep(duration: Duration) -> Sleep {
    #[allow(clippy::disallowed_types)]
    SendWrapper::new(TimeoutFuture::new(millis(duration)))
}

pub fn sleep_until(deadline: Instant) -> Sleep {
    sleep(deadline.saturating_duration_since(Instant::now()))
}

pin_project! {
    pub struct Timeout<F: Future> {
        #[pin]
        future: F,
        #[pin]
        deadline: Sleep,
    }
}

pub fn timeout<F: Future>(duration: Duration, future: F) -> Timeout<F> {
    Timeout {
        future: future,
        deadline: sleep(duration),
    }
}

impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output, ()>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<F::Output, ()>> {
        let this = self.project();
        if let Poll::Ready(result) = this.future.poll(cx) {
            return Poll::Ready(Ok(result));
        }
        if let Poll::Ready(_) = this.deadline.poll(cx) {
            return Poll::Ready(Err(()));
        }
        Poll::Pending
    }
}

#[track_caller]
fn millis(duration: Duration) -> u32 {
    duration
        .as_millis()
        .try_into()
        .expect("Period in milliseconds must fit into a u32")
}
