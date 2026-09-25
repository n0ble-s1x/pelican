//! Job queue: validate then upload.
//!
//! Each input path expands into one or more `Job`s. A worker drains the queue
//! and reports progress via a crossbeam channel. The GUI subscribes; the CLI
//! drains synchronously.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};

use crate::garmin::MUSIC_FOLDER;

#[derive(Debug, Clone)]
pub struct Options {
    pub skip_tag_check: bool,
    pub transcode: bool,
}

#[derive(Debug, Clone)]
pub struct Job {
    pub src: PathBuf,
    pub remote_dir: String,
    pub remote_name: String,
    /// True once [`resolve_conflicts`] has compared this name against a real
    /// listing of `remote_dir`.
    ///
    /// The write-time guard in [`run`] takes its own listing of the target
    /// directory — once per run, on the first file that targets it — and
    /// that listing can fail. What it does then depends on this flag: with
    /// plan-time evidence behind it, it proceeds on the older evidence; with
    /// nothing behind it, it refuses to write a name it cannot prove is
    /// free. `expand_inputs` leaves it false, because planning is pure and
    /// never touches the device.
    pub name_checked: bool,
}

/// A name the plan wanted that the watch is already using.
///
/// Reported rather than resolved silently: a collision the user is not told
/// about is either a track they think they sent and did not (`Skip`), or a
/// file on their watch under a name they never chose (`Rename`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Conflict {
    /// The local file that could not have the name it wanted.
    pub src: PathBuf,
    /// The remote stem the plan asked for.
    pub wanted: String,
    /// The name the watch reports, in the watch's own spelling.
    pub existing: String,
    /// Under [`OnConflict::Rename`], the name the job now targets. `None`
    /// when the job was dropped from the plan.
    pub renamed_to: Option<String>,
}

/// What to do when a planned name is already on the device.
///
/// `Skip` is the default on purpose. Re-sending an album you already sent is
/// the ordinary case, and renaming by default would fill a watch that holds
/// about 3.45 GiB with `-2` copies on every re-sync. Renaming is the user's
/// answer to a collision they were told about, not ours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnConflict {
    #[default]
    Skip,
    Rename,
}

#[derive(Debug, Clone)]
pub enum Event {
    /// The plan is known. Emitted once, before any file is touched, so a
    /// front-end can size its progress bar before the first byte moves.
    Planned {
        total: usize,
    },
    Started(Progress),
    /// Converting or copying into the cache dir, before the upload starts.
    /// The device is idle during this — it can take seconds for a big FLAC,
    /// and a UI that shows "uploading" here looks wedged.
    Staging(Progress),
    /// Mid-upload. `transferred`/`total_bytes` are this file; the counters on
    /// [`Progress`] are the whole job.
    Progress {
        at: Progress,
        transferred: u64,
        total_bytes: u64,
    },
    Skipped {
        at: Progress,
        reason: String,
    },
    Done {
        at: Progress,
        bytes: u64,
    },
    Failed {
        at: Progress,
        error: String,
    },
    /// The queue is drained. Always the last event.
    Finished(Report),
}

/// Where a job is in the run: which file, and how many are behind it.
#[derive(Debug, Clone)]
pub struct Progress {
    pub src: PathBuf,
    /// Files fully accounted for before this one — the numerator for
    /// "3 of 12". Not the index of `src` in the plan: a skip advances it too.
    pub done: usize,
    pub total: usize,
}

