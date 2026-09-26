//! The run, end to end, against the fake watch.
//!
//! Everything here is what the hardware acceptance run will check by hand
//! on the real FR165, proven first where a failure costs nothing: names
//! never reused, the reserve line on disk before the write, a bad read-back
//! recorded and retried under a new name, capacity refused before the first
//! byte, staging gone however the run ends — and no delete anywhere.
//!
//! The encoder is a stand-in (ffmpeg's own profile is proven in
//! `ffmpeg_profile.rs`): it writes the title and the source bytes, which is
//! enough for every file to hash differently and for the read-back to mean
//! something.

use std::cell::Cell;
use std::path::{Path, PathBuf};

use anyhow::Result;
use pelican_core::ledger::{Kind, Ledger};
use pelican_core::mtp::fake::{Call, FakeDevice, Faults};
use pelican_core::mtp::Backend;
use pelican_core::source;
use pelican_core::transcode::tags::{Overrides, Resolved};
use pelican_core::transfer::{self, Env, Options, Outcome, Report, Skip};

const SERIAL: &str = "3456789012";

struct Harness {
    tmp: tempfile::TempDir,
    dev: FakeDevice,
    opens: Cell<usize>,
}

fn fake_encode(src: &Path, dst: &Path, tags: &Resolved) -> Result<()> {
    let mut out = format!("ID3|{}|", tags.title).into_bytes();
    out.extend(std::fs::read(src)?);
    std::fs::write(dst, out)?;
    Ok(())
}

fn failing_encode(_: &Path, _: &Path, _: &Resolved) -> Result<()> {
    anyhow::bail!("ffmpeg exited 1: Invalid data found when processing input")
}

impl Harness {
    fn new() -> Self {
        Self {
            tmp: tempfile::tempdir().unwrap(),
            dev: FakeDevice::new(),
            opens: Cell::new(0),
        }
    }

    fn music(&self) -> PathBuf {
        self.tmp.path().join("music")
    }

    /// Write a source file under `music/`, contents unique per path unless
    /// `body` says otherwise.
    fn source(&self, rel: &str, body: Option<&str>) -> PathBuf {
        let p = self.music().join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, body.unwrap_or(rel)).unwrap();
        p
    }

    fn data(&self) -> PathBuf {
        self.tmp.path().join("data")
    }

    fn cache(&self) -> PathBuf {
        self.tmp.path().join("cache")
    }

    fn ledger_path(&self) -> PathBuf {
        Ledger::path_in(&self.data(), SERIAL).unwrap()
    }

    fn ledger_kinds(&self) -> Vec<(Kind, String)> {
        Ledger::read(&self.data(), SERIAL)
            .unwrap()
            .events()
            .iter()
            .map(|e| (e.event, e.remote.clone()))
            .collect()
    }

    fn run(&self, paths: &[PathBuf], opts: Options) -> Result<Report> {
        self.run_with(paths, opts, &fake_encode)
    }

    fn run_with(
        &self,
        paths: &[PathBuf],
        opts: Options,
        encode: &dyn Fn(&Path, &Path, &Resolved) -> Result<()>,
    ) -> Result<Report> {
        let sources = source::expand(paths).unwrap();
        let entries = transfer::plan(sources, &Overrides::default());
        let mut ledger = Ledger::open(&self.data(), SERIAL).unwrap();
        let mut lines = Vec::new();
        let dev = self.dev.clone();
        let report = transfer::push(
            entries,
            &mut ledger,
            || -> Result<Box<dyn Backend>> {
                self.opens.set(self.opens.get() + 1);
                Ok(dev.backend())
            },
            opts,
            Env {
                staging_base: &self.cache(),
                encode,
                progress: &mut |p| lines.push(format!("{p:?}")),
            },
        );
        // However the run went, its staging dir is gone (R8).
        self.assert_staging_empty();
        // And no text it produced tells anyone to delete anything (R6).
        for line in &lines {
            assert!(!line.to_lowercase().contains("delete"), "{line}");
        }
        if let Err(e) = &report {
            assert!(!format!("{e:#}").to_lowercase().contains("delete"), "{e:#}");
        }
        report
    }

    fn assert_staging_empty(&self) {
        let root = self.cache().join("staging");
        let left: Vec<_> = match std::fs::read_dir(&root) {
            Ok(rd) => rd.map(|e| e.unwrap().path()).collect(),
            Err(_) => Vec::new(),
        };
        assert!(left.is_empty(), "staging left behind: {left:?}");
    }

    fn uploads(&self) -> Vec<String> {
        self.dev
            .calls()
            .into_iter()
            .filter_map(|c| match c {
                Call::Upload(_, name) => Some(name),
                _ => None,
            })
            .collect()
    }
}

