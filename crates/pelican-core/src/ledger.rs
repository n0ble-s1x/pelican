//! The per-device ledger: every name Pelican has ever sent to a watch.
//!
//! `$XDG_DATA_HOME/pelican/ledger-<serial>.jsonl`, one JSON object per line,
//! append-only. It is never pruned and never rewritten, because the thing it
//! records never goes away either: a name once written to `/Music` is in the
//! watch's library until a factory reset, and a second write under it turns
//! both objects into stubs (libmtp #307). The device listing cannot be the
//! only record: a stub's name is unreadable, and an upload cut off before
//! its object info landed leaves nothing listable at all. The ledger is
//! what makes "never reuse a name" hold.
//!
//! Three rules follow from that, and each is load-bearing:
//!
//! - **Reserve before write.** A `reserve` line is appended and fsync'd
//!   before the upload starts. A crash mid-write still burns the name.
//! - **One writer.** A run holds an exclusive lock on the file for its whole
//!   length; a second run on the same watch fails fast rather than handing
//!   out the same counter. Read-only commands take a shared lock, so they
//!   never see a line half-appended.
//! - **Corruption is fatal.** A line that does not parse is an error naming
//!   the file and line. Starting empty would forget burned names, which is
//!   exactly the failure the ledger exists to prevent.
//!
//! A factory reset is the one thing that empties the watch's library, and
//! the ledger records it rather than forgetting: a `reset` line (appended
//! and fsync'd like any other, written only after `/Music` was read back
//! with no audio objects in it; see [`crate::reset`]) closes an *epoch*.
//! Names, verified sources and totals are answered from the current epoch
//! only; [`Ledger::max_counter`] still spans every epoch, so no counter is
//! ever handed out twice. Nothing before the reset line is rewritten.

use std::fs::{File, OpenOptions, TryLockError};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::mtp::fold_name;

/// The line format this build reads and writes.
pub const VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The name is burned; an upload under it is about to start.
    Reserve,
    /// The read-back hash matched the local transcode.
    Verified,
    /// The upload or its proof failed. The name stays burned.
    Failed,
    /// The watch was factory-reset and Pelican saw `/Music` empty: every
    /// event before this one describes a library that no longer exists.
    /// Written only by [`Ledger::reset`]; `counter` is the highest counter
    /// ever used, so the numbering keeps rising across it.
    Reset,
}

/// One line of the ledger. Field order is the on-disk order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    pub v: u32,
    /// RFC 3339, UTC.
    pub at: String,
    pub event: Kind,
    pub counter: u64,
    /// The name on the watch, without the folder.
    pub remote: String,
    /// Absolute source path. Informational: the hash is the identity.
    pub source: String,
    pub source_sha256: String,
    /// SHA-256 of the transcode that was (or is about to be) sent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub upload_sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl Event {
    /// A new event stamped now, with the optional fields empty.
    pub fn new(kind: Kind, counter: u64, remote: &str) -> Self {
        Self {
            v: VERSION,
            at: rfc3339(SystemTime::now()),
            event: kind,
            counter,
            remote: remote.to_string(),
            source: String::new(),
            source_sha256: String::new(),
            upload_sha256: None,
            bytes: None,
            title: String::new(),
            artist: None,
            album: None,
            reason: None,
        }
    }
}

/// Event counts, for `pelican status`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub reserved: usize,
    pub verified: usize,
    pub failed: usize,
    /// Reserved names with neither outcome: the run died mid-upload. The
    /// name is burned and whatever landed under it is unproven.
    pub unresolved: usize,
    /// Factory resets recorded before the current epoch. The other counts
    /// cover only the events since the last one.
    pub resets: usize,
}

pub struct Ledger {
    path: PathBuf,
    // Held for the lock, and for appending when opened for a run. `None`
    // only for a read of a ledger that does not exist yet.
    file: Option<File>,
    writable: bool,
    events: Vec<Event>,
}

impl Ledger {
    /// Where the ledger for `serial` lives under `dir`.
    ///
    /// The serial comes off the USB descriptor, which the device controls,
    /// and it becomes part of a path. Anything but ASCII letters, digits,
    /// `-` and `_` is refused rather than mapped, so two serials can never
    /// share a file and none can climb out of `dir`.
    pub fn path_in(dir: &Path, serial: &str) -> Result<PathBuf> {
        let ok = !serial.is_empty()
            && serial.len() <= 64
            && serial
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
        if !ok {
            bail!(
                "serial {:?} cannot be used as a file name (only letters, digits, - and _), \
                 so Pelican cannot keep a ledger for it",
                crate::garmin::strip_control(serial)
            );
        }
        Ok(dir.join(format!("ledger-{serial}.jsonl")))
    }

