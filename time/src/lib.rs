use std::{
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{Stream, StreamExt as _};
pub use instant::Instant;
use pin_project_lite::pin_project;

#[cfg(target_family = "wasm")]
mod gloo;
#[cfg(not(target_family = "wasm"))]
mod tokio;

#[cfg(target_family = "wasm")]
use gloo as sys;
#[cfg(not(target_family = "wasm"))]
use tokio as sys;

pub struct Interval {
    sys: sys::Interval,
}

// TODO: decide on first tick. right now or after one period?
pub fn interval(period: Duration) -> Interval {
    limit_duration(period);
    Interval {
        sys: sys::interval(period),
    }
}

impl Stream for Interval {
    type Item = ();
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<()>> {
        self.sys
            .poll_next_unpin(cx)
            .map(|option| option.map(|_| ()))
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (usize::MAX, None)
    }
}

pin_project! {
    pub struct Sleep {
        #[pin]
        sys: sys::Sleep,
    }
}

pub fn sleep(duration: Duration) -> Sleep {
    limit_duration(duration);
    Sleep {
        sys: sys::sleep(duration),
    }
}

pub fn sleep_until(deadline: Instant) -> Sleep {
    if let Some(duration) = deadline.checked_duration_since(Instant::now()) {
        limit_duration(duration);
    }
    Sleep {
        sys: sys::sleep_until(deadline),
    }
}

impl Future for Sleep {
    type Output = ();
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        self.project().sys.poll(cx)
    }
}

#[derive(Debug)]
pub struct Elapsed(());

impl fmt::Display for Elapsed {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        "timeout has elapsed".fmt(f)
    }
}

impl Error for Elapsed {}

pin_project! {
    pub struct Timeout<F: Future> {
        #[pin]
        sys: sys::Timeout<F>,
    }
}

pub fn timeout<F: Future>(timeout: Duration, future: F) -> Timeout<F> {
    limit_duration(timeout);
    Timeout {
        sys: sys::timeout(timeout, future),
    }
}

impl<F: Future> Future for Timeout<F> {
    type Output = Result<F::Output, Elapsed>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<F::Output, Elapsed>> {
        self.project().sys.poll(cx).map_err(|_| Elapsed(()))
    }
}

#[track_caller]
fn limit_duration(duration: Duration) {
    // Limit the period to the maximum allowed by gloo-timers to get consistent
    // behaviour across both implementations.
    assert!(
        duration.as_millis() <= u32::MAX as u128,
        "Period in milliseconds must fit into a u32",
    );
}
