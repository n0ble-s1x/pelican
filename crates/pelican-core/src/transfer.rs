//! A run: plan, transcode everything, then one session that writes and
//! proves each file.
//!
//! The order is the design:
//!
//! 1. **Plan** ([`plan`]) — resolve every file's tags. Reads only; this is
//!    all `--dry-run` does.
//! 2. **Hash and skip** — a source whose hash the ledger has as `verified`
//!    is already on the watch (R5), unless `--resend`.
//! 3. **Transcode all** into this run's staging dir (R8). The device is
//!    not open yet, and afterwards the exact bytes to send are on disk.
//! 4. **One session** (R6): list `/Music` once, refuse the run if it cannot
//!    fit (R7), then per file: reserve a fresh name in the ledger (fsync'd),
//!    upload, read the object back, compare SHA-256. A mismatch or error is
//!    recorded as `failed` and retried under a *new* name; the old one stays
//!    burned.
//!
//! Nothing here deletes, and nothing writes a name twice. A failed write
//! leaves an object on the watch that Pelican cannot remove — the music
//! library keeps it until a factory reset whatever anyone does — so the
//! honest response is to record it and move on under a fresh name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::garmin::MUSIC_FOLDER;
use crate::hash;
use crate::ledger::{Event, Kind, Ledger};
use crate::mtp::Backend;
use crate::naming::Names;
use crate::source::Source;
use crate::staging::StagingDir;
use crate::transcode::tags::{Overrides, Resolved};

/// Headroom kept free on the watch beyond the planned bytes.
pub const FREE_MARGIN: u64 = 2 << 20;

/// Garmin's documented ceiling on audio files in the music library.
pub const MAX_OBJECTS: usize = 500;

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

/// How one file ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// Uploaded and read back byte-identical.
    Verified { remote: String, bytes: u64 },
    /// Not sent, and nothing is wrong.
    Skipped(Skip),
    /// Not proven on the watch. Every name tried is in `remotes` and burned.
    Failed {
        reason: String,
        remotes: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    /// The ledger has this audio as verified under `remote`.
    AlreadyOnWatch { remote: String },
    /// The same audio appears earlier in this run.
    DuplicateOf(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileReport {
    pub source: PathBuf,
    pub outcome: Outcome,
}

/// What a run did, one entry per planned file, in plan order.
#[derive(Debug, Default, Clone)]
pub struct Report {
    pub files: Vec<FileReport>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Tally {
    pub verified: usize,
    pub skipped: usize,
    pub failed: usize,
}

impl Report {
    pub fn tally(&self) -> Tally {
        let mut t = Tally::default();
        for f in &self.files {
            match f.outcome {
                Outcome::Verified { .. } => t.verified += 1,
                Outcome::Skipped(_) => t.skipped += 1,
                Outcome::Failed { .. } => t.failed += 1,
            }
        }
        t
    }
}

/// What the run is doing, as it happens — for the CLI to print.
#[derive(Debug)]
pub enum Progress<'a> {
    Transcoding {
        n: usize,
        of: usize,
        source: &'a Path,
    },
    /// Every file is transcoded; the session is about to open.
    Connecting { files: usize, bytes: u64 },
    /// One attempt failed. `retrying` says whether another name is next.
    AttemptFailed {
        source: &'a Path,
        remote: &'a str,
        reason: &'a str,
        retrying: bool,
    },
    /// A file is finished, whichever way.
    Done(&'a FileReport),
}

/// Transcodes `src` to `dst` writing `tags`. [`crate::transcode::encoder::encode`]
/// in production; tests substitute something that needs no ffmpeg.
pub type Encode<'a> = &'a dyn Fn(&Path, &Path, &Resolved) -> Result<()>;

/// Where a run keeps things and how it talks to the outside world.
pub struct Env<'a> {
    /// Cache dir the run's [`StagingDir`] is created under.
    pub staging_base: &'a Path,
    pub encode: Encode<'a>,
    pub progress: &'a mut dyn FnMut(Progress<'_>),
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
    progress: &'a mut dyn FnMut(Progress<'_>),
    reports: Vec<Option<FileReport>>,
}

