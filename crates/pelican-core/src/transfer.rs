//! A run: plan, transcode everything, then one session that writes and
//! proves each file.
//!
//! The order is the design:
//!
//! 1. **Plan** ([`plan`]) — resolve every file's tags. Reads only.
//! 2. **Hash and skip** ([`preview`]) — a source whose hash the ledger has
//!    as `verified` is already on the watch (R5), unless `--resend`. Steps 1
//!    and 2 are all `--dry-run` does, and they are the same code the run
//!    uses, so the plan it prints is the run that would happen.
//! 3. **Transcode all** into this run's staging dir (R8). The device is
//!    not open yet, and afterwards the exact bytes to send are on disk.
//! 4. **One session** (R6): list `/Music` once, refuse the run if it cannot
//!    fit (R7), then per file: reserve a fresh name in the ledger (fsync'd),
//!    upload, read the object back, compare SHA-256. A mismatch or error is
//!    recorded as `failed` and retried under a *new* name; the old one stays
//!    burned.
//!
//! A front-end calls [`push`] and hears about the run through
//! [`Env::progress`], one owned, serializable [`Progress`] at a time, so the
//! events can be forwarded across a thread or to a webview as they are. It
//! can ask the run to end early through [`Env::stop`]; the run looks only
//! between files, so no file is ever cut off mid-write.
//!
//! A [`Mix`] is a user-ordered set of songs sent as one album — see
//! [`plan_with`].
//!
//! Nothing here deletes, and nothing writes a name twice. A failed write
//! leaves an object on the watch that Pelican cannot remove — the music
//! library keeps it until a factory reset whatever anyone does — so the
//! honest response is to record it and move on under a fresh name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::garmin::MUSIC_FOLDER;
use crate::hash;
use crate::ledger::{Event, Kind, Ledger};
use crate::mtp::Backend;
use crate::naming::Names;
use crate::source::Source;
use crate::staging::StagingDir;
pub use crate::transcode::tags::Mix;
use crate::transcode::tags::{Overrides, Resolved};
use crate::watch::{audio_objects, MAX_OBJECTS};

/// Headroom kept free on the watch beyond the planned bytes.
pub const FREE_MARGIN: u64 = 2 << 20;

/// One source and what its tag will say — or why it cannot be sent.
#[derive(Debug, Clone)]
pub struct PlanEntry {
    pub source: Source,
    /// `Err` holds the refusal (an empty title), already worded for the
    /// user. Kept in the plan so the run reports it rather than dropping
    /// the file silently.
    pub tags: std::result::Result<Resolved, String>,
}

/// Resolve every source's tags. Reads the sources; writes nothing.
pub fn plan(sources: Vec<Source>, ov: &Overrides) -> Vec<PlanEntry> {
    sources
        .into_iter()
        .map(|source| {
            let tags = Resolved::for_file(&source.path, source.root.as_deref(), ov)
                .map_err(|e| format!("{e:#}"));
            PlanEntry { source, tags }
        })
        .collect()
}

/// [`plan`], or with a mix, every source as one song of that album in the
/// order given: track numbers 1..n, album artist "Various Artists", each
/// song's own title and artist, and year and genre only from `ov`.
///
/// Errs only when the mix itself cannot be sent (a blank name).
pub fn plan_with(
    sources: Vec<Source>,
    ov: &Overrides,
    mix: Option<&Mix>,
) -> Result<Vec<PlanEntry>> {
    let Some(mix) = mix else {
        return Ok(plan(sources, ov));
    };
    // Album and artist overrides make no sense for a mix: the album is its
    // name, and the artists are the songs' own.
    let own = Overrides::default();
    let mut out = Vec::with_capacity(sources.len());
    for (i, source) in sources.into_iter().enumerate() {
        let tags = match Resolved::for_file(&source.path, source.root.as_deref(), &own) {
            Ok(r) => Ok(r.into_mix(mix, i + 1, ov)?),
            Err(e) => Err(format!("{e:#}")),
        };
        out.push(PlanEntry { source, tags });
    }
    Ok(out)
}