    /// Open the ledger for a run: created if missing, exclusively locked
    /// until dropped, every existing line parsed.
    pub fn open(dir: &Path, serial: &str) -> Result<Self> {
        let path = Self::path_in(dir, serial)?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        let existed = path.exists();
        let mut opts = OpenOptions::new();
        opts.read(true).append(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let file = opts
            .open(&path)
            .with_context(|| format!("opening {}", path.display()))?;
        match file.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => bail!(
                "another Pelican run is already using the ledger for watch {serial} ({}). \
                 Only one run per watch at a time. Wait for it to finish.",
                path.display()
            ),
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("locking {}", path.display()))
            }
        }
        if !existed {
            // A new file's directory entry is only durable once the
            // directory is synced. Without it a crash could lose the whole
            // ledger along with every reserve line in it.
            sync_dir(dir);
        }
        let events = parse(&path, &file)?;
        Ok(Self {
            path,
            file: Some(file),
            writable: true,
            events,
        })
    }

    /// Read the ledger without writing: a shared lock, so a run in
    /// progress is never read mid-line. A ledger that does not exist is
    /// empty: nothing has been sent to that watch from this machine.
    pub fn read(dir: &Path, serial: &str) -> Result<Self> {
        let path = Self::path_in(dir, serial)?;
        let file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == ErrorKind::NotFound => {
                return Ok(Self {
                    path,
                    file: None,
                    writable: false,
                    events: Vec::new(),
                })
            }
            Err(e) => return Err(e).with_context(|| format!("opening {}", path.display())),
        };
        match file.try_lock_shared() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => bail!(
                "a push to watch {serial} is running and holds its ledger ({}). \
                 Try again when it finishes.",
                path.display()
            ),
            Err(TryLockError::Error(e)) => {
                return Err(e).with_context(|| format!("locking {}", path.display()))
            }
        }
        let events = parse(&path, &file)?;
        Ok(Self {
            path,
            file: Some(file),
            writable: false,
            events,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn events(&self) -> &[Event] {
        &self.events
    }

    /// Append one line and fsync it. When this returns `Ok`, the line
    /// survives a crash or power loss.
    pub fn append(&mut self, e: Event) -> Result<()> {
        let file = match (&mut self.file, self.writable) {
            (Some(f), true) => f,
            _ => bail!(
                "internal: ledger {} was opened read-only",
                self.path.display()
            ),
        };
        let mut line = serde_json::to_string(&e).context("encoding a ledger line")?;
        line.push('\n');
        // One write of the whole line: with O_APPEND the kernel places it
        // at the end in one piece.
        file.write_all(line.as_bytes())
            .and_then(|()| file.sync_data())
            .with_context(|| format!("writing to the ledger {}", self.path.display()))?;
        self.events.push(e);
        Ok(())
    }

    /// Highest counter ever used, across every epoch, 0 for an empty
    /// ledger. A reset does not lower it: the numbering only rises.
    pub fn max_counter(&self) -> u64 {
        self.events.iter().map(|e| e.counter).max().unwrap_or(0)
    }

    /// The events since the last [`Kind::Reset`] (all of them if the
    /// watch was never reset here), without the reset line itself. This is
    /// what describes the watch as it is now.
    pub fn current(&self) -> &[Event] {
        match self.events.iter().rposition(|e| e.event == Kind::Reset) {
            Some(i) => &self.events[i + 1..],
            None => &self.events,
        }
    }

    /// The newest reset line, if the watch was ever reset from here.
    pub fn last_reset(&self) -> Option<&Event> {
        self.events.iter().rev().find(|e| e.event == Kind::Reset)
    }

    /// Record that the watch was factory-reset: append a `reset` line,
    /// fsync'd, after which no earlier name, verified source or total
    /// counts. `reason` says what was checked.
    ///
    /// The caller proves the watch is clean first; [`crate::reset::reset_ledger`]
    /// is the only caller that should exist, and it re-reads `/Music`.
    /// Returns `false`, writing nothing, when the current epoch is already
    /// empty: there is nothing to close.
    pub fn reset(&mut self, reason: &str) -> Result<bool> {
        if self.current().is_empty() {
            return Ok(false);
        }
        let mut e = Event::new(Kind::Reset, self.max_counter(), "");
        e.reason = Some(reason.to_string());
        self.append(e)?;
        Ok(true)
    }

    /// Every remote name in the current epoch, whatever its status, folded.
    pub fn names(&self) -> impl Iterator<Item = String> + '_ {
        self.current().iter().map(|e| fold_name(&e.remote))
    }

    pub fn has_name(&self, name: &str) -> bool {
        let want = fold_name(name);
        self.current().iter().any(|e| fold_name(&e.remote) == want)
    }

    /// The newest `verified` event in the current epoch for a source with
    /// this hash.
    pub fn verified(&self, source_sha256: &str) -> Option<&Event> {
        self.current()
            .iter()
            .rev()
            .find(|e| e.event == Kind::Verified && e.source_sha256 == source_sha256)
    }

    /// The newest `verified` event in the current epoch for this audio
    /// *in this album*.
    ///
    /// The same song on its own album and inside a mix are two different
    /// library entries on the watch, so "already there" has to mean the
    /// pair: a song sent with its album is still sent inside a mix, and
    /// the other way round.
    pub fn verified_in(&self, source_sha256: &str, album: Option<&str>) -> Option<&Event> {
        self.current().iter().rev().find(|e| {
            e.event == Kind::Verified
                && e.source_sha256 == source_sha256
                && e.album.as_deref() == album
        })
    }

    /// Names, folded, in the current epoch whose last event is `failed` or
    /// a bare `reserve`: writes that may have left an unreadable object on
    /// the watch.
    pub fn unproven_names(&self) -> impl Iterator<Item = String> {
        let mut last: std::collections::HashMap<String, Kind> = std::collections::HashMap::new();
        for e in self.current() {
            last.insert(fold_name(&e.remote), e.event);
        }
        last.into_iter()
            .filter(|(_, k)| *k != Kind::Verified)
            .map(|(n, _)| n)
    }

    /// Counts for the current epoch, plus how many resets came before it.
    pub fn totals(&self) -> Totals {
        let mut t = Totals {
            resets: self
                .events
                .iter()
                .filter(|e| e.event == Kind::Reset)
                .count(),
            ..Totals::default()
        };
        let mut open = std::collections::HashSet::new();
        for e in self.current() {
            match e.event {
                Kind::Reserve => {
                    t.reserved += 1;
                    open.insert(fold_name(&e.remote));
                }
                Kind::Verified => {
                    t.verified += 1;
                    open.remove(&fold_name(&e.remote));
                }
                Kind::Failed => {
                    t.failed += 1;
                    open.remove(&fold_name(&e.remote));
                }
                Kind::Reset => unreachable!("current() starts after the last reset"),
            }
        }
        t.unresolved = open.len();
        t
    }
}