impl Out<'_> {
    fn done(&mut self, idx: usize, source: &Path, outcome: Outcome) {
        let report = FileReport {
            source: source.to_path_buf(),
            outcome,
        };
        (self.progress)(Progress::Done(&report));
        self.reports[idx] = Some(report);
    }

    fn failed(&mut self, idx: usize, source: &Path, reason: String) {
        let outcome = Outcome::Failed {
            reason,
            remotes: Vec::new(),
        };
        self.done(idx, source, outcome);
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

    // Step 2: hash, refuse, skip. Hashing reads each source once in full;
    // it is the only identity that survives a rename or a re-rip to the
    // same bytes.
    let mut todo = Vec::new();
    let mut seen: HashMap<String, PathBuf> = HashMap::new();
    for (idx, e) in entries.into_iter().enumerate() {
        let path = e.source.path;
        let tags = match e.tags {
            Ok(t) => t,
            Err(reason) => {
                out.failed(idx, &path, reason);
                continue;
            }
        };
        let sha = match hash::file(&path) {
            Ok(s) => s,
            Err(err) => {
                out.failed(idx, &path, format!("{err:#}"));
                continue;
            }
        };
        // "Already on the watch" is the more useful thing to say about a
        // copy, so the ledger is asked first.
        let skip = match (opts.resend, ledger.verified(&sha), seen.get(&sha)) {
            (false, Some(v), _) => Some(Skip::AlreadyOnWatch {
                remote: v.remote.clone(),
            }),
            (_, _, Some(first)) => Some(Skip::DuplicateOf(first.clone())),
            _ => None,
        };
        seen.entry(sha.clone()).or_insert_with(|| path.clone());
        if let Some(skip) = skip {
            out.done(idx, &path, Outcome::Skipped(skip));
            continue;
        }
        todo.push((idx, path, sha, tags));
    }

    // Step 3: transcode everything before the device is touched. The
    // staging dir lives exactly as long as this function, whichever way
    // it returns.
    let staging = StagingDir::create_in(env.staging_base)?;
    let mut ready = Vec::new();
    let of = todo.len();
    for (n, (idx, source, source_sha256, tags)) in todo.into_iter().enumerate() {
        (out.progress)(Progress::Transcoding {
            n: n + 1,
            of,
            source: &source,
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

    // Step 4.
    if !ready.is_empty() {
        let bytes = ready.iter().map(|r| r.bytes).sum();
        (out.progress)(Progress::Connecting {
            files: ready.len(),
            bytes,
        });
        let mut dev = open()?;
        send_all(dev.as_mut(), ledger, ready, opts, &mut out)?;
    }
    drop(staging);

    let files = out
        .reports
        .into_iter()
        .map(|r| r.expect("every planned file gets exactly one outcome"))
        .collect();
    Ok(Report { files })
}

/// The session: one listing, the capacity check, then write-and-prove.
fn send_all(
    dev: &mut dyn Backend,
    ledger: &mut Ledger,
    ready: Vec<Ready>,
    opts: Options,
    out: &mut Out<'_>,
) -> Result<()> {
    let listing = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    let mut names = Names::new(&listing, ledger);
    // Every non-folder counts toward the 500, stubs included: they are
    // objects the library still holds, and "audio" cannot be told from a
    // name we cannot read.
    let mut objects = listing.iter().filter(|e| !e.is_folder).count();

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
    if names.stubs() > 0 {
        tracing::warn!(
            stubs = names.stubs(),
            "/{MUSIC_FOLDER} holds unreadable objects from earlier failed writes; \
             their names cannot be seen, so only the ledger guards them"
        );
    }
    dev.ensure_folder(MUSIC_FOLDER)?;

    for r in ready {
        let mut remotes = Vec::new();
        let mut attempt = 0u32;
        let outcome = loop {
            let (counter, remote) = names.allocate(&r.tags.title);
            // Burned before a byte moves: if we die inside the upload the
            // ledger still knows this name was used.
            ledger.append(event(Kind::Reserve, counter, &remote, &r, None))?;
            remotes.push(remote.clone());
            objects += 1;
            match send_one(dev, &r, &remote) {
                Ok(()) => {
                    ledger.append(event(Kind::Verified, counter, &remote, &r, None))?;
                    break Outcome::Verified {
                        remote,
                        bytes: r.bytes,
                    };
                }
                Err(reason) => {
                    let failed = event(Kind::Failed, counter, &remote, &r, Some(reason.clone()));
                    ledger.append(failed)?;
                    let mut final_reason = reason.clone();
                    let mut retrying = attempt < opts.retries;
                    if retrying {
                        if let Err(why) = room_for_retry(dev, &r, objects) {
                            retrying = false;
                            final_reason = format!("{reason}; not retried: {why}");
                        }
                    }
                    (out.progress)(Progress::AttemptFailed {
                        source: &r.source,
                        remote: &remote,
                        reason: &reason,
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
fn send_one(dev: &mut dyn Backend, r: &Ready, remote: &str) -> std::result::Result<(), String> {
    dev.upload(&r.staged, MUSIC_FOLDER, remote, &mut |_, _| {})
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

/// A retry is another write, so it gets the same capacity check the run
/// got — against the watch as it is now, after the failed attempt.
fn room_for_retry(dev: &mut dyn Backend, r: &Ready, objects: usize) -> Result<(), String> {
    let (free, _) = dev
        .free_space()
        .map_err(|e| format!("could not read free space: {e:#}"))?;
    if r.bytes.saturating_add(FREE_MARGIN) > free {
        return Err(format!("only {} free", mib(free)));
    }
    if objects >= MAX_OBJECTS {
        return Err(format!("the music library is at {MAX_OBJECTS} objects"));
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