fn verified(r: &Report) -> Vec<String> {
    r.files
        .iter()
        .filter_map(|f| match &f.outcome {
            Outcome::Verified { remote, .. } => Some(remote.clone()),
            _ => None,
        })
        .collect()
}

fn one_retry() -> Options {
    Options::default()
}

#[test]
fn a_clean_run_verifies_every_file_in_one_session() {
    let h = Harness::new();
    h.source("Sea of Thieves/01 - Grogmire.wav", None);
    h.source("Sea of Thieves/02 - Maiden Voyage.wav", None);
    let report = h.run(&[h.music()], one_retry()).unwrap();

    assert_eq!(report.tally().verified, 2);
    assert_eq!(
        verified(&report),
        ["pl00001-Grogmire.mp3", "pl00002-Maiden Voyage.mp3"]
    );
    assert_eq!(h.opens.get(), 1, "one session per run, not per file");

    // Listing and capacity come before the first write, and every upload
    // is followed by its own read-back.
    let calls = h.dev.calls();
    assert_eq!(
        calls,
        vec![
            Call::ListDir("Music".into()),
            Call::FreeSpace,
            Call::EnsureFolder("Music".into()),
            Call::Upload("Music".into(), "pl00001-Grogmire.mp3".into()),
            Call::Download("Music/pl00001-Grogmire.mp3".into()),
            Call::Upload("Music".into(), "pl00002-Maiden Voyage.mp3".into()),
            Call::Download("Music/pl00002-Maiden Voyage.mp3".into()),
        ]
    );

    // What landed is exactly the transcode, tags included.
    let on_watch = h.dev.files("Music");
    let (_, bytes) = on_watch
        .iter()
        .find(|(n, _)| n == "pl00002-Maiden Voyage.mp3")
        .unwrap();
    assert!(bytes.starts_with(b"ID3|Maiden Voyage|"));

    use Kind::*;
    assert_eq!(
        h.ledger_kinds(),
        vec![
            (Reserve, "pl00001-Grogmire.mp3".into()),
            (Verified, "pl00001-Grogmire.mp3".into()),
            (Reserve, "pl00002-Maiden Voyage.mp3".into()),
            (Verified, "pl00002-Maiden Voyage.mp3".into()),
        ]
    );
    let l = Ledger::read(&h.data(), SERIAL).unwrap();
    let v = &l.events()[3];
    assert_eq!(v.title, "Maiden Voyage");
    assert_eq!(v.album.as_deref(), Some("Sea of Thieves"));
    assert_eq!(v.artist.as_deref(), Some("Sea of Thieves"));
    assert_eq!(v.bytes, Some(bytes.len() as u64));
    assert_eq!(
        v.upload_sha256.as_deref(),
        Some(pelican_core::hash::bytes(bytes).as_str())
    );
    assert!(v.source.ends_with("02 - Maiden Voyage.wav"));
}