/// Asks a run to end after the file in flight. Cheap to clone; every clone
/// is the same switch.
#[derive(Debug, Clone, Default)]
pub struct Stop(Arc<AtomicBool>);

impl Stop {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop before the next file. The file in flight — including its
    /// retries — finishes and is proven or failed as usual.
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Options {
    /// Send sources the ledger already has as verified, under a new name.
    pub resend: bool,
    /// Further attempts after a failed one, each under a fresh name.
    pub retries: u32,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            resend: false,
            retries: 1,
        }
    }
}

/// What the run will do with one planned file, decided before anything is
/// transcoded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Verdict {
    /// Transcode and send it.
    Send {
        source_sha256: String,
    },
    Skip(Skip),
    /// It fails without reaching the watch: an empty title, or a source
    /// that cannot be read.
    Refused {
        reason: String,
    },
}

/// Steps 1–2 of a run, for every entry in plan order: hash each source and
/// decide. Reads the sources and `ledger`; writes nothing.
///
/// `ledger` is `None` when there is none to ask (a dry run that has not
/// been told which watch), and then nothing is "already on the watch".
pub fn preview(entries: &[PlanEntry], ledger: Option<&Ledger>, resend: bool) -> Vec<Verdict> {
    // Hashing reads each source once in full; it is the only identity that
    // survives a rename or a re-rip to the same bytes.
    let mut seen: HashMap<String, &Path> = HashMap::new();
    entries
        .iter()
        .map(|e| {
            if let Err(reason) = &e.tags {
                return Verdict::Refused {
                    reason: reason.clone(),
                };
            }
            let path = e.source.path.as_path();
            let sha = match hash::file(path) {
                Ok(s) => s,
                Err(err) => {
                    return Verdict::Refused {
                        reason: format!("{err:#}"),
                    }
                }
            };
            // "Already on the watch" is the more useful thing to say about
            // a copy, so the ledger is asked first.
            // Keyed on the album too: the same song inside a mix is a
            // different entry in the watch's library from the one on its
            // own album.
            let album = e.tags.as_ref().ok().and_then(|t| t.album.as_deref());
            let on_watch = ledger
                .filter(|_| !resend)
                .and_then(|l| l.verified_in(&sha, album))
                .map(|v| v.remote.clone());
            let first = seen.get(&sha).map(|p| p.to_path_buf());
            seen.entry(sha.clone()).or_insert(path);
            match (on_watch, first) {
                (Some(remote), _) => Verdict::Skip(Skip::AlreadyOnWatch { remote }),
                (None, Some(first)) => Verdict::Skip(Skip::DuplicateOf(first)),
                (None, None) => Verdict::Send { source_sha256: sha },
            }
        })
        .collect()
}

