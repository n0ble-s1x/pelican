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
use pelican_core::transfer::{
    self, Env, Mix, Options, Outcome, Progress, Report, Skip, Stop, Verdict,
};

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
        self.run_entries(entries, opts, encode, &Stop::new(), &mut |_| {})
    }

    /// Send `paths` as the mix `name`, with `ov`.
    fn run_mix(
        &self,
        paths: &[PathBuf],
        name: &str,
        ov: &Overrides,
        opts: Options,
        encode: &dyn Fn(&Path, &Path, &Resolved) -> Result<()>,
    ) -> Result<Report> {
        let sources = source::expand(paths).unwrap();
        let mix = Mix { name: name.into() };
        let entries = transfer::plan_with(sources, ov, Some(&mix)).unwrap();
        self.run_entries(entries, opts, encode, &Stop::new(), &mut |_| {})
    }

    /// The run, with `on` seeing every event first (and free to pull
    /// `stop`).
    fn run_entries(
        &self,
        entries: Vec<transfer::PlanEntry>,
        opts: Options,
        encode: &dyn Fn(&Path, &Path, &Resolved) -> Result<()>,
        stop: &Stop,
        on: &mut (dyn FnMut(&Progress) + Send),
    ) -> Result<Report> {
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
                progress: &mut |p| {
                    on(&p);
                    lines.push(format!("{p:?}"));
                },
                stop: stop.clone(),
            },
        );
        // A run that returns ends with its tally, and only then.
        if report.is_ok() {
            assert!(lines.last().unwrap().starts_with("Finished"), "{lines:?}");
        }
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
    // 12 is the highest known counter; the one stub moves it one further,
    // since its hidden name could be 13.
    assert_eq!(verified(&report), ["pl00014-Old.mp3", "pl00015-Lost.mp3"]);
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

    // The same audio, moved and renamed: the hash is the identity, within
    // its album.
    let moved = h.tmp.path().join("elsewhere/A/renamed.wav");
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