fn parse(path: &Path, mut file: &File) -> Result<Vec<Event>> {
    let mut raw = Vec::new();
    file.read_to_end(&mut raw)
        .with_context(|| format!("reading {}", path.display()))?;
    let corrupt = |n: usize, why: String| {
        anyhow!(
            "the ledger {} is damaged at line {n}: {why}. Pelican will not guess past it: \
             the ledger is the only record of some names on the watch. Repair that line \
             by hand, then re-run.",
            path.display()
        )
    };
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines: Vec<&[u8]> = raw.split(|&b| b == b'\n').collect();
    // Every line is written with its newline, so a file that does not end
    // in one was cut off mid-append.
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    } else {
        return Err(corrupt(lines.len(), "the line is incomplete".into()));
    }
    let mut events = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        let e: Event = serde_json::from_slice(line).map_err(|err| corrupt(n, err.to_string()))?;
        if e.v != VERSION {
            return Err(corrupt(
                n,
                format!("format v{} (this Pelican reads v{VERSION})", e.v),
            ));
        }
        events.push(e);
    }
    Ok(events)
}

fn sync_dir(dir: &Path) {
    if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
        tracing::warn!(dir = %dir.display(), error = %e, "could not sync the ledger's directory");
    }
}

/// `YYYY-MM-DDTHH:MM:SSZ`. Hand-rolled to keep a date crate out of the
/// dependency tree for one format string.
pub fn rfc3339(t: SystemTime) -> String {
    let secs = t
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let (days, rem) = (secs / 86_400, secs % 86_400);
    let (y, m, d) = civil_from_days(days as i64);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

/// Days since 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// algorithm).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = yoe + era * 400 + i64::from(m <= 2);
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn event(kind: Kind, counter: u64, remote: &str) -> Event {
        let mut e = Event::new(kind, counter, remote);
        e.source = "/music/a.flac".into();
        e.source_sha256 = "aa".into();
        e.title = "A".into();
        e
    }

    #[test]
    fn round_trips_and_keeps_the_spec_field_order() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut l = Ledger::open(tmp.path(), "3456").unwrap();
            l.append(event(Kind::Reserve, 1, "pl00001-A.mp3")).unwrap();
            let mut v = event(Kind::Verified, 1, "pl00001-A.mp3");
            v.upload_sha256 = Some("bb".into());
            v.bytes = Some(3);
            l.append(v).unwrap();
        }
        let text = std::fs::read_to_string(tmp.path().join("ledger-3456.jsonl")).unwrap();
        let first = text.lines().next().unwrap();
        assert!(
            first.starts_with(r#"{"v":1,"at":""#),
            "field order drifted: {first}"
        );
        assert!(first.contains(r#""event":"reserve","counter":1,"remote":"pl00001-A.mp3""#));
        assert!(
            !first.contains("upload_sha256"),
            "absent optionals are omitted"
        );
        let l = Ledger::read(tmp.path(), "3456").unwrap();
        assert_eq!(l.events().len(), 2);
        assert_eq!(l.max_counter(), 1);
        assert!(l.verified("aa").is_some());
        assert!(l.has_name("PL00001-a.MP3"));
    }

    #[test]
    fn unparsable_line_is_a_hard_error_naming_file_and_line() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut l = Ledger::open(tmp.path(), "1").unwrap();
            l.append(event(Kind::Reserve, 1, "pl00001-A.mp3")).unwrap();
        }
        let p = tmp.path().join("ledger-1.jsonl");
        let mut text = std::fs::read_to_string(&p).unwrap();
        text.push_str("{not json\n");
        std::fs::write(&p, &text).unwrap();
        for r in [
            Ledger::open(tmp.path(), "1").err(),
            Ledger::read(tmp.path(), "1").err(),
        ] {
            let msg = format!("{:#}", r.expect("a damaged ledger must not open"));
            assert!(msg.contains("ledger-1.jsonl"), "{msg}");
            assert!(msg.contains("line 2"), "{msg}");
            assert!(!msg.to_lowercase().contains("delete"), "{msg}");
        }
        // Never rewritten: the damaged file is byte-for-byte as it was.
        assert_eq!(std::fs::read_to_string(&p).unwrap(), text);
    }

    #[test]
    fn a_cut_off_last_line_is_damage_too() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("ledger-1.jsonl");
        let line = serde_json::to_string(&event(Kind::Reserve, 1, "pl00001-A.mp3")).unwrap();
        std::fs::write(&p, line).unwrap();
        let msg = format!("{:#}", Ledger::open(tmp.path(), "1").err().unwrap());
        assert!(
            msg.contains("line 1") && msg.contains("incomplete"),
            "{msg}"
        );
    }

    #[test]
    fn an_unknown_version_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        let mut e = event(Kind::Reserve, 1, "pl00001-A.mp3");
        e.v = 2;
        let line = serde_json::to_string(&e).unwrap() + "\n";
        std::fs::write(tmp.path().join("ledger-1.jsonl"), line).unwrap();
        assert!(Ledger::open(tmp.path(), "1").is_err());
    }

    #[test]
    fn a_second_run_on_the_same_watch_fails_fast() {
        let tmp = tempfile::tempdir().unwrap();
        let _held = Ledger::open(tmp.path(), "77").unwrap();
        let msg = format!("{:#}", Ledger::open(tmp.path(), "77").err().unwrap());
        assert!(msg.contains("already using the ledger"), "{msg}");
        // Readers are kept out of a live run too, rather than seeing a
        // half-appended line.
        assert!(Ledger::read(tmp.path(), "77").is_err());
        // A different watch has its own ledger and its own lock.
        assert!(Ledger::open(tmp.path(), "78").is_ok());
    }

    #[test]
    fn readers_share() {
        let tmp = tempfile::tempdir().unwrap();
        drop(Ledger::open(tmp.path(), "5").unwrap());
        let _a = Ledger::read(tmp.path(), "5").unwrap();
        let _b = Ledger::read(tmp.path(), "5").unwrap();
        assert!(
            Ledger::open(tmp.path(), "5").is_err(),
            "a reader blocks a run"
        );
    }

    #[test]
    fn a_missing_ledger_reads_empty_and_is_not_created() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::read(tmp.path(), "9").unwrap();
        assert!(l.events().is_empty());
        assert!(!tmp.path().join("ledger-9.jsonl").exists());
        assert!(l.append(event(Kind::Reserve, 1, "x")).is_err());
    }

    #[test]
    fn serials_that_are_not_plain_names_are_refused() {
        let d = Path::new("/d");
        for bad in ["", "../x", "a/b", "a b", "ser\u{1b}ial", "é"] {
            assert!(Ledger::path_in(d, bad).is_err(), "{bad:?}");
        }
        assert_eq!(
            Ledger::path_in(d, "0000ABCD-12_x").unwrap(),
            Path::new("/d/ledger-0000ABCD-12_x.jsonl")
        );
    }

    #[test]
    fn totals_count_unresolved_reserves() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        l.append(event(Kind::Reserve, 1, "pl00001-A.mp3")).unwrap();
        l.append(event(Kind::Verified, 1, "pl00001-A.mp3")).unwrap();
        l.append(event(Kind::Reserve, 2, "pl00002-A.mp3")).unwrap();
        l.append(event(Kind::Failed, 2, "pl00002-A.mp3")).unwrap();
        l.append(event(Kind::Reserve, 3, "pl00003-A.mp3")).unwrap();
        assert_eq!(
            l.totals(),
            Totals {
                reserved: 3,
                verified: 1,
                failed: 1,
                unresolved: 1,
                resets: 0,
            }
        );
    }

    #[test]
    fn a_reset_closes_the_epoch_but_the_counter_keeps_rising() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut l = Ledger::open(tmp.path(), "1").unwrap();
            l.append(event(Kind::Reserve, 1, "pl00001-A.mp3")).unwrap();
            l.append(event(Kind::Verified, 1, "pl00001-A.mp3")).unwrap();
            l.append(event(Kind::Reserve, 2, "pl00002-A.mp3")).unwrap();
            assert!(l.reset("test").unwrap());
            // An empty epoch has nothing to close.
            assert!(!l.reset("again").unwrap());
            l.append(event(Kind::Reserve, 3, "pl00003-A.mp3")).unwrap();
        }
        // Survives a reopen: the line is on disk, not just in memory.
        let l = Ledger::read(tmp.path(), "1").unwrap();
        assert_eq!(l.events().len(), 5, "append-only: nothing was removed");
        let reset = l.last_reset().unwrap();
        assert_eq!(reset.counter, 2, "the reset carries the highest counter");
        assert_eq!(reset.remote, "");
        assert_eq!(l.max_counter(), 3);
        assert!(!l.has_name("pl00001-A.mp3"), "pre-reset names are free");
        assert!(l.has_name("pl00003-A.mp3"));
        assert!(
            l.verified("aa").is_none(),
            "pre-reset verified no longer skips"
        );
        assert!(l.verified_in("aa", None).is_none());
        assert_eq!(l.names().count(), 1);
        assert_eq!(l.unproven_names().collect::<Vec<_>>(), ["pl00003-a.mp3"]);
        assert_eq!(
            l.totals(),
            Totals {
                reserved: 1,
                verified: 0,
                failed: 0,
                unresolved: 1,
                resets: 1,
            }
        );
        let text = std::fs::read_to_string(tmp.path().join("ledger-1.jsonl")).unwrap();
        let line = text.lines().nth(3).unwrap();
        assert!(
            line.contains(r#""event":"reset","counter":2,"remote":"""#),
            "{line}"
        );
        assert!(line.contains(r#""reason":"test""#), "{line}");
    }

    #[test]
    fn a_ledger_without_a_reset_reads_as_before() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        assert!(!l.reset("nothing yet").unwrap());
        l.append(event(Kind::Verified, 4, "pl00004-A.mp3")).unwrap();
        assert!(l.last_reset().is_none());
        assert_eq!(l.current().len(), l.events().len());
        assert!(l.verified("aa").is_some());
    }

    #[test]
    fn rfc3339_known_instants() {
        let at = |s| rfc3339(SystemTime::UNIX_EPOCH + Duration::from_secs(s));
        assert_eq!(at(0), "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(at(1_790_424_245), "2026-09-26T12:04:05Z");
    }
}