impl Progress {
    /// Filename alone, for a UI label that has no room for a path.
    pub fn label(&self) -> String {
        self.src
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.src.display().to_string())
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Report {
    pub ok: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn channel() -> (Sender<Event>, Receiver<Event>) {
    crossbeam_channel::unbounded()
}

/// Containers Garmin firmware plays without conversion.
pub const SUPPORTED_EXTS: &[&str] = &["mp3", "m4a", "m4b", "aac", "wav"];

pub fn expand_inputs(inputs: &[PathBuf], transcode: bool) -> Result<Vec<Job>> {
    expand_inputs_into(inputs, MUSIC_FOLDER, transcode)
}

/// Plan jobs that target a specific remote folder rather than the default
/// `Music/`. Folders are flattened — every audio file lands directly in
/// `remote_root`, regardless of source-side subfolder depth. Garmin firmware
/// is unreliable when listing newly-created subfolders inside Music/, and
/// the watch's library view is built from ID3 tags anyway, so a flat layout
/// is both more robust and what Garmin's docs recommend.
pub fn expand_inputs_into(
    inputs: &[PathBuf],
    remote_root: &str,
    transcode: bool,
) -> Result<Vec<Job>> {
    expand_inputs_with(inputs, remote_root, true, transcode)
}

pub fn expand_inputs_with(
    inputs: &[PathBuf],
    remote_root: &str,
    flatten: bool,
    transcode: bool,
) -> Result<Vec<Job>> {
    // Sort the inputs, not just each directory's entries. The GUI builds this
    // slice by iterating a HashSet, whose order is seeded per process — without
    // this, the same selection could hand the unsuffixed name to a different
    // file on each run, so `intro.mp3` and `intro-2.mp3` would swap tracks
    // between syncs and any playlist referencing them by name would follow.
    let mut inputs: Vec<&PathBuf> = inputs.iter().collect();
    inputs.sort();
    let mut jobs = Vec::new();
    for input in inputs {
        walk(input, remote_root, flatten, &mut jobs)?;
    }
    dedupe_remote_names(&mut jobs, transcode);
    Ok(jobs)
}

/// The comparison key for "is this the same file to the firmware?": the last
/// path segment, extension removed, lowercased. Nothing else.
///
/// Lowercased because `/Music` is FAT-derived and case-insensitive —
/// `Track.mp3` and `track.mp3` are one file to the watch and two strings to
/// us. Stem-only because the final extension is not known until
/// `encoder::plan` runs, and `encoder::available()` shells out to
/// `ffmpeg -version` uncached, so predicting it at plan time would cost a
/// process spawn per file. Stem-level over-reports across extensions (a
/// device-side `song.mp3` blocks a planned `song.wav`) and never
/// under-reports, which is the right direction when a miss is unrecoverable.
///
/// **Not** sanitized. `sanitize_filename_stem` is for names *we* are about to
/// write; running it over a name Garmin Express wrote would turn `Café` into
/// `Caf`, which would then falsely collide with a planned `Caf` that in fact
/// targets a free name.
///
/// `pelican-shell`'s `device::stem_key` computes the same key for the wall.
pub fn stem_key(name: &str) -> String {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    match leaf.rfind('.') {
        Some(i) if i > 0 => leaf[..i].to_lowercase(),
        _ => leaf.to_lowercase(),
    }
}

/// How far the `-2`, `-3`, … walk will go before giving up. Reaching this
/// means something pathological; refusing beats looping.
const MAX_SUFFIX: u32 = 999;

/// Result of trying to reserve a stem in `remote_dir`.
enum Claim {
    Free {
        stem: String,
        /// The watch's own spelling of a name we had to step around, when
        /// there was one. `None` for a plain within-plan disambiguation.
        device_hit: Option<String>,
    },
    /// The device already holds this name. Only returned under
    /// [`OnConflict::Skip`], which does not step around anything.
    Taken(String),
    /// No free name inside Garmin's 56-char stem budget.
    Exhausted,
}

/// The `-2`, `-3`, … suffixing walk, shared by the plan-only dedupe and the
/// device-aware resolver so the 56-char budget, the sanitize-before-compare
/// rule and the lowercased keys exist in exactly one place.
///
/// `seen` holds names this plan has already claimed; `device` maps
/// `(dir_key, stem_key)` to the watch's own spelling. Device names are never
/// inserted into `seen` — they are rejected by lookup, so `seen` stays a
/// record of what *this plan* took.
fn claim_stem(
    stem: &str,
    dir_key: &str,
    seen: &mut std::collections::HashSet<(String, String)>,
    device: &std::collections::HashMap<(String, String), String>,
    policy: OnConflict,
) -> Claim {
    // Key on the *sanitized* stem, because that is the name that will
    // actually be written: transcode::normalize re-runs
    // sanitize_filename_stem on whatever it is handed, and that collapses
    // repeated dashes and trims. Comparing the raw stem would let
    // "foo-" + "-2" -> "foo--2" -> "foo-2" collide with a real "foo-2".
    let mut candidate = crate::transcode::sanitize_filename_stem(stem);
    let mut device_hit: Option<String> = None;
    let mut n = 1u32;
    loop {
        let key = (dir_key.to_string(), candidate.to_ascii_lowercase());
        if let Some(name) = device.get(&key) {
            if policy == OnConflict::Skip {
                return Claim::Taken(name.clone());
            }
            if device_hit.is_none() {
                device_hit = Some(name.clone());
            }
        } else if seen.insert(key) {
            return Claim::Free {
                stem: candidate,
                device_hit,
            };
        }
        n += 1;
        if n > MAX_SUFFIX {
            return Claim::Exhausted;
        }
        let suffix = format!("-{n}");
        // Keep the disambiguated name inside Garmin's 56-char stem budget.
        let budget = 56usize.saturating_sub(suffix.len());
        let base: String = stem.chars().take(budget).collect();
        candidate = crate::transcode::sanitize_filename_stem(&format!("{base}{suffix}"));
    }
}

/// True when this job will actually reach the device, and so deserves to
/// reserve a name.
///
/// walk() emits a Job for every file it sees, but the workers skip some of
/// them at upload time. A file that never reaches the device must not reserve
/// a name, or the real track gets pushed to "-2" — an `Album.cue` beside
/// `Album.flac` is the common case.
///
/// The predicate has to track the transcode setting: with transcoding on
/// anything is_audio() will be normalized and uploaded, but with it off only
/// Garmin-native containers survive. Using is_audio() in both modes let
/// `Album.flac` reserve the name under --no-transcode, shipping `Album.mp3`
/// as `Album-2.mp3`; drop the FLAC from the source later and the next sync
/// uploads `Album.mp3` too, leaving two copies on a device that has no
/// overwrite.
fn will_upload(job: &Job, transcode: bool) -> bool {
    if transcode {
        crate::transcode::is_audio(&job.src)
    } else {
        ext_supported(&job.src)
    }
}

/// Split a planned remote name into `(stem, ".ext")`.
fn split_remote_name(remote_name: &str) -> (String, String) {
    let path = Path::new(remote_name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (stem, ext)
}

/// The sentence the engine uses for a name it refused to write over.
///
/// Names the file and the consequence, not the mechanism. `dto::classify_skip`
/// matches on it, so rewording it costs a label, never a message.
pub fn name_taken_reason(existing: &str) -> String {
    format!(
        "a file called {existing} is already on your watch — sending this one over it \
         would destroy both copies, so it was not sent"
    )
}

/// Make `(remote_dir, stem)` unique across the plan.
///
/// Two things conspire to collide names: the walk flattens every source
/// subfolder into one remote folder, and `sanitize_filename_stem` truncates to
/// 56 chars. Two tracks agreeing on their first 56 sanitized characters — or
/// simply sharing a basename in different albums — produce identical remote
/// names, and MTP has no overwrite semantics.
///
/// What a same-name write actually does is worse than a duplicate. Per
/// `docs/garmin-mtp.md` §7, observed on an FR165 running FW 2506: **both**
/// objects — the one already on the watch and the one being sent — become
/// unreadable stubs, and per §6 no `DeleteObject` against a stub has ever
/// succeeded. So a collision destroys a file the user already had and leaves
/// wreckage that cannot be removed. That is what the extra folder walk per
/// file in [`run`] is buying.
///
/// This function only makes the plan unique against *itself*. Uniqueness
/// against what is already on the watch is [`resolve_conflicts`], which needs
/// a device listing and so cannot live here.
///
/// Dedup is on the *stem*, not the full name, so `track.m4a` (which becomes
/// `track.mp3` after transcoding) still cannot collide with a real `track.mp3`.
fn dedupe_remote_names(jobs: &mut [Job], transcode: bool) {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let device = std::collections::HashMap::new();
    for job in jobs.iter_mut() {
        if !will_upload(job, transcode) {
            continue;
        }
        let (stem, ext) = split_remote_name(&job.remote_name);
        let dir_key = job.remote_dir.to_ascii_lowercase();
        // With an empty device map only `Free` and `Exhausted` are reachable,
        // and `Exhausted` means 999 files in one folder share a stem. Leaving
        // the name alone there is safe: the write-time guard in `run` still
        // refuses to send the second one.
        if let Claim::Free {
            stem: candidate, ..
        } = claim_stem(&stem, &dir_key, &mut seen, &device, OnConflict::Rename)
        {
            if candidate != stem {
                job.remote_name = format!("{candidate}{ext}");
            }
        }
    }
}

/// Compare a plan against what the watch is already holding.
///
/// This is the plan-time half of the collision guard. It is pure: `existing`
/// is `(remote_dir, remote_name)` exactly as the device reports them, taken
/// by the caller with **one** listing per transfer — the plan targets a
/// single folder today, so one walk covers it, and the cost does not grow
/// with plan size.
///
/// Under [`OnConflict::Skip`] a colliding job is removed from `jobs` and
/// reported; under [`OnConflict::Rename`] it is moved to a name free on both
/// sides and reported with `renamed_to` set. Either way the caller has to say
/// so — a silently skipped track is one the user believes they sent, and a
/// silently renamed one is a file on their watch under a name they never
/// chose.
///
/// Every surviving job comes back with `name_checked` set, which is what
/// tells the write-time guard in [`run`] that a failed listing there still
/// has evidence behind it.
pub fn resolve_conflicts(
    jobs: &mut Vec<Job>,
    existing: &[(String, String)],
    policy: OnConflict,
    transcode: bool,
) -> Vec<Conflict> {
    let mut device: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::with_capacity(existing.len());
    for (dir, name) in existing {
        device.insert((dir.to_ascii_lowercase(), stem_key(name)), name.clone());
    }
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    let mut conflicts = Vec::new();
    let mut keep: Vec<Job> = Vec::with_capacity(jobs.len());

    for mut job in jobs.drain(..) {
        if !will_upload(&job, transcode) {
            // Never going to be written, so it cannot collide with anything.
            // Reporting a `cover.jpg` as "already on your watch" would be a
            // false alarm about a file that was always going to be skipped.
            keep.push(job);
            continue;
        }
        let (stem, ext) = split_remote_name(&job.remote_name);
        let dir_key = job.remote_dir.to_ascii_lowercase();
        let wanted = crate::transcode::sanitize_filename_stem(&stem);
        match claim_stem(&stem, &dir_key, &mut seen, &device, policy) {
            Claim::Free {
                stem: candidate,
                device_hit,
            } => {
                if candidate != stem {
                    job.remote_name = format!("{candidate}{ext}");
                }
                if let Some(existing) = device_hit {
                    conflicts.push(Conflict {
                        src: job.src.clone(),
                        wanted: wanted.clone(),
                        existing,
                        renamed_to: Some(job.remote_name.clone()),
                    });
                }
                job.name_checked = true;
                keep.push(job);
            }
            Claim::Taken(existing) => conflicts.push(Conflict {
                src: job.src,
                wanted,
                existing,
                renamed_to: None,
            }),
            Claim::Exhausted => conflicts.push(Conflict {
                src: job.src,
                wanted: wanted.clone(),
                existing: wanted,
                renamed_to: None,
            }),
        }
    }
    *jobs = keep;
    conflicts
}

fn walk(path: &Path, remote_dir: &str, flatten: bool, out: &mut Vec<Job>) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.is_file() {
        if let Some(name) = path.file_name() {
            out.push(Job {
                src: path.to_path_buf(),
                remote_dir: remote_dir.to_string(),
                remote_name: sanitize_name(&name.to_string_lossy()),
                // Planning is pure — nothing here has seen the device.
                name_checked: false,
            });
        }
        return Ok(());
    }
    if meta.is_dir() {
        let next_dir = if flatten {
            remote_dir.to_string()
        } else {
            let dir_name = path
                .file_name()
                .map(|n| sanitize_name(&n.to_string_lossy()))
                .unwrap_or_default();
            if dir_name.is_empty() {
                remote_dir.to_string()
            } else {
                format!("{remote_dir}/{dir_name}")
            }
        };
        // read_dir order is unspecified. Sort so a given source tree always
        // produces the same plan — and therefore the same disambiguating
        // suffixes — across runs, machines and filesystems.
        let mut entries: Vec<PathBuf> = Vec::new();
        for entry in std::fs::read_dir(path)? {
            // Propagate, don't swallow: a directory entry we cannot read means
            // files would be silently missing from the sync.
            entries.push(entry?.path());
        }
        entries.sort();
        for entry in entries {
            walk(&entry, &next_dir, flatten, out)?;
        }
    }
    Ok(())
}

/// Garmin firmware is picky about both characters AND total length: writes
/// to `/Music` whose `remote_name` exceeds ~60 chars or contains exotic
/// punctuation are silently rejected (broken stub). The transcode path
/// applies `sanitize_filename_stem` already; this is the same treatment for
/// the `--no-transcode` path so direct uploads of MP3/M4A/AAC/WAV are safe.
fn sanitize_name(name: &str) -> String {
    let path = std::path::Path::new(name);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| name.to_string());
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_ascii_lowercase()));
    let cleaned = crate::transcode::sanitize_filename_stem(&stem);
    match ext {
        Some(e) => format!("{cleaned}{e}"),
        None => cleaned,
    }
}

