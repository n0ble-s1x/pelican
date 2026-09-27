//! One MTP session at a time.
//!
//! The watch allows a single session, and a second open fails with an
//! exclusive-access error that reads like someone else's fault. So every
//! command that opens one — `status`, `watch_list`, `push`, `backup_watch`,
//! `reset_check`, `reset_ledger` — takes this lock
//! first, and a command that cannot take it is refused as busy rather than
//! queued: a click that silently waits minutes behind a send looks broken.
//!
//! The guard is an owned value, not a `MutexGuard`, because a push holds it
//! on its own worker thread for the whole run.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use pelican_core::preview::Room;
use pelican_core::transfer::Stop;

/// What a busy command answers.
pub const BUSY: &str = "busy: the watch is in use by another Pelican operation (it allows \
                        one session at a time). Try again when it finishes.";

#[derive(Debug, Clone, Default)]
pub struct DeviceLock(Arc<AtomicBool>);

impl DeviceLock {
    /// Take the lock, or say who has it.
    pub fn try_acquire(&self) -> Result<DeviceGuard, String> {
        self.0
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map(|_| DeviceGuard(self.0.clone()))
            .map_err(|_| BUSY.to_string())
    }

    pub fn is_held(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

/// Releases the lock however its holder ends, panics included.
#[derive(Debug)]
pub struct DeviceGuard(Arc<AtomicBool>);

impl Drop for DeviceGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// What the last `status` learned, for a command that must not open a
/// session of its own: `preview` marks skips from `serial`'s ledger and
/// sizes the plan against `room`.
#[derive(Debug, Clone, Default)]
pub struct Last {
    pub serial: Option<String>,
    pub room: Option<Room>,
    /// The model it reported, for naming a backup folder.
    pub model: Option<String>,
}

#[derive(Debug, Default)]
pub struct Watch {
    pub lock: DeviceLock,
    /// Serial and room from the last successful `status`. Cleared when a
    /// read fails and after every push, since the room is then stale.
    pub last: Mutex<Option<Last>>,
    /// The running push's or backup's stop switch; `None` between runs.
    pub stop: Mutex<Option<Stop>>,
}

impl Watch {
    pub fn remember(&self, serial: Option<String>, room: Option<Room>, model: Option<String>) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = Some(Last {
            serial,
            room,
            model,
        });
    }

    pub fn model(&self) -> Option<String> {
        self.last
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|l| l.model.clone())
    }

    pub fn forget(&self) {
        *self.last.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }

    pub fn serial(&self) -> Option<String> {
        self.last
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|l| l.serial.clone())
    }

    pub fn room(&self) -> Option<Room> {
        self.last
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .as_ref()
            .and_then(|l| l.room)
    }

    /// Ask the running push or backup to stop between files. A no-op
    /// between runs.
    pub fn request_stop(&self) {
        if let Some(stop) = self.stop.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            stop.request();
        }
    }

    pub fn set_stop(&self, stop: Option<Stop>) {
        *self.stop.lock().unwrap_or_else(|e| e.into_inner()) = stop;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_operation_is_refused_as_busy_until_the_first_ends() {
        let lock = DeviceLock::default();
        let g = lock.try_acquire().expect("first");
        let err = lock.try_acquire().expect_err("second while held");
        assert!(err.starts_with("busy:"), "{err}");
        assert!(lock.is_held());
        drop(g);
        assert!(!lock.is_held());
        let _again = lock.try_acquire().expect("free again");
    }

    /// A push holds the guard on its own thread; a panic there must not
    /// leave every later command refused as busy for the life of the app.
    #[test]
    fn the_lock_is_released_when_its_thread_panics() {
        let lock = DeviceLock::default();
        let g = lock.try_acquire().unwrap();
        let r = std::thread::spawn(move || {
            let _g = g;
            panic!("a run that dies");
        })
        .join();
        assert!(r.is_err());
        assert!(!lock.is_held());
    }

    #[test]
    fn stop_reaches_the_running_push_and_nothing_between_runs() {
        let w = Watch::default();
        w.request_stop(); // between runs: nothing to stop, and no panic
        let stop = Stop::new();
        w.set_stop(Some(stop.clone()));
        w.request_stop();
        assert!(stop.is_requested());
        w.set_stop(None);
    }

    #[test]
    fn the_remembered_room_is_forgotten() {
        let w = Watch::default();
        let room = Room {
            free_bytes: 1 << 30,
            music_objects: 24,
        };
        w.remember(Some("42".into()), Some(room), Some("Forerunner 165".into()));
        assert_eq!(w.model().as_deref(), Some("Forerunner 165"));
        assert_eq!(w.serial().as_deref(), Some("42"));
        assert_eq!(w.room(), Some(room));
        w.forget();
        assert_eq!(w.serial(), None);
        assert_eq!(w.room(), None);
    }
}
