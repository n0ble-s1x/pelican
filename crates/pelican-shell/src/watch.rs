//! Noticing that a watch was plugged in, without ever opening it.
//!
//! `garmin::list_devices` reads USB descriptors: it enumerates, it does not
//! claim an interface, and it therefore cannot contend with a session the
//! device thread holds. That is the whole reason this can be a poll loop at
//! all — anything that opened the device would have to be serialised behind
//! the device thread and would defeat the point.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use pelican_core::garmin;

use crate::dto::{EventSink, UiEvent};

/// How often USB is actually enumerated.
const POLL: Duration = Duration::from_millis(1500);
/// How often the loop wakes. Short so a requested re-announce is answered
/// promptly rather than up to a full poll interval later.
const TICK: Duration = Duration::from_millis(250);

pub struct WatchHandle {
    announce: Arc<AtomicBool>,
}

impl WatchHandle {
    /// Ask for the current device state to be re-emitted on the next tick.
    ///
    /// `subscribe` needs it: it installs the event channel *after* this
    /// thread has started, so the one `Attached` that mattered may already
    /// have been emitted into nothing.
    ///
    /// `disconnect` deliberately does *not* use it. This thread reports what
    /// is on the USB bus; the device thread reports whether a session is
    /// open. A re-announce after an explicit disconnect would immediately
    /// contradict the `Detached` the device thread just sent.
    pub fn request_announce(&self) {
        self.announce.store(true, Ordering::SeqCst);
    }
}

pub fn spawn(sink: Arc<EventSink>) -> WatchHandle {
    let announce = Arc::new(AtomicBool::new(false));
    let flag = announce.clone();

    std::thread::Builder::new()
        .name("pelican-usb-watch".into())
        .spawn(move || {
            let mut last: Option<String> = None;
            let mut next_poll = Instant::now();
            loop {
                std::thread::sleep(TICK);
                let forced = flag.swap(false, Ordering::SeqCst);
                if !forced && Instant::now() < next_poll {
                    continue;
                }
                next_poll = Instant::now() + POLL;

                // An enumeration failure is not "no watch" — it is "we could
                // not look". Treating them the same would flap the UI between
                // states on a transient USB hiccup, so hold the last known
                // answer and try again next tick.
                let Ok(devices) = garmin::list_devices() else {
                    continue;
                };

                let key = devices
                    .iter()
                    .map(|d| d.label())
                    .collect::<Vec<_>>()
                    .join("|");
                if !forced && last.as_deref() == Some(key.as_str()) {
                    continue;
                }

                let event = match devices.first() {
                    Some(d) => UiEvent::Attached {
                        label: d.label(),
                        serial: d.serial.clone(),
                        product: d.product.clone(),
                    },
                    None => UiEvent::Detached,
                };
                if sink.send(event) {
                    last = Some(key);
                } else {
                    // Nobody is listening yet. Forget what we "sent" so the
                    // next tick says it again.
                    last = None;
                }
            }
        })
        .expect("spawning the USB watch thread");

    WatchHandle { announce }
}