/// R4: the reserve line is on disk — not buffered, not pending — at the
/// moment the upload starts. Observed from inside the fake's upload.
#[test]
fn the_reserve_line_is_on_disk_before_the_write() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    let ledger_file = h.ledger_path();
    let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen_w = std::sync::Arc::clone(&seen);
    h.dev.on_upload(move |_, name| {
        let text = std::fs::read_to_string(&ledger_file).unwrap();
        let last: serde_json::Value = serde_json::from_str(text.lines().last().unwrap()).unwrap();
        seen_w.lock().unwrap().push((
            last["event"].as_str().unwrap().to_string(),
            last["remote"].clone(),
            name.to_string(),
        ));
    });
    h.run(&[h.music()], one_retry()).unwrap();
    let seen = seen.lock().unwrap();
    assert_eq!(seen.len(), 2);
    for (event, remote, name) in seen.iter() {
        assert_eq!(event, "reserve");
        assert_eq!(remote, name);
    }
}

/// R3: names already on the watch (any case), names in the ledger
/// (any status), and stubs are never written; the counter starts above
/// them all.
#[test]
fn taken_names_are_never_written() {
    let h = Harness::new();
    h.dev.add_file("Music", "PL00007-Old.MP3", b"x");
    h.dev.add_file("Music", "Foreign.mp3", b"y");
    h.dev.add_stub("Music");
    {
        let mut l = Ledger::open(&h.data(), SERIAL).unwrap();
        let mut e = pelican_core::ledger::Event::new(Kind::Failed, 12, "pl00012-Lost.mp3");
        e.source_sha256 = "00".into();
        l.append(e).unwrap();
    }
    let before: Vec<String> = h
        .dev
        .objects()
        .iter()
        .map(|o| pelican_core::mtp::fold_name(&o.name))
        .collect();
    h.source("X/01 - Old.wav", None);
    h.source("X/02 - Lost.wav", None);
    let report = h.run(&[h.music()], one_retry()).unwrap();
    assert_eq!(verified(&report), ["pl00013-Old.mp3", "pl00014-Lost.mp3"]);
    for name in h.uploads() {
        assert!(
            !before.contains(&pelican_core::mtp::fold_name(&name)),
            "{name} was already on the watch"
        );
    }
    // Nothing on the watch became a stub: no name was reused.
    let stubs = h.dev.objects().iter().filter(|o| o.broken).count();
    assert_eq!(stubs, 1, "only the stub that was already there");
}

/// R5: verified audio is skipped next time without opening the device;
/// `--resend` sends it again under new names.
#[test]
fn already_verified_audio_is_skipped_unless_resent() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    h.run(&[h.music()], one_retry()).unwrap();
    assert_eq!(h.opens.get(), 1);

    // The same audio, moved and renamed: the hash is the identity.
    let moved = h.tmp.path().join("elsewhere/renamed.wav");
    std::fs::create_dir_all(moved.parent().unwrap()).unwrap();
    std::fs::copy(h.music().join("A/01 - One.wav"), &moved).unwrap();
    let report = h.run(&[h.music(), moved.clone()], one_retry()).unwrap();
    assert_eq!(report.tally().skipped, 3);
    assert_eq!(
        h.opens.get(),
        1,
        "nothing to send, so the watch is not opened"
    );
    assert_eq!(
        report.files[2].outcome,
        Outcome::Skipped(Skip::AlreadyOnWatch {
            remote: "pl00001-One.mp3".into()
        })
    );

    let report = h
        .run(
            &[h.music()],
            Options {
                resend: true,
                ..one_retry()
            },
        )
        .unwrap();
    assert_eq!(verified(&report), ["pl00003-One.mp3", "pl00004-Two.mp3"]);
    assert_eq!(
        h.dev.files("Music").len(),
        4,
        "nothing overwritten, nothing removed"
    );
}

#[test]
fn the_same_audio_twice_in_one_run_is_sent_once() {
    let h = Harness::new();
    h.source("A/01 - One.wav", Some("same"));
    h.source("B/01 - One copy.wav", Some("same"));
    let report = h.run(&[h.music()], one_retry()).unwrap();
    let t = report.tally();
    assert_eq!((t.verified, t.skipped), (1, 1));
    assert!(matches!(
        &report.files[1].outcome,
        Outcome::Skipped(Skip::DuplicateOf(p)) if p.ends_with("A/01 - One.wav")
    ));
}