/// True when the file is already in a container the watch plays.
pub fn ext_supported(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

/// True when the file carries the title+artist Garmin's music app needs
/// to show it. Files without them land on disk but stay invisible.
///
/// **One definition, delegated.** This used to reparse the file itself with
/// `id3` or `mp4ameta` chosen by extension, and return an unconditional
/// `true` for everything else — including `wav`, which is in
/// [`SUPPORTED_EXTS`] and so reaches the passthrough path. That gave the
/// crate two different answers to "is this file tagged?": the transfer gate
/// declared an untagged WAV to have its required tags while the scan, which
/// asks [`Tags::playable_in_library`], simultaneously warned the user it had
/// none. Nothing reconciled them. Now there is one rule and one parser, and
/// the WAV exemption below is an explicit, argued special case rather than a
/// silent `true` for every extension the match arm did not name.
///
/// Note the two callers still legitimately read different bytes: this is
/// called on the *staged* upload copy, while the scan judges the *source*.
/// That is the right order for a gate — what matters is what is about to be
/// written — but it means the two can disagree about one file, and that
/// disagreement is real rather than a bug.
pub fn has_required_tags(p: &Path) -> bool {
    // WAV is exempt, and the exemption is about the format rather than about
    // this file. There is no tag block Pelican writes into a WAV and no
    // observation in docs/garmin-mtp.md that the watch reads one, so refusing
    // a WAV for missing tags would block a transfer over a field that could
    // not have been supplied. It goes across; the UI's own notice is what
    // tells the owner it may not appear in the music app.
    if p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return true;
    }
    crate::transcode::tags::Tags::read(p)
        .map(|t| t.playable_in_library())
        .unwrap_or(false)
}

/// What the post-write size probe actually tells us.
///
/// `GetObjectInfo` on FR165 firmware 2506 returns 0 for an object the device
/// has written but not yet indexed, and errors outright for a few seconds
/// after that. Neither is evidence of a bad write. Only a non-zero size that
/// disagrees with what we streamed is a confirmed mismatch — and even that
/// means "the object is on the watch at the wrong size", never "nothing
/// happened".
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PostWrite {
    Landed,
    Mismatch { expected: u64, actual: u64 },
}

/// Read the probe. Split out of [`run`] so the rule can be tested without a
/// watch: the verdict is the thing the reported tally hangs off, and the
/// firmware race that produces `Some(0)` cannot be forced on demand.
pub(crate) fn post_write_verdict(expected: u64, probed: &Result<Option<u64>>) -> PostWrite {
    match probed {
        Ok(Some(actual)) if *actual != 0 && *actual != expected => PostWrite::Mismatch {
            expected,
            actual: *actual,
        },
        _ => PostWrite::Landed,
    }
}

/// An upload error, worded by *where in the transfer it landed*.
///
/// `upload` can fail after the data phase has fully drained — a cable pulled
/// during the PTP response phase is exactly that shape, and it reaches us as
/// `kIOReturnAborted`. The object may well be intact on the watch. Asserting
/// a bare failure there is a claim about the device we cannot support, so the
/// two cases get different sentences. `mtp::UploadPhase` is the typed context
/// that tells them apart; the engine's own message is always kept whole.
fn describe_upload_failure(e: &anyhow::Error) -> String {
    let verbatim = format!("{e:#}");
    match e.downcast_ref::<crate::mtp::UploadPhase>() {
        Some(p) if p.drained() => format!(
            "the bytes finished streaming but the watch did not confirm the write — \
             the file may be on your watch; check the list below ({verbatim})"
        ),
        _ => verbatim,
    }
}

/// Drain a plan onto the device, emitting an [`Event`] for every step.
///
/// **One MTP session per file.** Garmin firmware on the FR165 silently
/// rejects most uploads — leaving a broken metadata stub — when many files
/// are sent over a single session. Closing and reopening between files is
/// what makes the pipeline reliable, and it is why this opens the backend
/// itself instead of borrowing one from the caller.
///
/// This is the only implementation of the transfer loop. The CLI drains it
/// synchronously through [`run_to_report`]; the GUI drains it on a worker
/// thread and renders the events. They used to be separate copies of this
/// function that drifted apart.
pub fn run(device: &crate::garmin::Device, jobs: Vec<Job>, opts: &Options, tx: &Sender<Event>) {
    run_with(&mut || crate::mtp::open(device), jobs, opts, tx, &|| false);
}