/// The limit is on audio: readable non-audio files in /Music do not count,
/// so 480 tracks plus 20 other files leave room for 5 more.
#[test]
fn readable_non_audio_does_not_count_toward_the_object_limit() {
    let h = Harness::new();
    for i in 0..480 {
        h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
    }
    for i in 0..20 {
        let ext = ["m3u8", "txt", "jpg", "DAT"][i % 4];
        h.dev.add_file("Music", &format!("other{i}.{ext}"), b"x");
    }
    let listing = h.dev.backend().list_dir("Music").unwrap();
    assert_eq!(pelican_core::watch::audio_objects(&listing), 480);
    for n in 1..=5 {
        h.source(&format!("A/0{n} - Song.wav"), None);
    }
    assert_eq!(
        h.run(&[h.music()], one_retry()).unwrap().tally().verified,
        5
    );
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

/// The reviewer's case: 495 tracks, 5 planned, the first read-back bad.
/// The up-front check reserved exactly five slots; the failed attempt
/// spent one, so a retry would take a later file's and the run would end
/// at 501. The retry is refused and the library stays at 500.
#[test]
fn a_retry_does_not_spend_a_later_files_slot() {
    let h = Harness::new();
    for i in 0..495 {
        h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
    }
    for n in 1..=5 {
        h.source(&format!("A/0{n} - Song.wav"), None);
    }
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let report = h.run(&[h.music()], one_retry()).unwrap();
    match &report.files[0].outcome {
        Outcome::Failed { reason, remotes } => {
            assert!(reason.contains("not retried"), "{reason}");
            assert!(reason.contains("4 more are queued"), "{reason}");
            assert_eq!(remotes.len(), 1);
        }
        o => panic!("{o:?}"),
    }
    let t = report.tally();
    assert_eq!((t.verified, t.failed), (4, 1));
    let listing = h.dev.backend().list_dir("Music").unwrap();
    assert_eq!(pelican_core::watch::audio_objects(&listing), 500);
}

/// With room to spare the same failure is retried as usual: the check
/// holds back what later files need, not more.
#[test]
fn a_retry_with_room_for_everything_still_happens() {
    let h = Harness::new();
    for i in 0..494 {
        h.dev.add_file("Music", &format!("t{i}.mp3"), b"x");
    }
    for n in 1..=5 {
        h.source(&format!("A/0{n} - Song.wav"), None);
    }
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let report = h.run(&[h.music()], one_retry()).unwrap();
    assert_eq!(report.tally().verified, 5);
    let listing = h.dev.backend().list_dir("Music").unwrap();
    assert_eq!(pelican_core::watch::audio_objects(&listing), 500);
}

/// Bytes the same way: room for both files plus the margin and nothing
/// more. A retry of the first would leave the second without room.
#[test]
fn a_retry_does_not_spend_a_later_files_bytes() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    let staged = ("ID3|One|A/01 - One.wav".len() + "ID3|Two|A/02 - Two.wav".len()) as u64;
    h.dev.set_space((2 << 20) + staged, 4 << 30);
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let report = h.run(&[h.music()], one_retry()).unwrap();
    match &report.files[0].outcome {
        Outcome::Failed { reason, .. } => {
            assert!(reason.contains("not retried"), "{reason}");
            assert!(reason.contains("1 file(s) queued after"), "{reason}");
        }
        o => panic!("{o:?}"),
    }
    assert!(
        matches!(report.files[1].outcome, Outcome::Verified { .. }),
        "{:?}",
        report.files[1]
    );
    assert_eq!(h.uploads().len(), 2);
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
            stop: Stop::new(),
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

/// The event stream a front-end sees for one file that fails once and then
/// verifies: every attempt announced with its burned name, upload bytes in
/// between, and exactly one `Done` per planned file — the last event.
#[test]
fn a_run_reports_each_attempt_as_it_happens() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - One.wav", Some("A/01 - One.wav"));
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let entries = transfer::plan(source::expand(&[h.music()]).unwrap(), &Overrides::default());
    let mut ledger = Ledger::open(&h.data(), SERIAL).unwrap();
    let mut events = Vec::new();
    let dev = h.dev.clone();
    transfer::push(
        entries,
        &mut ledger,
        || Ok(dev.backend()),
        one_retry(),
        Env {
            staging_base: &h.cache(),
            encode: &fake_encode,
            progress: &mut |p| events.push(p),
            stop: Stop::new(),
        },
    )
    .unwrap();

    let kinds: Vec<String> = events
        .iter()
        .map(|p| {
            serde_json::to_value(p).unwrap()["kind"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    // The copy is decided before anything is transcoded, so its `Done`
    // comes first.
    assert_eq!(kinds.first().map(String::as_str), Some("done"));
    let sending: Vec<_> = events
        .iter()
        .filter_map(|p| match p {
            Progress::Sending {
                remote, attempt, ..
            } => Some((remote.as_str(), *attempt)),
            _ => None,
        })
        .collect();
    assert_eq!(sending, [("pl00001-One.mp3", 1), ("pl00002-One.mp3", 2)]);
    assert!(events.iter().any(|p| matches!(
        p,
        Progress::Uploading { sent, total, .. } if sent == total && *total > 0
    )));
    assert!(events
        .iter()
        .any(|p| matches!(p, Progress::AttemptFailed { retrying: true, .. })));
    let done = kinds.iter().filter(|k| *k == "done").count();
    assert_eq!(done, 2, "one Done per planned file");
    assert_eq!(kinds.last().map(String::as_str), Some("finished"));

    // Serialized, an outcome reads the way a UI would switch on it.
    let n = events.len();
    let last = serde_json::to_value(&events[n - 2]).unwrap();
    assert_eq!(last["index"], 0);
    assert_eq!(last["outcome"]["kind"], "verified");
    assert_eq!(last["outcome"]["remote"], "pl00002-One.mp3");
    assert_eq!(last["outcome"]["sha256"].as_str().unwrap().len(), 64);
    let fin = serde_json::to_value(&events[n - 1]).unwrap();
    assert_eq!(
        fin,
        serde_json::json!({"kind": "finished", "verified": 1, "skipped": 1, "failed": 0, "stopped": false})
    );
    let first = serde_json::to_value(&events[0]).unwrap();
    assert_eq!(first["outcome"]["kind"], "skipped");
    assert!(first["outcome"]["duplicate_of"]
        .as_str()
        .unwrap()
        .ends_with("01 - One.wav"));
}

/// `preview` is steps 1–2 of the run, and touches nothing.
#[test]
fn preview_decides_what_the_run_would_do() {
    let h = Harness::new();
    let one = h.source("A/01 - One.wav", None);
    h.source("A/02 - Again.wav", Some("A/01 - One.wav"));
    h.source("A/\u{2117}.wav", None);
    let entries = transfer::plan(source::expand(&[h.music()]).unwrap(), &Overrides::default());
    let mut l = Ledger::open(&h.data(), SERIAL).unwrap();
    let mut e = pelican_core::ledger::Event::new(Kind::Verified, 9, "pl00009-One.mp3");
    e.source_sha256 = pelican_core::hash::file(&one).unwrap();
    e.album = Some("A".into());
    l.append(e).unwrap();

    let kinds = |v: Vec<Verdict>| -> Vec<String> {
        v.iter()
            .map(|v| {
                serde_json::to_value(v).unwrap()["kind"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    };
    // Plan order: 01, 02, ℗ (sorted by name).
    assert_eq!(
        kinds(transfer::preview(&entries, Some(&l), false)),
        ["skip", "skip", "refused"]
    );
    assert!(matches!(
        &transfer::preview(&entries, Some(&l), false)[0],
        Verdict::Skip(Skip::AlreadyOnWatch { remote }) if remote == "pl00009-One.mp3"
    ));
    // `--resend`, or no ledger to ask: the first copy goes, the second is a repeat.
    for v in [
        transfer::preview(&entries, Some(&l), true),
        transfer::preview(&entries, None, false),
    ] {
        assert!(matches!(v[0], Verdict::Send { .. }), "{v:?}");
        assert!(matches!(v[1], Verdict::Skip(Skip::DuplicateOf(_))), "{v:?}");
    }
    assert_eq!(h.dev.calls(), [], "a preview touches no device");
    assert!(!h.cache().exists(), "a preview transcodes nothing");
}

fn plan_of(paths: &[PathBuf]) -> Vec<transfer::PlanEntry> {
    transfer::plan(source::expand(paths).unwrap(), &Overrides::default())
}

fn stopped(r: &Report) -> Vec<usize> {
    r.files
        .iter()
        .filter(|f| f.outcome == Outcome::Skipped(Skip::Stopped {}))
        .map(|f| f.index)
        .collect()
}

/// A stop lands between files: the file in flight is proven, the rest are
/// never reserved, and the tally says the run was stopped.
#[test]
fn a_stop_ends_the_run_between_files() {
    let h = Harness::new();
    for n in 1..=3 {
        h.source(&format!("A/0{n} - T{n}.wav"), None);
    }
    let stop = Stop::new();
    let pull = stop.clone();
    let mut fin = None;
    let report = h
        .run_entries(
            plan_of(&[h.music()]),
            one_retry(),
            &fake_encode,
            &stop,
            &mut |p| match p {
                // Asked mid-upload: the upload and its read-back still finish.
                Progress::Uploading { index: 0, .. } => pull.request(),
                Progress::Finished(t) => fin = Some(*t),
                _ => {}
            },
        )
        .unwrap();

    assert_eq!(verified(&report), ["pl00001-T1.mp3"]);
    assert_eq!(stopped(&report), [1, 2]);
    let t = report.tally();
    assert_eq!(
        (t.verified, t.skipped, t.failed, t.stopped),
        (1, 2, 0, true)
    );
    assert_eq!(fin, Some(t));
    assert_eq!(
        report.files[1].outcome.reason().as_deref(),
        Some("stopped before it was sent")
    );
    // Every reserve has its outcome; the stopped files have no line at all.
    assert_eq!(
        h.ledger_kinds(),
        [
            (Kind::Reserve, "pl00001-T1.mp3".into()),
            (Kind::Verified, "pl00001-T1.mp3".into()),
        ]
    );
    assert_eq!(
        Ledger::read(&h.data(), SERIAL).unwrap().totals().unresolved,
        0
    );
    assert_eq!(h.uploads(), ["pl00001-T1.mp3"]);

    // The stopped files go next time, under the next names.
    let report = h.run(&[h.music()], one_retry()).unwrap();
    assert_eq!(verified(&report), ["pl00002-T2.mp3", "pl00003-T3.mp3"]);
}

/// A stop during the transcodes never opens the watch.
#[test]
fn a_stop_before_the_session_opens_nothing() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    let stop = Stop::new();
    let pull = stop.clone();
    let report = h
        .run_entries(
            plan_of(&[h.music()]),
            one_retry(),
            &fake_encode,
            &stop,
            &mut |p| {
                if matches!(p, Progress::Transcoding { n: 1, .. }) {
                    pull.request();
                }
            },
        )
        .unwrap();
    assert_eq!(stopped(&report), [0, 1]);
    assert!(report.tally().stopped);
    assert_eq!(h.opens.get(), 0);
    assert!(h.ledger_kinds().is_empty());
}

/// A retry belongs to the file in flight, so a stop does not cut it off.
#[test]
fn a_stop_lets_the_current_files_retry_finish() {
    let h = Harness::new();
    h.source("A/01 - One.wav", None);
    h.source("A/02 - Two.wav", None);
    h.dev.set_faults(Faults {
        corrupt_uploads: 1,
        ..Default::default()
    });
    let stop = Stop::new();
    let pull = stop.clone();
    let report = h
        .run_entries(
            plan_of(&[h.music()]),
            one_retry(),
            &fake_encode,
            &stop,
            &mut |p| {
                if matches!(p, Progress::AttemptFailed { .. }) {
                    pull.request();
                }
            },
        )
        .unwrap();
    assert_eq!(verified(&report), ["pl00002-One.mp3"]);
    assert_eq!(stopped(&report), [1]);
    let totals = Ledger::read(&h.data(), SERIAL).unwrap().totals();
    assert_eq!((totals.reserved, totals.unresolved), (2, 0));
}

/// A mix goes in the order given, as one album of other artists.
#[test]
fn a_mix_is_sent_in_order_as_one_album() {
    let h = Harness::new();
    let c = h.source("Windrose/Wintersaga/03 - Mylir.flac", None);
    let a = h.source("Sea of Thieves/01 - Grogmire.wav", None);
    let b = h.source("Master and Commander/05 - Folly.flac", None);
    let seen = std::sync::Mutex::new(Vec::new());
    let record = |src: &Path, dst: &Path, tags: &Resolved| {
        seen.lock().unwrap().push(tags.clone());
        fake_encode(src, dst, tags)
    };
    let ov = Overrides {
        genre: Some("Running".into()),
        ..Overrides::default()
    };
    let report = h
        .run_mix(&[c, a, b], "Long Run", &ov, one_retry(), &record)
        .unwrap();

    assert_eq!(
        verified(&report),
        [
            "pl00001-Mylir.mp3",
            "pl00002-Grogmire.mp3",
            "pl00003-Folly.mp3"
        ]
    );
    let seen = seen.into_inner().unwrap();
    let got: Vec<_> = seen
        .iter()
        .map(|t| {
            (
                t.title.as_str(),
                t.artist.as_deref(),
                t.album.as_deref(),
                t.album_artist.as_deref(),
                t.track.as_deref(),
                t.genre.as_deref(),
                t.date.as_deref(),
            )
        })
        .collect();
    let mix = Some("Long Run");
    let va = Some("Various Artists");
    let g = Some("Running");
    assert_eq!(
        got,
        [
            ("Mylir", Some("Wintersaga"), mix, va, Some("1"), g, None),
            (
                "Grogmire",
                Some("Sea of Thieves"),
                mix,
                va,
                Some("2"),
                g,
                None
            ),
            (
                "Folly",
                Some("Master and Commander"),
                mix,
                va,
                Some("3"),
                g,
                None
            ),
        ]
    );
    // The ledger records the mix as the album, which is what the skip keys on.
    let l = Ledger::read(&h.data(), SERIAL).unwrap();
    assert!(l
        .events()
        .iter()
        .all(|e| e.album.as_deref() == Some("Long Run")));
}

/// "Already on the watch" means this audio in this album: a song sent with
/// its own album still goes inside a mix, and the other way round.
#[test]
fn the_skip_keys_on_the_audio_and_the_album() {
    let h = Harness::new();
    let one = h.source("A/01 - One.wav", None);
    let two = h.source("B/01 - Two.wav", None);

    // One on its album.
    assert_eq!(
        verified(&h.run(std::slice::from_ref(&one), one_retry()).unwrap()),
        ["pl00001-One.mp3"]
    );
    // In a mix it is a different library entry, so it is sent.
    let mix = |h: &Harness, opts| {
        h.run_mix(
            &[two.clone(), one.clone()],
            "Mix",
            &Overrides::default(),
            opts,
            &fake_encode,
        )
        .unwrap()
    };
    let r = mix(&h, one_retry());
    assert_eq!(verified(&r), ["pl00002-Two.mp3", "pl00003-One.mp3"]);
    // The same mix again: all there.
    let r = mix(&h, one_retry());
    assert_eq!(r.tally().skipped, 2);
    assert_eq!(
        r.files[1].outcome,
        Outcome::Skipped(Skip::AlreadyOnWatch {
            remote: "pl00003-One.mp3".into()
        })
    );
    // Two on its own album was never sent that way.
    let r = h.run(std::slice::from_ref(&two), one_retry()).unwrap();
    assert_eq!(verified(&r), ["pl00004-Two.mp3"]);
    // One on its album is still skipped, under its first name.
    let r = h.run(std::slice::from_ref(&one), one_retry()).unwrap();
    assert_eq!(
        r.files[0].outcome,
        Outcome::Skipped(Skip::AlreadyOnWatch {
            remote: "pl00001-One.mp3".into()
        })
    );
    // `--resend` is unchanged: sent again under new names.
    let r = mix(
        &h,
        Options {
            resend: true,
            ..one_retry()
        },
    );
    assert_eq!(verified(&r), ["pl00005-Two.mp3", "pl00006-One.mp3"]);
}

#[test]
fn a_mix_with_a_blank_name_is_refused_before_anything() {
    let h = Harness::new();
    let one = h.source("A/01 - One.wav", None);
    let sources = source::expand(&[one]).unwrap();
    let mix = Mix { name: "  ".into() };
    assert!(transfer::plan_with(sources, &Overrides::default(), Some(&mix)).is_err());
}