/// R6: every way a write can go wrong is recorded as `failed`, the name
/// stays burned, and the retry goes out under a fresh counter.
#[test]
fn a_bad_attempt_is_failed_and_retried_under_a_new_name() {
    for (label, faults) in [
        (
            "corrupt on the watch",
            Faults {
                corrupt_uploads: 1,
                ..Default::default()
            },
        ),
        (
            "corrupt on the way back",
            Faults {
                corrupt_downloads: 1,
                ..Default::default()
            },
        ),
        (
            "upload error",
            Faults {
                fail_uploads: 1,
                ..Default::default()
            },
        ),
        (
            "read-back error",
            Faults {
                fail_downloads: 1,
                ..Default::default()
            },
        ),
    ] {
        let h = Harness::new();
        h.source("A/01 - One.wav", None);
        h.dev.set_faults(faults);
        let report = h.run(&[h.music()], one_retry()).unwrap();
        assert_eq!(verified(&report), ["pl00002-One.mp3"], "{label}");
        assert_eq!(
            h.uploads(),
            ["pl00001-One.mp3", "pl00002-One.mp3"],
            "{label}: the failed name is never sent again"
        );
        let l = Ledger::read(&h.data(), SERIAL).unwrap();
        let kinds: Vec<_> = l.events().iter().map(|e| (e.event, e.counter)).collect();
        use Kind::*;
        assert_eq!(
            kinds,
            [(Reserve, 1), (Failed, 1), (Reserve, 2), (Verified, 2)],
            "{label}"
        );
        let reason = l.events()[1].reason.as_deref().unwrap();
        assert!(!reason.is_empty(), "{label}");
        assert!(
            !reason.to_lowercase().contains("delete"),
            "{label}: {reason}"
        );
    }
}

/// Where a bad object landed, it stays: there is no delete to call.
#[test]
fn a_corrupt_object_is_left_where_it_is() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    h.run(&[h.music()], one_retry()).unwrap();
    let names: Vec<_> = h.dev.files("Music").into_iter().map(|(n, _)| n).collect();
    assert_eq!(names, ["pl00001-One.mp3", "pl00002-One.mp3"]);
}

#[test]
fn retries_run_out_and_the_file_fails() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    h.dev.set_faults(Faults {
        corrupt_uploads: 2,
        ..Default::default()
    });
    let report = h.run(&[h.music()], one_retry()).unwrap();
    match &report.files[0].outcome {
        Outcome::Failed { reason, remotes } => {
            assert!(reason.contains("read-back mismatch"), "{reason}");
            assert_eq!(remotes, &["pl00001-One.mp3", "pl00002-One.mp3"]);
        }
        o => panic!("{o:?}"),
    }
    // The next file is unaffected and takes the next counter.
    assert_eq!(verified(&report), ["pl00003-Two.mp3"]);
    let t = report.tally();
    assert_eq!((t.verified, t.failed), (1, 1));

    // `--retries 0`: one attempt, no second name burned.
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.dev.set_faults(Faults {
        fail_uploads: 1,
        ..Default::default()
    });
    let report = h
        .run(
            &[h.music()],
            Options {
                retries: 0,
                ..one_retry()
            },
        )
        .unwrap();
    assert_eq!(report.tally().failed, 1);
    assert_eq!(h.uploads(), ["pl00001-One.mp3"]);
}

/// R7: not enough space — refused before the first write, nothing
/// reserved, nothing sent.
#[test]
fn a_run_that_does_not_fit_is_refused_before_any_write() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    // The transcode is a few dozen bytes; the 2 MiB margin is what fails.
    h.dev.set_space(2 << 20, 4 << 30);
    let err = h.run(&[h.music()], one_retry()).unwrap_err();
    assert!(format!("{err:#}").contains("not enough room"), "{err:#}");
    assert!(h.uploads().is_empty());
    assert!(!h.dev.calls().contains(&Call::EnsureFolder("Music".into())));
    assert!(h.ledger_kinds().is_empty(), "no name reserved");
}