/// [`run`], with a Stop button behind it.
///
/// `cancelled` is polled between files — never mid-file, because there is no
/// way to abort a PTP data phase that has already started without leaving a
/// half-written object the firmware will not delete. A run stopped here has
/// written whole files or none.
///
/// The whole plan goes in one call on purpose: the write-time guard reads
/// each directory once per *run*, so handing it the plan a file at a time
/// throws that away and pays for a folder walk per file.
pub fn run_cancellable(
    device: &crate::garmin::Device,
    jobs: Vec<Job>,
    opts: &Options,
    tx: &Sender<Event>,
    cancelled: &dyn Fn() -> bool,
) {
    run_with(&mut || crate::mtp::open(device), jobs, opts, tx, cancelled);
}

/// [`run`], with the backend opener injected.
///
/// Exists so the transfer loop — and in particular the write-time collision
/// guard below, which is the last place anything can decline to destroy a
/// file — can be tested without a watch on the desk. `run` is the two-line
/// wrapper that supplies the real opener.
pub(crate) fn run_with(
    open: &mut dyn FnMut() -> Result<Box<dyn crate::mtp::Backend>>,
    jobs: Vec<Job>,
    opts: &Options,
    tx: &Sender<Event>,
    cancelled: &dyn Fn() -> bool,
) {
    let total = jobs.len();
    let mut report = Report::default();
    let _ = tx.send(Event::Planned { total });

    // What each target directory is known to be using, keyed by lowercased
    // directory. The value maps a stem key to the name to *show* — the
    // watch's own spelling when it came from a listing, ours when this run
    // wrote it.
    //
    // Read once per directory, on the first file that targets it, inside
    // that file's session — and maintained from then on by the run itself.
    // See the guard below for why one read is enough and what it does not
    // cover.
    let mut known: std::collections::HashMap<String, std::collections::HashMap<String, String>> =
        std::collections::HashMap::new();

    // Names this run has written, or may have written, keyed by
    // `(dir, stem)`. Kept apart from `known` deliberately: a directory whose
    // listing failed has no `known` entry and must not gain a fabricated
    // empty one, and a file written seconds ago may not be in a listing yet —
    // Garmin's indexer lags the write. This is the run's own memory, and it
    // is the only writer a run can account for.
    let mut written: std::collections::HashMap<(String, String), String> =
        std::collections::HashMap::new();

    for job in jobs {
        if cancelled() {
            break;
        }
        // `done` counts outcomes, not iterations, so a skipped file still
        // advances "3 of 12" — otherwise the counter stalls on a folder full
        // of cover art and the run looks hung.
        let at = Progress {
            src: job.src.clone(),
            done: report.ok + report.skipped + report.failed,
            total,
        };
        let _ = tx.send(Event::Started(at.clone()));

        if !crate::transcode::is_audio(&job.src) {
            report.skipped += 1;
            let _ = tx.send(Event::Skipped {
                at,
                reason: "not an audio file".into(),
            });
            continue;
        }

        // Stage the file: convert it, or copy and re-tag it. Either way the
        // result carries only the tag allowlist Garmin accepts.
        let mut staged: Option<crate::transcode::Transcoded> = None;
        let (upload_path, upload_name) = if opts.transcode {
            let _ = tx.send(Event::Staging(at.clone()));
            let planned_stem = Path::new(&job.remote_name)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned());
            match crate::transcode::normalize(&job.src, planned_stem.as_deref()) {
                Ok(t) => {
                    let (p, n) = (t.path.clone(), t.remote_name.clone());
                    staged = Some(t);
                    (p, n)
                }
                Err(e) => {
                    report.failed += 1;
                    let _ = tx.send(Event::Failed {
                        at,
                        error: format!("{e:#}"),
                    });
                    continue;
                }
            }
        } else {
            // Normalization off: we can't change container, so only formats
            // the watch already plays can go.
            if !ext_supported(&job.src) {
                report.skipped += 1;
                let _ = tx.send(Event::Skipped {
                    at,
                    reason: "not playable as-is, and normalization is off".into(),
                });
                continue;
            }
            (job.src.clone(), job.remote_name.clone())
        };

        if !has_required_tags(&upload_path) {
            if !opts.skip_tag_check {
                report.skipped += 1;
                let _ = tx.send(Event::Skipped {
                    at,
                    reason: "missing title/artist — would be hidden on the watch".into(),
                });
                continue;
            }
            tracing::warn!(
                file = %job.src.display(),
                "uploading without title/artist — the file lands on the watch but stays hidden"
            );
        }

        let mut backend = match open() {
            Ok(b) => b,
            Err(e) => {
                report.failed += 1;
                let _ = tx.send(Event::Failed {
                    at,
                    error: format!("opening session: {e:#}"),
                });
                continue;
            }
        };
        if let Err(e) = backend.ensure_folder(&job.remote_dir) {
            report.failed += 1;
            let _ = tx.send(Event::Failed {
                at,
                error: format!("ensure_folder: {e:#}"),
            });
            continue;
        }

        // ── the write-time collision guard ──────────────────────────────
        //
        // The last place this application can decline to destroy a file. It
        // runs unconditionally, in an open session, against the FINAL name —
        // what `transcode::normalize` returned, not what the planner guessed.
        //
        // The rule: never write a name for which there is no evidence it is
        // free. A listing error with plan-time evidence behind it proceeds on
        // the older evidence; a listing error with nothing behind it refuses,
        // and says so.
        //
        // **Cost: one folder walk per directory per run, not per file.** The
        // first file targeting a directory reads it; every file after that is
        // answered from `known`, which the run updates itself. Today a plan
        // targets exactly one directory, so a 200-file sync pays for one
        // walk, and the guard's cost does not grow with the plan.
        //
        // What that buys, precisely: the listing is taken *after* planning,
        // staging and any user dithering in between, so it catches a plan
        // that went stale — and `written` catches a run colliding with
        // itself, which is the only other writer the run can account for.
        // What it does not cover is a second program writing to /Music while
        // this run is in flight. Re-reading per file would narrow that window
        // and not close it, and it is not a failure anyone has reported:
        // docs/garmin-mtp.md section 7 records the guard's scope in the same
        // terms.
        let dir_key = job.remote_dir.to_ascii_lowercase();
        let stem = stem_key(&upload_name);
        if let Some(existing) = written.get(&(dir_key.clone(), stem.clone())) {
            report.skipped += 1;
            let _ = tx.send(Event::Skipped {
                at,
                reason: name_taken_reason(existing),
            });
            continue;
        }
        if !known.contains_key(&dir_key) {
            match backend.list_dir(&job.remote_dir) {
                Ok(entries) => {
                    // Broken stubs are indexed too, though their synthesized
                    // "‹unreadable #N›" names can never equal a real one —
                    // see docs/garmin-mtp.md section 7 on why that blind spot
                    // exists and why the guard is described as "every name
                    // the watch can report" rather than "every name".
                    let taken = entries
                        .iter()
                        .filter(|e| !e.is_folder)
                        .map(|e| (stem_key(&e.name), e.name.clone()))
                        .collect();
                    known.insert(dir_key.clone(), taken);
                }
                Err(e) if job.name_checked => {
                    // Checked against a real listing when the plan was made
                    // and nothing has been written under this name since.
                    // Proceed on that evidence rather than fail a whole run
                    // on one flaky GetObjectHandles. Not cached: the next
                    // file gets a fresh attempt at reading the folder.
                    tracing::warn!(
                        dir = %job.remote_dir,
                        name = %upload_name,
                        error = %format!("{e:#}"),
                        "could not read the folder before writing; proceeding on the plan-time listing"
                    );
                }
                Err(e) => {
                    report.failed += 1;
                    let _ = tx.send(Event::Failed {
                        at,
                        error: format!(
                            "could not read {} on your watch, so there is no evidence the name \
                             {upload_name} is free — nothing was written, because writing over a \
                             name that is taken destroys both copies ({e:#})",
                            job.remote_dir
                        ),
                    });
                    continue;
                }
            }
        }
        if let Some(existing) = known.get(&dir_key).and_then(|d| d.get(&stem)) {
            report.skipped += 1;
            let _ = tx.send(Event::Skipped {
                at,
                reason: name_taken_reason(existing),
            });
            continue;
        }

        let prog_tx = tx.clone();
        let prog_at = at.clone();
        let mut on_progress = move |transferred: u64, total_bytes: u64| {
            let _ = prog_tx.send(Event::Progress {
                at: prog_at.clone(),
                transferred,
                total_bytes,
            });
        };

        match backend.upload(
            &upload_path,
            &job.remote_dir,
            &upload_name,
            &mut on_progress,
        ) {
            Ok(bytes) => {
                // Soft verify, while this file's session is still open.
                // Garmin's GetObjectInfo errors on freshly-written files until
                // the indexer settles, so a missing size or a listing error is
                // normal here — only a confirmed mismatch is a failure.
                let mut verdict =
                    post_write_verdict(bytes, &backend.remote_size(&job.remote_dir, &upload_name));
                // Before believing a mismatch, let the indexer settle and ask
                // once more. The firmware reports intermediate sizes for a
                // second or so after the data phase closes, and a single
                // disagreeing probe is not enough to call a landed file bad.
                if matches!(verdict, PostWrite::Mismatch { .. }) {
                    std::thread::sleep(std::time::Duration::from_millis(300));
                    let second = backend.remote_size(&job.remote_dir, &upload_name);
                    if matches!(post_write_verdict(bytes, &second), PostWrite::Landed) {
                        verdict = PostWrite::Landed;
                    }
                }
                match verdict {
                    PostWrite::Mismatch { expected, actual } => {
                        report.failed += 1;
                        let _ = tx.send(Event::Failed {
                            at,
                            error: format!(
                                "post-write size mismatch: the file is on your watch but at \
                                 {actual} bytes, not the {expected} we sent — it will probably \
                                 not play, and MTP has no overwrite, so delete it before retrying"
                            ),
                        });
                    }
                    PostWrite::Landed => {
                        report.ok += 1;
                        // Reserved only now, not before the write: a job that
                        // never reached the wire must not lock the name out
                        // of a retry later in the same run.
                        written.insert((dir_key, stem), upload_name.clone());
                        let _ = tx.send(Event::Done { at, bytes });
                    }
                }
            }
            Err(e) => {
                report.failed += 1;
                // A write whose bytes drained is a different case from one
                // that never started. The watch did not answer, so this may
                // be a file that is aboard — and the guard biases toward
                // "taken" everywhere else it is uncertain, so it does here
                // too. Reserving the name costs a retry within this run;
                // not reserving it risks the second write that turns both
                // objects into stubs. That trade is not close.
                if e.downcast_ref::<crate::mtp::UploadPhase>()
                    .is_some_and(|p| p.drained())
                {
                    written.insert((dir_key, stem), upload_name.clone());
                }
                let _ = tx.send(Event::Failed {
                    at,
                    error: describe_upload_failure(&e),
                });
            }
        }
        // Drops the staged temp file, then closes the MTP session.
        drop(staged);
    }

    let _ = tx.send(Event::Finished(report));
}