/// How one file ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Outcome {
    /// Uploaded and read back byte-identical. `sha256` is the hash both
    /// sides agreed on.
    Verified {
        remote: String,
        bytes: u64,
        sha256: String,
    },
    /// Not sent, and nothing is wrong.
    Skipped(Skip),
    /// Not proven on the watch. Every name tried is in `remotes` and burned.
    Failed {
        reason: String,
        remotes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Skip {
    /// The ledger has this audio as verified under `remote`.
    AlreadyOnWatch { remote: String },
    /// The same audio appears earlier in this run.
    DuplicateOf(PathBuf),
    /// The run was stopped before this file was sent. Nothing of it
    /// reached the watch and no name was used for it.
    Stopped {},
}

impl Skip {
    /// Why, in one line.
    pub fn reason(&self) -> String {
        match self {
            Skip::AlreadyOnWatch { remote } => format!("already on watch as {remote}"),
            Skip::DuplicateOf(first) => format!("same audio as {}", first.display()),
            Skip::Stopped {} => "stopped before it was sent".into(),
        }
    }
}

impl Outcome {
    /// One line on why a file was not verified; `None` when it was.
    pub fn reason(&self) -> Option<String> {
        match self {
            Outcome::Verified { .. } => None,
            Outcome::Skipped(skip) => Some(skip.reason()),
            Outcome::Failed { reason, .. } => Some(reason.clone()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileReport {
    /// Position in the plan, from 0.
    pub index: usize,
    pub source: PathBuf,
    pub outcome: Outcome,
}

/// What a run did, one entry per planned file, in plan order.
#[derive(Debug, Default, Clone, Serialize)]
pub struct Report {
    pub files: Vec<FileReport>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub verified: usize,
    /// Includes files a stop left unsent.
    pub skipped: usize,
    pub failed: usize,
    /// A stop left at least one file unsent.
    pub stopped: bool,
}

impl Report {
    pub fn tally(&self) -> Tally {
        let mut t = Tally::default();
        for f in &self.files {
            match f.outcome {
                Outcome::Verified { .. } => t.verified += 1,
                Outcome::Skipped(Skip::Stopped {}) => {
                    t.skipped += 1;
                    t.stopped = true;
                }
                Outcome::Skipped(_) => t.skipped += 1,
                Outcome::Failed { .. } => t.failed += 1,
            }
        }
        t
    }
}

/// What the run is doing, as it happens.
///
/// Owned, so a front-end can send it to another thread or serialize it as
/// is. Every planned file ends in exactly one [`Progress::Done`], and a run
/// that returns `Ok` ends with one [`Progress::Finished`]; the rest are for
/// showing work in flight. `index` is always the file's position in the
/// plan, from 0, so a front-end can find its row.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// `n` of `of` counts only the files being transcoded.
    Transcoding {
        index: usize,
        n: usize,
        of: usize,
        source: PathBuf,
    },
    /// Every file is transcoded; the session is about to open.
    Connecting { files: usize, bytes: u64 },
    /// An attempt is starting — `attempt` counts from 1. `remote` is
    /// already burned in the ledger. `n` of `of` counts only the files
    /// being sent.
    Sending {
        index: usize,
        n: usize,
        of: usize,
        source: PathBuf,
        remote: String,
        attempt: u32,
    },
    /// Bytes of the current attempt's upload that have crossed the wire.
    /// Read-back follows; nothing is proven until [`Progress::Done`].
    Uploading {
        index: usize,
        remote: String,
        sent: u64,
        total: u64,
    },
    /// One attempt failed. `retrying` says whether another name is next.
    AttemptFailed {
        index: usize,
        source: PathBuf,
        remote: String,
        reason: String,
        retrying: bool,
    },
    /// A file is finished, whichever way.
    Done(FileReport),
    /// The run is over; every file has had its `Done`.
    Finished(Tally),
}

/// Transcodes `src` to `dst` writing `tags`. [`crate::transcode::encoder::encode`]
/// in production; tests substitute something that needs no ffmpeg.
pub type Encode<'a> = &'a dyn Fn(&Path, &Path, &Resolved) -> Result<()>;

/// Where a run keeps things and how it talks to the outside world.
pub struct Env<'a> {
    /// Cache dir the run's [`StagingDir`] is created under.
    pub staging_base: &'a Path,
    pub encode: Encode<'a>,
    /// `Send` because upload progress is reported from inside the
    /// backend's upload, which requires it.
    pub progress: &'a mut (dyn FnMut(Progress) + Send),
    /// Checked before each file is transcoded and before each is sent —
    /// never inside one. Files it leaves unsent end as
    /// [`Skip::Stopped`], and none of them has a ledger line.
    pub stop: Stop,
}

/// A file that made it to step 4.
struct Ready {
    idx: usize,
    source: PathBuf,
    source_sha256: String,
    tags: Resolved,
    staged: PathBuf,
    staged_sha256: String,
    bytes: u64,
}

/// Collects outcomes in plan order and tells the CLI as each one lands.
struct Out<'a> {
    progress: &'a mut (dyn FnMut(Progress) + Send),
    reports: Vec<Option<FileReport>>,
}

impl Out<'_> {
    fn done(&mut self, idx: usize, source: &Path, outcome: Outcome) {
        let report = FileReport {
            index: idx,
            source: source.to_path_buf(),
            outcome,
        };
        (self.progress)(Progress::Done(report.clone()));
        self.reports[idx] = Some(report);
    }

    fn failed(&mut self, idx: usize, source: &Path, reason: String) {
        let outcome = Outcome::Failed {
            reason,
            remotes: Vec::new(),
        };
        self.done(idx, source, outcome);
    }

    fn stopped(&mut self, idx: usize, source: &Path) {
        self.done(idx, source, Outcome::Skipped(Skip::Stopped {}));
    }
}

/// Run a push.
///
/// `ledger` is held open (and locked) by the caller for the whole run.
/// `open` is called at most once, after every transcode, and only if
/// something is left to send.
///
/// Returns `Err` only when the run as a whole cannot go on — the session
/// will not open, `/Music` cannot be listed, the watch cannot fit the plan,
/// the ledger cannot be written. Per-file trouble is in the [`Report`].
/// Either way the staging directory is gone when this returns.
pub fn push(
    entries: Vec<PlanEntry>,
    ledger: &mut Ledger,
    open: impl FnOnce() -> Result<Box<dyn Backend>>,
    opts: Options,
    env: Env<'_>,
) -> Result<Report> {
    let mut out = Out {
        progress: env.progress,
        reports: vec![None; entries.len()],
    };

    // Step 2: hash, refuse, skip.
    let verdicts = preview(&entries, Some(ledger), opts.resend);
    let mut todo = Vec::new();
    for (idx, (e, verdict)) in entries.into_iter().zip(verdicts).enumerate() {
        let path = e.source.path;
        match (verdict, e.tags) {
            (Verdict::Send { source_sha256 }, Ok(tags)) => {
                todo.push((idx, path, source_sha256, tags))
            }
            (Verdict::Skip(skip), _) => out.done(idx, &path, Outcome::Skipped(skip)),
            (Verdict::Refused { reason }, _) => out.failed(idx, &path, reason),
            (Verdict::Send { .. }, Err(_)) => unreachable!("preview refuses a file without tags"),
        }
    }

    // Step 3: transcode everything before the device is touched. The
    // staging dir lives exactly as long as this function, whichever way
    // it returns.
    let staging = StagingDir::create_in(env.staging_base)?;
    let mut ready = Vec::new();
    let of = todo.len();
    for (n, (idx, source, source_sha256, tags)) in todo.into_iter().enumerate() {
        if env.stop.is_requested() {
            out.stopped(idx, &source);
            continue;
        }
        (out.progress)(Progress::Transcoding {
            index: idx,
            n: n + 1,
            of,
            source: source.clone(),
        });
        let staged = staging.file(n);
        let done = (env.encode)(&source, &staged, &tags).and_then(|()| {
            let sha = hash::file(&staged)?;
            let bytes = std::fs::metadata(&staged)
                .with_context(|| format!("statting {}", staged.display()))?
                .len();
            Ok((sha, bytes))
        });
        match done {
            Ok((staged_sha256, bytes)) => ready.push(Ready {
                idx,
                source,
                source_sha256,
                tags,
                staged,
                staged_sha256,
                bytes,
            }),
            Err(err) => out.failed(idx, &source, format!("transcode failed: {err:#}")),
        }
    }

    // Step 4. A stop that came during the transcodes ends the run here,
    // without opening the watch.
    if env.stop.is_requested() {
        for r in ready.drain(..) {
            out.stopped(r.idx, &r.source);
        }
    }
    if !ready.is_empty() {
        let bytes = ready.iter().map(|r| r.bytes).sum();
        (out.progress)(Progress::Connecting {
            files: ready.len(),
            bytes,
        });
        let mut dev = open()?;
        send_all(dev.as_mut(), ledger, ready, opts, &env.stop, &mut out)?;
    }
    drop(staging);

    let files = out
        .reports
        .into_iter()
        .map(|r| r.expect("every planned file gets exactly one outcome"))
        .collect();
    let report = Report { files };
    (out.progress)(Progress::Finished(report.tally()));
    Ok(report)
}

/// The session: one listing, the capacity check, then write-and-prove.
fn send_all(
    dev: &mut dyn Backend,
    ledger: &mut Ledger,
    ready: Vec<Ready>,
    opts: Options,
    stop: &Stop,
    out: &mut Out<'_>,
) -> Result<()> {
    let listing = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    let mut names = Names::new(&listing, ledger)?;
    let mut objects = audio_objects(&listing);

    // R7, before the first write.
    let planned: u64 = ready.iter().map(|r| r.bytes).sum();
    let (free, _) = dev.free_space().context("reading free space")?;
    if planned.saturating_add(FREE_MARGIN) > free {
        bail!(
            "not enough room on the watch: {} files need {} plus a {} margin, and {} is free. \
             Nothing was sent. Push fewer files.",
            ready.len(),
            mib(planned),
            mib(FREE_MARGIN),
            mib(free)
        );
    }
    if objects + ready.len() > MAX_OBJECTS {
        bail!(
            "too many files for the watch's music library: /{MUSIC_FOLDER} holds {objects} \
             and this run adds {}, past Garmin's limit of {MAX_OBJECTS}. Nothing was sent. \
             Push fewer files.",
            ready.len()
        );
    }
    if names.unaccounted_stubs() > 0 {
        tracing::warn!(
            stubs = names.stubs(),
            unaccounted = names.unaccounted_stubs(),
            first_counter = names.next_counter(),
            "/{MUSIC_FOLDER} holds unreadable objects this machine's ledger has no record of \
             (another machine, or a lost ledger). Their names cannot be seen, so the counter \
             was moved past them; if that ledger still exists elsewhere, it is the only full \
             record of the names used on this watch"
        );
    } else if names.stubs() > 0 {
        tracing::warn!(
            stubs = names.stubs(),
            "/{MUSIC_FOLDER} holds unreadable objects from earlier failed writes; \
             their names cannot be seen, so the counter was moved past them"
        );
    }
    dev.ensure_folder(MUSIC_FOLDER)?;

    // What the files after this one still need. The up-front check set
    // their room aside; a retry may only use what is left over.
    let of = ready.len();
    let mut later_files = ready.len();
    let mut later_bytes = planned;
    let mut ready = ready.into_iter().enumerate();
    while let Some((n, r)) = ready.next() {
        // Between files only, and before the name is reserved, so a stop
        // never leaves a reserve line without its outcome.
        if stop.is_requested() {
            out.stopped(r.idx, &r.source);
            for (_, r) in ready.by_ref() {
                out.stopped(r.idx, &r.source);
            }
            break;
        }
        later_files -= 1;
        later_bytes -= r.bytes;
        let later = Later {
            files: later_files,
            bytes: later_bytes,
        };
        let mut remotes = Vec::new();
        let mut attempt = 0u32;
        let outcome = loop {
            let (counter, remote) = names.allocate(&r.tags.title)?;
            // Burned before a byte moves: if we die inside the upload the
            // ledger still knows this name was used.
            ledger.append(event(Kind::Reserve, counter, &remote, &r, None))?;
            remotes.push(remote.clone());
            objects += 1;
            (out.progress)(Progress::Sending {
                index: r.idx,
                n: n + 1,
                of,
                source: r.source.clone(),
                remote: remote.clone(),
                attempt: attempt + 1,
            });
            match send_one(dev, &r, &remote, out.progress) {
                Ok(()) => {
                    ledger.append(event(Kind::Verified, counter, &remote, &r, None))?;
                    break Outcome::Verified {
                        remote,
                        bytes: r.bytes,
                        sha256: r.staged_sha256.clone(),
                    };
                }
                Err(reason) => {
                    let failed = event(Kind::Failed, counter, &remote, &r, Some(reason.clone()));
                    ledger.append(failed)?;
                    let mut final_reason = reason.clone();
                    let mut retrying = attempt < opts.retries;
                    if retrying {
                        if let Err(why) = room_for_retry(dev, &r, objects, later) {
                            retrying = false;
                            final_reason = format!("{reason}; not retried: {why}");
                        }
                    }
                    (out.progress)(Progress::AttemptFailed {
                        index: r.idx,
                        source: r.source.clone(),
                        remote: remote.clone(),
                        reason: reason.clone(),
                        retrying,
                    });
                    if !retrying {
                        break Outcome::Failed {
                            reason: final_reason,
                            remotes,
                        };
                    }
                    attempt += 1;
                }
            }
        };
        out.done(r.idx, &r.source, outcome);
    }
    Ok(())
}

/// Upload one staged file under `remote` and prove it by read-back.
///
/// The error is the ledger's `reason`, so it states what happened and
/// nothing more.
fn send_one(
    dev: &mut dyn Backend,
    r: &Ready,
    remote: &str,
    progress: &mut (dyn FnMut(Progress) + Send),
) -> std::result::Result<(), String> {
    let mut on_bytes = |sent, total| {
        progress(Progress::Uploading {
            index: r.idx,
            remote: remote.to_string(),
            sent,
            total,
        })
    };
    dev.upload(&r.staged, MUSIC_FOLDER, remote, &mut on_bytes)
        .map_err(|e| format!("upload failed: {e:#}"))?;
    let back = dev
        .download_file(&format!("{MUSIC_FOLDER}/{remote}"))
        .map_err(|e| format!("read-back failed: {e:#}"))?;
    let got = hash::bytes(&back);
    if got != r.staged_sha256 {
        return Err(format!(
            "read-back mismatch: sent {} bytes (sha256 {}), read back {} bytes (sha256 {got})",
            r.bytes,
            r.staged_sha256,
            back.len()
        ));
    }
    Ok(())
}

/// The files still queued behind the current one.
#[derive(Debug, Clone, Copy)]
struct Later {
    files: usize,
    bytes: u64,
}

/// A retry is another write, so it gets the same capacity check the run
/// got — against the watch as it is now, after the failed attempt, and
/// with the room the queued files were promised still held back for them.
/// Otherwise a retry spends a later file's slot and the run ends past 500
/// having checked every write. `objects` already counts the failed attempt.
fn room_for_retry(
    dev: &mut dyn Backend,
    r: &Ready,
    objects: usize,
    later: Later,
) -> Result<(), String> {
    let (free, _) = dev
        .free_space()
        .map_err(|e| format!("could not read free space: {e:#}"))?;
    let need = r
        .bytes
        .saturating_add(later.bytes)
        .saturating_add(FREE_MARGIN);
    if need > free {
        return Err(format!(
            "only {} free, and the {} file(s) queued after this one need theirs",
            mib(free),
            later.files
        ));
    }
    if objects + 1 + later.files > MAX_OBJECTS {
        return Err(format!(
            "the music library would pass {MAX_OBJECTS} objects: it holds {objects} \
             and {} more are queued after this one",
            later.files
        ));
    }
    Ok(())
}

fn event(kind: Kind, counter: u64, remote: &str, r: &Ready, reason: Option<String>) -> Event {
    let mut e = Event::new(kind, counter, remote);
    e.source = r.source.to_string_lossy().into_owned();
    e.source_sha256 = r.source_sha256.clone();
    e.upload_sha256 = Some(r.staged_sha256.clone());
    e.bytes = Some(r.bytes);
    e.title = r.tags.title.clone();
    e.artist = r.tags.artist.clone();
    e.album = r.tags.album.clone();
    e.reason = reason;
    e
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / f64::from(1u32 << 20))
}