#[test]
fn a_run_past_500_objects_is_refused_before_any_write() {
    let h = Harness::new();
    for i in 0..499 {
        h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
    }
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    let err = h.run(&[h.music()], one_retry()).unwrap_err();
    assert!(format!("{err:#}").contains("500"), "{err:#}");
    assert!(h.uploads().is_empty());
    assert!(h.ledger_kinds().is_empty());

    // Exactly 500 is allowed.
    let h = Harness::new();
    for i in 0..498 {
        h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
    }
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    assert_eq!(
        h.run(&[h.music()], one_retry()).unwrap().tally().verified,
        2
    );
}

/// Stubs are objects in the library and count toward the 500.
#[test]
fn stubs_count_toward_the_object_limit() {
    let h = Harness::new();
    for i in 0..499 {
        if i % 2 == 0 {
            h.dev.add_stub("Music");
        } else {
            h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
        }
    }
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    assert!(h.run(&[h.music()], one_retry()).is_err());
}

/// A retry is a write and gets its own capacity check.
#[test]
fn no_retry_when_the_watch_is_full() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    // What `fake_encode` writes: the title header, then the source body.
    let staged_len = "ID3|One|A/01 - One.wav".len() as u64;
    // Room for exactly one attempt plus the margin.
    h.dev.set_space((2 << 20) + staged_len + 1, 4 << 30);
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let report = h.run(&[h.music()], one_retry()).unwrap();
    match &report.files[0].outcome {
        Outcome::Failed { reason, remotes } => {
            assert!(reason.contains("not retried"), "{reason}");
            assert_eq!(remotes.len(), 1);
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(h.uploads().len(), 1);
}

#[test]
fn a_listing_failure_stops_the_run_before_any_write() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.dev.set_faults(Faults {
        fail_listing: true,
        ..Default::default()
    });
    assert!(h.run(&[h.music()], one_retry()).is_err());
    assert!(h.uploads().is_empty());
    assert!(h.ledger_kinds().is_empty());
}

/// R8: staging is gone after an error that aborts the run (checked inside
/// `Harness::run` for every test), including when the device will not
/// open at all.
#[test]
fn staging_is_gone_when_the_device_will_not_open() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    let sources = source::expand(&[h.music()]).unwrap();
    let entries = transfer::plan(sources, &Overrides::default());
    let mut ledger = Ledger::open(&h.data(), SERIAL).unwrap();
    let staged_while_running = Cell::new(false);
    let cache = h.cache();
    let err = transfer::push(
        entries,
        &mut ledger,
        || {
            // The transcodes exist at the moment the session opens…
            let dirs: Vec<_> = std::fs::read_dir(cache.join("staging")).unwrap().collect();
            staged_while_running.set(dirs.len() == 1);
            anyhow::bail!("could not get exclusive access to the watch")
        },
        one_retry(),
        Env {
            staging_base: &h.cache(),
            encode: &fake_encode,
            progress: &mut |_| {},
        },
    )
    .unwrap_err();
    assert!(format!("{err}").contains("exclusive access"));
    assert!(staged_while_running.get());
    // …and not after.
    h.assert_staging_empty();
}

#[test]
fn refused_and_unencodable_files_fail_without_touching_the_watch() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    let report = h
        .run_with(&[h.music()], one_retry(), &failing_encode)
        .unwrap();
    match &report.files[0].outcome {
        Outcome::Failed { reason, remotes } => {
            assert!(reason.contains("transcode failed"), "{reason}");
            assert!(remotes.is_empty());
        }
        o => panic!("{o:?}"),
    }
    assert_eq!(h.opens.get(), 0);
    assert!(h.ledger_kinds().is_empty());

    // A file whose title resolves empty is refused at plan time.
    let h = Harness::new();
    // ℗ is stripped by `sanitize_tag_value`, leaving nothing to call it.
    let bad = h.source("A/\u{2117}.wav", None);
    let good = h.source("A/02 - Two.wav", None);
    let report = h.run(&[bad, good], one_retry()).unwrap();
    let t = report.tally();
    assert_eq!((t.verified, t.failed), (1, 1));
}
