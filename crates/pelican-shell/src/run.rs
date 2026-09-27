//! A push, from a planned run to the events the window sees.
//!
//! Split from `commands::push` so everything but finding the watch runs in
//! tests against the in-memory `FakeDevice`: the ledger, the stop switch,
//! the event mapping and the throttle are exercised end to end, and the
//! real device is never opened.

use std::path::Path;
use std::time::{Duration, Instant};

use anyhow::Result;

use pelican_core::ledger::Ledger;
use pelican_core::mtp::Backend;
use pelican_core::transfer::{self, Encode, Env, Options, PlanEntry, Stop, Tally};

use crate::dto::{self, Payload, Throttle};

/// ~30 upload ticks a second is a smooth bar and a quiet IPC channel.
const UPLOAD_TICK: Duration = Duration::from_millis(33);

/// Which watch, and where its ledger and the run's staging live.
pub struct Target<'a> {
    pub serial: &'a str,
    /// Where the per-watch ledger lives.
    pub data: &'a Path,
    /// Where the run's staging directory is made.
    pub cache: &'a Path,
}

/// Run `entries` to the watch and forward its progress to `emit`.
///
/// The core ends an `Ok` run with its own `finished`; an `Err` is returned
/// for the caller to report as the run's `error` event.
pub fn push(
    entries: Vec<PlanEntry>,
    to: Target<'_>,
    options: Options,
    open: impl FnOnce() -> Result<Box<dyn Backend>>,
    encode: Encode<'_>,
    stop: Stop,
    emit: &mut (dyn FnMut(Payload) + Send),
) -> Result<Tally> {
    let mut ledger = Ledger::open(to.data, to.serial)?;
    let mut throttle = Throttle::new(UPLOAD_TICK);
    let mut progress = |p| {
        if let Some(payload) = dto::map_progress(p) {
            if throttle.admit(&payload, Instant::now()) {
                emit(payload);
            }
        }
    };
    let report = transfer::push(
        entries,
        &mut ledger,
        open,
        options,
        Env {
            staging_base: to.cache,
            encode,
            progress: &mut progress,
            stop,
        },
    )?;
    Ok(report.tally())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelican_core::mtp::fake::FakeDevice;
    use pelican_core::source;
    use pelican_core::transcode::tags::{Overrides, Resolved};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    /// Stands in for ffmpeg: the "transcode" is the source's bytes.
    fn copy(src: &Path, dst: &Path, _: &Resolved) -> Result<()> {
        std::fs::copy(src, dst)?;
        Ok(())
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        album: PathBuf,
        data: PathBuf,
        cache: PathBuf,
    }

    fn fixture() -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let album = tmp.path().join("Sea of Thieves");
        std::fs::create_dir(&album).unwrap();
        std::fs::write(album.join("01 - Maiden Voyage.wav"), b"one").unwrap();
        std::fs::write(album.join("02 - Spectral Sails.wav"), b"two").unwrap();
        let data = tmp.path().join("data");
        let cache = tmp.path().join("cache");
        std::fs::create_dir(&cache).unwrap();
        Fixture {
            album,
            data,
            cache,
            _tmp: tmp,
        }
    }

    fn entries(f: &Fixture) -> Vec<PlanEntry> {
        let sources = source::expand(std::slice::from_ref(&f.album)).unwrap();
        transfer::plan(sources, &Overrides::default())
    }

    fn run(f: &Fixture, dev: &FakeDevice, stop: Stop) -> (Result<Tally>, Vec<Payload>) {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        let mut emit = move |p| sink.lock().unwrap().push(p);
        let dev = dev.clone();
        let r = push(
            entries(f),
            Target {
                serial: "42",
                data: &f.data,
                cache: &f.cache,
            },
            Options::default(),
            move || Ok(dev.backend()),
            &copy,
            stop,
            &mut emit,
        );
        let events = seen.lock().unwrap().clone();
        (r, events)
    }

    #[test]
    fn a_run_reports_each_file_proven_then_finishes() {
        let f = fixture();
        let dev = FakeDevice::new();
        let (r, events) = run(&f, &dev, Stop::new());
        let t = r.unwrap();
        assert_eq!(
            (t.verified, t.skipped, t.failed, t.stopped),
            (2, 0, 0, false)
        );

        let kinds: Vec<&str> = events
            .iter()
            .map(|p| match p {
                Payload::Transcoding { .. } => "transcoding",
                Payload::Sending { .. } => "sending",
                Payload::Uploading { .. } => "uploading",
                Payload::Done { .. } => "done",
                Payload::Finished { .. } => "finished",
                Payload::Error { .. } => "error",
            })
            .filter(|k| *k != "uploading")
            .collect();
        assert_eq!(
            kinds,
            [
                "transcoding",
                "transcoding",
                "sending",
                "done",
                "sending",
                "done",
                "finished"
            ]
        );
        let proven: Vec<_> = events
            .iter()
            .filter_map(|p| match p {
                Payload::Done {
                    outcome: dto::DoneKind::Verified,
                    sha256: Some(h),
                    remote: Some(r),
                    ..
                } => Some((r.clone(), h.clone())),
                _ => None,
            })
            .collect();
        assert_eq!(proven.len(), 2, "{events:?}");
        assert_eq!(proven[0].1, pelican_core::hash::bytes(b"one"));
        assert_eq!(dev.files("Music").len(), 2);

        // Sent again, the ledger now has both: skipped, and nothing opened.
        let (r, events) = run(&f, &dev, Stop::new());
        let t = r.unwrap();
        assert_eq!((t.verified, t.skipped), (0, 2));
        assert!(matches!(
            events.last(),
            Some(Payload::Finished { skipped: 2, .. })
        ));
        assert_eq!(dev.files("Music").len(), 2);
    }

    /// The watch stops answering mid-run: the run ends in an error whose
    /// text, as the window gets it, is the replug instruction.
    #[test]
    fn a_wedged_watch_ends_the_run_with_the_replug_instruction() {
        use pelican_core::error::REPLUG;
        use pelican_core::mtp::fake::Faults;
        let f = fixture();
        let dev = FakeDevice::new();
        dev.set_faults(Faults {
            wedged: true,
            ..Faults::default()
        });
        let (r, events) = run(&f, &dev, Stop::new());
        let e = r.expect_err("a wedged watch cannot succeed");
        assert_eq!(dto::explain_device_error(&e), REPLUG);
        assert!(
            !events.iter().any(|p| matches!(p, Payload::Finished { .. })),
            "{events:?}"
        );
    }

    #[test]
    fn a_stop_before_the_run_sends_nothing_and_says_so() {
        let f = fixture();
        let dev = FakeDevice::new();
        let stop = Stop::new();
        stop.request();
        let (r, events) = run(&f, &dev, stop);
        let t = r.unwrap();
        assert_eq!((t.verified, t.skipped, t.stopped), (0, 2, true));
        assert!(dev.files("Music").is_empty());
        assert!(dev.calls().is_empty(), "the watch was opened");
        assert!(events.iter().all(|p| !matches!(p, Payload::Sending { .. })));
        assert!(matches!(
            events.last(),
            Some(Payload::Finished { stopped: true, .. })
        ));
    }
}
