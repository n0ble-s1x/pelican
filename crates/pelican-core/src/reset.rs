//! After a factory reset: prove the watch is clean, then close the
//! ledger's epoch.
//!
//! Nothing Pelican sends can be deleted — the watch's music library keeps
//! every track until a factory reset, which erases everything on the watch,
//! not just music. After one, this machine's ledger still lists every name
//! and every verified song, so a push would skip songs that are no longer
//! there. [`reset_ledger`] fixes that, and only when the watch proves it:
//! it re-reads `/Music` itself, in the session the caller opened, and
//! appends a `reset` line only if no audio object — readable or stub — is
//! left. A person's "it's clean" is never enough on its own.
//!
//! Read-only on the device: a listing, nothing else.

use anyhow::{Context, Result};
use serde::Serialize;

use crate::garmin::MUSIC_FOLDER;
use crate::ledger::Ledger;
use crate::mtp::Backend;
use crate::watch::audio_objects;

/// What `/Music` holds right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Check {
    /// Audio objects in `/Music`, stubs included (see
    /// [`crate::watch::audio_objects`]).
    pub audio_objects: u32,
    /// No audio object is left: the library was wiped.
    pub clean: bool,
}

/// List `/Music` now and count what is left. A missing `/Music` is clean —
/// a freshly reset watch may not have one yet.
pub fn check(dev: &mut dyn Backend) -> Result<Check> {
    let listing = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    let n = u32::try_from(audio_objects(&listing)).unwrap_or(u32::MAX);
    Ok(Check {
        audio_objects: n,
        clean: n == 0,
    })
}

/// What [`reset_ledger`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Outcome {
    pub clean: bool,
    /// A `reset` line was appended.
    pub reset: bool,
    pub audio_objects: u32,
    /// One or two sentences for the person, whatever happened.
    pub message: String,
}

/// Re-read `/Music`; if it holds no audio objects, append a `reset` line to
/// `ledger` (opened for a run, so exclusively locked). Refuses otherwise.
pub fn reset_ledger(dev: &mut dyn Backend, ledger: &mut Ledger) -> Result<Outcome> {
    let c = check(dev)?;
    if !c.clean {
        let n = c.audio_objects;
        return Ok(Outcome {
            clean: false,
            reset: false,
            audio_objects: n,
            message: format!(
                "The watch still has {n} song file{s} in its music folder, so it has not been \
                 factory-reset. Pelican's record was left as it is. Reset the watch, plug it \
                 back in, and check again.",
                s = if n == 1 { "" } else { "s" }
            ),
        });
    }
    let reason = format!("factory reset confirmed: /{MUSIC_FOLDER} read back with 0 audio objects");
    let reset = ledger.reset(&reason)?;
    let message = if reset {
        "The watch is clean. Pelican has started a fresh record for it — every song can be \
         sent again."
            .to_string()
    } else {
        "The watch is clean, and Pelican's record for it was already fresh.".to_string()
    };
    Ok(Outcome {
        clean: true,
        reset,
        audio_objects: 0,
        message,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Event, Kind};
    use crate::mtp::fake::{Call, FakeDevice};

    fn verified(l: &mut Ledger, counter: u64) {
        let mut e = Event::new(Kind::Verified, counter, &format!("pl{counter:05}-A.mp3"));
        e.source_sha256 = "aa".into();
        l.append(e).unwrap();
    }

    #[test]
    fn refuses_while_audio_remains_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.add_file("Music", "pl00001-A.mp3", b"x");
        dev.add_file("Music", "notes.txt", b"x");
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        verified(&mut l, 1);
        let before = std::fs::read(l.path()).unwrap();

        let out = reset_ledger(dev.backend().as_mut(), &mut l).unwrap();
        assert!(!out.clean && !out.reset);
        assert_eq!(out.audio_objects, 1, "a .txt is not audio");
        assert!(out.message.contains("1 song file "), "{}", out.message);
        assert_eq!(std::fs::read(l.path()).unwrap(), before);
        assert!(l.verified("aa").is_some());
    }

    #[test]
    fn a_stub_counts_as_audio() {
        let dev = FakeDevice::new();
        dev.add_stub("Music");
        let c = check(dev.backend().as_mut()).unwrap();
        assert_eq!(
            c,
            Check {
                audio_objects: 1,
                clean: false
            }
        );
    }

    #[test]
    fn a_clean_watch_resets_the_ledger_and_only_lists() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        verified(&mut l, 7);

        let out = reset_ledger(dev.backend().as_mut(), &mut l).unwrap();
        assert!(out.clean && out.reset, "{out:?}");
        assert!(l.verified("aa").is_none());
        assert_eq!(l.max_counter(), 7);
        assert_eq!(l.last_reset().unwrap().counter, 7);
        // Read-only on the device.
        assert_eq!(dev.calls(), [Call::ListDir("Music".into())]);

        // Again: already fresh, no second line.
        let n = l.events().len();
        let out = reset_ledger(dev.backend().as_mut(), &mut l).unwrap();
        assert!(out.clean && !out.reset);
        assert_eq!(l.events().len(), n);
    }

    #[test]
    fn a_listing_failure_is_an_error_not_a_reset() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.set_faults(crate::mtp::fake::Faults {
            fail_listing: true,
            ..Default::default()
        });
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        verified(&mut l, 1);
        assert!(reset_ledger(dev.backend().as_mut(), &mut l).is_err());
        assert!(l.last_reset().is_none());
    }
}