/// Plan `inputs`, run them, and log each event as it lands. Used by the CLI,
/// which has nothing to render and just wants the tally.
pub fn run_to_report(
    device: &crate::garmin::Device,
    inputs: &[PathBuf],
    opts: &Options,
) -> Result<Report> {
    let jobs = expand_inputs(inputs, opts.transcode)?;
    let (tx, rx) = channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            run(device, jobs, opts, &tx);
            drop(tx);
        });
        let mut report = Report::default();
        for evt in rx {
            match evt {
                Event::Planned { total } => tracing::info!(files = total, "planned"),
                Event::Started(at) => tracing::info!(file=%at.src.display(), "uploading"),
                Event::Staging(at) => tracing::info!(file=%at.src.display(), "normalizing"),
                Event::Progress { .. } => {}
                Event::Done { at, bytes } => {
                    tracing::info!(file=%at.src.display(), bytes, "ok")
                }
                Event::Skipped { at, reason } => {
                    tracing::warn!(file=%at.src.display(), %reason, "skipped")
                }
                Event::Failed { at, error } => {
                    tracing::error!(file=%at.src.display(), %error, "failed")
                }
                Event::Finished(r) => report = r,
            }
        }
        Ok(report)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// What the fake backend was asked to do. The write-time guard's whole
    /// job is to *not* call `upload`, so the assertions are about absence.
    #[derive(Default, Debug)]
    struct Calls {
        list_dir: usize,
        uploads: Vec<String>,
    }

    struct FakeBackend {
        entries: Vec<crate::mtp::RemoteEntry>,
        list_fails: bool,
        /// When set, every `upload` streams its bytes and then fails in the
        /// response phase — the drained-but-unconfirmed shape.
        drains_then_fails: bool,
        calls: Arc<Mutex<Calls>>,
    }

    impl crate::mtp::Backend for FakeBackend {
        fn ensure_folder(&mut self, _path: &str) -> Result<()> {
            Ok(())
        }
        fn upload(
            &mut self,
            local: &Path,
            _remote_dir: &str,
            remote_name: &str,
            on_progress: &mut (dyn FnMut(u64, u64) + Send),
        ) -> Result<u64> {
            let len = std::fs::metadata(local).map(|m| m.len()).unwrap_or(0);
            self.calls.lock().unwrap().uploads.push(remote_name.into());
            on_progress(len, len);
            if self.drains_then_fails {
                return Err(anyhow::anyhow!("kIOReturnAborted (0xe00002ed)").context(
                    crate::mtp::UploadPhase {
                        local: local.display().to_string(),
                        sent: len,
                        len,
                    },
                ));
            }
            Ok(len)
        }
        fn remote_size(&mut self, _remote_dir: &str, _remote_name: &str) -> Result<Option<u64>> {
            // The firmware's usual answer for a freshly-written file.
            Ok(None)
        }
        fn list_dir(&mut self, _path: &str) -> Result<Vec<crate::mtp::RemoteEntry>> {
            self.calls.lock().unwrap().list_dir += 1;
            if self.list_fails {
                anyhow::bail!("Protocol GeneralError");
            }
            Ok(self.entries.clone())
        }
        fn delete(&mut self, _path: &str) -> Result<()> {
            Ok(())
        }
        fn free_space(&mut self) -> Result<(u64, u64)> {
            Ok((1 << 30, 1 << 31))
        }
        fn download_file(&mut self, _path: &str) -> Result<Vec<u8>> {
            anyhow::bail!("not used")
        }
        fn write_raw(&mut self, _dir: &str, _name: &str, _bytes: &[u8]) -> Result<()> {
            Ok(())
        }
    }

    fn remote_entry(name: &str) -> crate::mtp::RemoteEntry {
        crate::mtp::RemoteEntry {
            name: name.into(),
            path: format!("Music/{name}"),
            size: 4,
            is_folder: false,
            is_broken: false,
        }
    }

    /// Run a plan against a fake watch. `transcode` is off so no encoder is
    /// spawned: the planned name is the written name.
    fn drive(
        jobs: Vec<Job>,
        entries: Vec<crate::mtp::RemoteEntry>,
        list_fails: bool,
    ) -> (Vec<Event>, Calls, Report) {
        drive_with(jobs, entries, list_fails, false, &|| false)
    }

    fn drive_with(
        jobs: Vec<Job>,
        entries: Vec<crate::mtp::RemoteEntry>,
        list_fails: bool,
        drains_then_fails: bool,
        cancelled: &dyn Fn() -> bool,
    ) -> (Vec<Event>, Calls, Report) {
        let calls = Arc::new(Mutex::new(Calls::default()));
        let opts = Options {
            skip_tag_check: true,
            transcode: false,
        };
        let (tx, rx) = channel();
        {
            let calls = calls.clone();
            let mut open = move || -> Result<Box<dyn crate::mtp::Backend>> {
                Ok(Box::new(FakeBackend {
                    entries: entries.clone(),
                    list_fails,
                    drains_then_fails,
                    calls: calls.clone(),
                }))
            };
            run_with(&mut open, jobs, &opts, &tx, cancelled);
        }
        drop(tx);
        let events: Vec<Event> = rx.into_iter().collect();
        let report = events
            .iter()
            .find_map(|e| match e {
                Event::Finished(r) => Some(*r),
                _ => None,
            })
            .expect("Finished is always last");
        (
            events,
            Arc::try_unwrap(calls).unwrap().into_inner().unwrap(),
            report,
        )
    }

    fn skips(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Skipped { reason, .. } => Some(reason.as_str()),
                _ => None,
            })
            .collect()
    }

    fn failures(events: &[Event]) -> Vec<&str> {
        events
            .iter()
            .filter_map(|e| match e {
                Event::Failed { error, .. } => Some(error.as_str()),
                _ => None,
            })
            .collect()
    }

    fn staged_job(dir: &tempfile::TempDir, name: &str) -> Job {
        let src = dir.path().join(name);
        std::fs::write(&src, b"data").unwrap();
        Job {
            src,
            remote_dir: "Music".into(),
            remote_name: name.into(),
            name_checked: false,
        }
    }

    /// The plan said the name was free; by the time this file's turn came,
    /// the watch disagreed. `SendObjectInfo` must never be reached.
    #[test]
    fn a_stale_plan_is_caught_before_send_object_info() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = staged_job(&dir, "track.mp3");
        job.name_checked = true; // the planner saw an empty folder
        let (events, calls, report) = drive(vec![job], vec![remote_entry("track.mp3")], false);

        assert!(
            calls.uploads.is_empty(),
            "wrote anyway: {:?}",
            calls.uploads
        );
        assert_eq!(skips(&events).len(), 1, "exactly one Skipped");
        assert!(skips(&events)[0].contains("already on your watch"));
        assert_eq!(report.skipped, 1);
        assert_eq!(report.ok, 0);
    }

    /// Case-insensitivity matters here too: `/Music` is FAT-derived, so
    /// `Track.mp3` on the watch is the same file as the `track.mp3` we are
    /// about to write.
    #[test]
    fn the_write_time_guard_compares_case_insensitively() {
        let dir = tempfile::tempdir().unwrap();
        let (events, calls, _) = drive(
            vec![staged_job(&dir, "track.mp3")],
            vec![remote_entry("TRACK.MP3")],
            false,
        );
        assert!(calls.uploads.is_empty());
        assert!(
            skips(&events)[0].contains("TRACK.MP3"),
            "the watch's spelling"
        );
    }

    /// No listing, and no plan-time listing behind it either: there is no
    /// evidence the name is free, so nothing is written and the run says so.
    #[test]
    fn an_unverifiable_listing_is_not_written_over() {
        let dir = tempfile::tempdir().unwrap();
        let (events, calls, report) = drive(vec![staged_job(&dir, "track.mp3")], vec![], true);

        assert!(calls.uploads.is_empty(), "wrote without evidence");
        assert_eq!(report.failed, 1);
        let f = failures(&events);
        assert_eq!(f.len(), 1);
        assert!(f[0].contains("no evidence"), "{}", f[0]);
        assert!(
            f[0].contains("Protocol GeneralError"),
            "the engine's own words must survive: {}",
            f[0]
        );
    }

    /// The same failed listing, but the planner did compare this name against
    /// a real listing. A flaky `GetObjectHandles` must not fail a whole run
    /// when there is older evidence behind it.
    #[test]
    fn a_failed_listing_proceeds_on_plan_time_evidence() {
        let dir = tempfile::tempdir().unwrap();
        let mut job = staged_job(&dir, "track.mp3");
        job.name_checked = true;
        let (_, calls, report) = drive(vec![job], vec![], true);

        assert_eq!(calls.uploads, vec!["track.mp3".to_string()]);
        assert_eq!(report.ok, 1);
    }

    /// The budget the guard was costed at: one folder walk per *directory*
    /// for the whole run. Three files into one directory read it once; the
    /// run answers the other two from what it already knows.
    #[test]
    fn one_listing_per_run_not_per_file() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = vec![
            staged_job(&dir, "a.mp3"),
            staged_job(&dir, "b.mp3"),
            staged_job(&dir, "c.mp3"),
        ];
        let (_, calls, report) = drive(jobs, vec![remote_entry("z.mp3")], false);
        assert_eq!(report.ok, 3);
        assert_eq!(
            calls.list_dir, 1,
            "one listing for the run, not one per file"
        );
    }

    /// The budget stated as the property that matters: device reads for the
    /// guard do not grow with the plan. Twenty files, one folder walk. The
    /// macOS shell hands `run_cancellable` the whole plan in one call for
    /// exactly this reason — a call per file would make the count twenty.
    #[test]
    fn guard_listings_do_not_grow_with_plan_size() {
        let dir = tempfile::tempdir().unwrap();
        let jobs: Vec<Job> = (0..20)
            .map(|i| staged_job(&dir, &format!("track-{i:02}.mp3")))
            .collect();
        let (_, calls, report) = drive(jobs, vec![remote_entry("unrelated.mp3")], false);
        assert_eq!(report.ok, 20);
        assert_eq!(calls.list_dir, 1);
    }

    /// The cached listing is not a licence to write over what it named. The
    /// second file in the run wants a name the *first* listing already
    /// reported, and is refused without re-reading the folder.
    #[test]
    fn a_cached_listing_still_refuses_a_taken_name() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = vec![staged_job(&dir, "a.mp3"), staged_job(&dir, "taken.mp3")];
        let (events, calls, report) = drive(jobs, vec![remote_entry("Taken.MP3")], false);
        assert_eq!(calls.uploads, vec!["a.mp3".to_string()]);
        assert_eq!(calls.list_dir, 1);
        assert_eq!(report.skipped, 1);
        assert!(
            skips(&events)[0].contains("Taken.MP3"),
            "the watch's spelling"
        );
    }

    /// A directory whose listing could not be read must not be remembered as
    /// read. The next file gets a fresh attempt rather than inheriting a
    /// fabricated empty folder.
    #[test]
    fn a_failed_listing_is_not_cached_as_an_empty_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut a = staged_job(&dir, "a.mp3");
        let mut b = staged_job(&dir, "b.mp3");
        a.name_checked = true;
        b.name_checked = true;
        let (_, calls, report) = drive(vec![a, b], vec![], true);
        assert_eq!(report.ok, 2, "both proceed on plan-time evidence");
        assert_eq!(calls.list_dir, 2, "the failure is retried, not cached");
    }

    /// The CLI and the egui app plan with no device listing at all, so a run
    /// has to be able to catch itself. The second `track.mp3` is refused by
    /// the names this run has already written.
    #[test]
    fn a_run_cannot_collide_with_itself() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let jobs = vec![staged_job(&one, "track.mp3"), staged_job(&two, "track.mp3")];
        // Fresh, empty watch: only the run's own memory can catch this.
        let (events, calls, report) = drive(jobs, vec![], false);

        assert_eq!(calls.uploads, vec!["track.mp3".to_string()]);
        assert_eq!(report.ok, 1);
        assert_eq!(report.skipped, 1);
        assert!(skips(&events)[0].contains("already on your watch"));
    }

    /// The bytes drained and the watch never answered. The file may be
    /// aboard under that name, so the run treats the name as taken — the
    /// same direction the guard biases in every other uncertain case. A
    /// second write under it is what turns both objects into stubs.
    #[test]
    fn a_drained_but_unconfirmed_write_reserves_the_name() {
        let one = tempfile::tempdir().unwrap();
        let two = tempfile::tempdir().unwrap();
        let jobs = vec![staged_job(&one, "track.mp3"), staged_job(&two, "track.mp3")];
        let (events, calls, report) = drive_with(jobs, vec![], false, true, &|| false);

        assert_eq!(
            calls.uploads,
            vec!["track.mp3".to_string()],
            "the second write must not be attempted"
        );
        assert_eq!(report.failed, 1);
        assert_eq!(report.skipped, 1);
        assert!(failures(&events)[0].contains("did not confirm the write"));
        assert!(skips(&events)[0].contains("already on your watch"));
    }

    /// Stop is honoured between files, never inside one: an aborted data
    /// phase leaves a half-written object the firmware will not delete.
    #[test]
    fn stop_takes_effect_between_files() {
        let dir = tempfile::tempdir().unwrap();
        let jobs = vec![
            staged_job(&dir, "a.mp3"),
            staged_job(&dir, "b.mp3"),
            staged_job(&dir, "c.mp3"),
        ];
        let seen = std::sync::atomic::AtomicUsize::new(0);
        let stop_after_one =
            || -> bool { seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) >= 1 };
        let (_, calls, report) = drive_with(jobs, vec![], false, false, &stop_after_one);
        assert_eq!(calls.uploads, vec!["a.mp3".to_string()]);
        assert_eq!(report.ok, 1, "the file in flight finished whole");
    }

    /// A file that failed to write must not lock its name out of a retry
    /// later in the same run.
    #[test]
    fn a_name_is_only_reserved_once_the_write_succeeded() {
        let dir = tempfile::tempdir().unwrap();
        // Not audio: skipped before the device is ever opened, so nothing is
        // reserved and the real track that follows keeps the name.
        let mut cue = staged_job(&dir, "Album.cue");
        cue.remote_name = "Album.cue".into();
        let mut track = staged_job(&dir, "Album.mp3");
        track.remote_name = "Album.mp3".into();
        let (_, calls, report) = drive(vec![cue, track], vec![], false);
        assert_eq!(calls.uploads, vec!["Album.mp3".to_string()]);
        assert_eq!(report.ok, 1);
    }

    /// The reported mechanism behind "Sent 0 tracks" with the music present.
    /// The watch answers `GetObjectInfo` with 0 for an object it has written
    /// but not yet indexed; treating that as a mismatch failed a file that
    /// had landed, and left it on the device with nothing offering to remove
    /// it.
    #[test]
    fn zero_size_after_write_is_not_a_failure() {
        assert_eq!(post_write_verdict(4096, &Ok(Some(0))), PostWrite::Landed);
    }

    #[test]
    fn missing_or_failed_probe_is_not_a_failure() {
        assert_eq!(post_write_verdict(4096, &Ok(None)), PostWrite::Landed);
        assert_eq!(
            post_write_verdict(4096, &Err(anyhow::anyhow!("busy"))),
            PostWrite::Landed
        );
    }

    #[test]
    fn confirmed_disagreement_is_a_mismatch() {
        assert_eq!(
            post_write_verdict(4096, &Ok(Some(2048))),
            PostWrite::Mismatch {
                expected: 4096,
                actual: 2048
            }
        );
    }

    #[test]
    fn agreement_is_landed() {
        assert_eq!(post_write_verdict(4096, &Ok(Some(4096))), PostWrite::Landed);
    }

    /// A cable pulled in the response phase drained the data phase first. The
    /// object may be on the watch, and the sentence has to leave room for it.
    #[test]
    fn a_drained_upload_says_the_file_may_be_aboard() {
        let e = anyhow::anyhow!("kIOReturnAborted (0xe00002ed)").context(crate::mtp::UploadPhase {
            local: "/x/t.m4a".into(),
            sent: 4096,
            len: 4096,
        });
        let msg = describe_upload_failure(&e);
        assert!(msg.contains("did not confirm"), "{msg}");
        assert!(
            msg.contains("kIOReturnAborted"),
            "the engine's own words must survive: {msg}"
        );
    }

    /// Cut mid-data, there is no such doubt: what is on the watch is not the
    /// file, and offering hope would be the lie the whole plan is about.
    #[test]
    fn an_upload_cut_mid_data_is_reported_verbatim() {
        let e = anyhow::anyhow!("kIOReturnAborted (0xe00002ed)").context(crate::mtp::UploadPhase {
            local: "/x/t.m4a".into(),
            sent: 1024,
            len: 4096,
        });
        let msg = describe_upload_failure(&e);
        assert!(!msg.contains("did not confirm"), "{msg}");
        assert!(msg.contains("streamed 1024 of 4096 bytes"), "{msg}");
    }

    #[test]
    fn dedupe_disambiguates_colliding_remote_stems() {
        let mut jobs = vec![
            Job {
                src: PathBuf::from("/a/track.mp3"),
                remote_dir: "Music".into(),
                remote_name: "track.mp3".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/b/track.mp3"),
                remote_dir: "Music".into(),
                remote_name: "track.mp3".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/c/track.m4a"),
                remote_dir: "Music".into(),
                remote_name: "track.m4a".into(),
                name_checked: false,
            },
        ];
        dedupe_remote_names(&mut jobs, true);
        let names: Vec<&str> = jobs.iter().map(|j| j.remote_name.as_str()).collect();
        assert_eq!(names, vec!["track.mp3", "track-2.mp3", "track-3.m4a"]);
    }

    #[test]
    fn dedupe_keeps_disambiguated_stem_within_budget() {
        let long = "x".repeat(56);
        let mut jobs: Vec<Job> = (0..3)
            .map(|i| Job {
                src: PathBuf::from(format!("/{i}/{long}.mp3")),
                remote_dir: "Music".into(),
                remote_name: format!("{long}.mp3"),
                name_checked: false,
            })
            .collect();
        dedupe_remote_names(&mut jobs, true);
        for j in &jobs {
            let stem_len = j
                .remote_name
                .rsplit_once('.')
                .map(|(s, _)| s.len())
                .unwrap();
            assert!(stem_len <= 56, "{} stem is {stem_len}", j.remote_name);
        }
        let unique: std::collections::HashSet<_> = jobs.iter().map(|j| &j.remote_name).collect();
        assert_eq!(unique.len(), 3, "names must stay distinct");
    }

    #[test]
    fn dedupe_survives_stem_sanitization() {
        // "foo-" sanitizes to "foo"; the disambiguated "foo--2" collapses to
        // "foo-2", which must not collide with a genuine "foo-2".
        let mut jobs = vec![
            Job {
                src: PathBuf::from("/a/foo-.mp3"),
                remote_dir: "Music".into(),
                remote_name: "foo-.mp3".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/b/foo.mp3"),
                remote_dir: "Music".into(),
                remote_name: "foo.mp3".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/c/foo-2.mp3"),
                remote_dir: "Music".into(),
                remote_name: "foo-2.mp3".into(),
                name_checked: false,
            },
        ];
        dedupe_remote_names(&mut jobs, true);
        let finals: Vec<String> = jobs
            .iter()
            .map(|j| {
                let stem = Path::new(&j.remote_name)
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                crate::transcode::sanitize_filename_stem(&stem)
            })
            .collect();
        let unique: std::collections::HashSet<&String> = finals.iter().collect();
        assert_eq!(
            unique.len(),
            finals.len(),
            "post-sanitization names collide: {finals:?}"
        );
    }

    #[test]
    fn dedupe_ignores_files_that_are_never_uploaded() {
        // Album.cue is skipped as non-audio at upload time, so it must not
        // reserve "Album" and push the real track to "Album-2".
        let mut jobs = vec![
            Job {
                src: PathBuf::from("/a/Album.cue"),
                remote_dir: "Music".into(),
                remote_name: "Album.cue".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/a/Album.flac"),
                remote_dir: "Music".into(),
                remote_name: "Album.flac".into(),
                name_checked: false,
            },
        ];
        dedupe_remote_names(&mut jobs, true);
        assert_eq!(jobs[1].remote_name, "Album.flac");
    }

    #[test]
    fn dedupe_predicate_follows_the_transcode_setting() {
        let build = || {
            vec![
                Job {
                    src: PathBuf::from("/a/Album.flac"),
                    remote_dir: "Music".into(),
                    remote_name: "Album.flac".into(),
                    name_checked: false,
                },
                Job {
                    src: PathBuf::from("/a/Album.mp3"),
                    remote_dir: "Music".into(),
                    remote_name: "Album.mp3".into(),
                    name_checked: false,
                },
            ]
        };
        // --no-transcode: the FLAC is skipped, so it must not hold the name.
        let mut off = build();
        dedupe_remote_names(&mut off, false);
        assert_eq!(off[1].remote_name, "Album.mp3");
        // Transcoding on: the FLAC really does upload, so the collision is real.
        let mut on = build();
        dedupe_remote_names(&mut on, true);
        assert_eq!(on[1].remote_name, "Album-2.mp3");
    }

    #[test]
    fn dedupe_is_scoped_per_remote_dir() {
        let mut jobs = vec![
            Job {
                src: PathBuf::from("/a/track.mp3"),
                remote_dir: "Music".into(),
                remote_name: "track.mp3".into(),
                name_checked: false,
            },
            Job {
                src: PathBuf::from("/b/track.mp3"),
                remote_dir: "Music/Other".into(),
                remote_name: "track.mp3".into(),
                name_checked: false,
            },
        ];
        dedupe_remote_names(&mut jobs, true);
        assert_eq!(
            jobs[1].remote_name, "track.mp3",
            "different dirs may share a name"
        );
    }

    #[test]
    fn sanitize_name_caps_long_filenames() {
        let raw = "11 - Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time - Tognetti Into the Fog.flac";
        let out = sanitize_name(raw);
        let stem_len = out
            .rsplit_once('.')
            .map(|(s, _)| s.len())
            .unwrap_or(out.len());
        assert!(
            stem_len <= 56,
            "stem must be ≤56 chars, got {stem_len}: {out}"
        );
        assert!(
            out.ends_with(".flac"),
            "extension preserved (lowercased): {out}"
        );
    }

    #[test]
    fn sanitize_name_strips_fat_hostile_chars() {
        let out = sanitize_name("a/b\\c:d*e?f\"g<h>i|j.mp3");
        assert!(!out.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']));
        assert!(out.ends_with(".mp3"));
    }

    #[test]
    fn sanitize_name_keeps_short_names() {
        assert_eq!(sanitize_name("track-01.mp3"), "track-01.mp3");
    }

    #[test]
    fn sanitize_name_lowercases_extension() {
        // Garmin firmware accepts mixed case, but normalizing avoids a
        // surprise "TRACK.MP3 vs track.mp3" duplicate-detection miss in
        // the watch's library indexer.
        assert!(sanitize_name("track.MP3").ends_with(".mp3"));
        assert!(sanitize_name("song.FLAC").ends_with(".flac"));
    }

    #[test]
    fn ext_supported_matches_garmin_formats() {
        for ok in ["x.mp3", "x.M4A", "x.m4b", "x.aac", "x.WAV"] {
            assert!(ext_supported(Path::new(ok)), "{ok} should be supported");
        }
        for ko in ["x.flac", "x.ogg", "x.opus", "x.wma", "x.txt", "x"] {
            assert!(
                !ext_supported(Path::new(ko)),
                "{ko} should NOT be supported"
            );
        }
    }

    #[test]
    fn expand_inputs_flattens_directory_tree() {
        let tmp = tempdir_with_layout(&[
            "album/01-track.mp3",
            "album/disc2/02-track.mp3",
            "album/cover.jpg",
        ]);
        let jobs = expand_inputs_with(&[tmp.path().to_path_buf()], "Music", true, true).unwrap();
        let names: std::collections::HashSet<&str> =
            jobs.iter().map(|j| j.remote_name.as_str()).collect();
        for j in &jobs {
            assert_eq!(j.remote_dir, "Music", "flatten should keep dir==Music");
        }
        assert!(names.contains("01-track.mp3"));
        assert!(names.contains("02-track.mp3"));
    }

    #[test]
    fn expand_inputs_preserves_subfolders_when_not_flat() {
        let tmp = tempdir_with_layout(&["album/01.mp3", "album/disc2/02.mp3"]);
        let jobs = expand_inputs_with(&[tmp.path().to_path_buf()], "Music", false, true).unwrap();
        let dirs: std::collections::HashSet<&str> =
            jobs.iter().map(|j| j.remote_dir.as_str()).collect();
        assert!(dirs.iter().any(|d| d.starts_with("Music/")));
        assert!(dirs.iter().any(|d| d.contains("disc2")));
    }

    fn tempdir_with_layout(paths: &[&str]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        for p in paths {
            let full = dir.path().join(p);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&full, b"x").unwrap();
        }
        dir
    }
}
