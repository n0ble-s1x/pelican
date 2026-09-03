//! Job queue: validate then upload.
//!
//! Each input path expands into one or more `Job`s. A worker drains the queue
//! and reports progress via a crossbeam channel. The GUI subscribes; the CLI
//! drains synchronously.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use crossbeam_channel::{Receiver, Sender};
use id3::TagLike;

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

/// Make `(remote_dir, stem)` unique across the plan.
///
/// Two things conspire to collide names: the walk flattens every source
/// subfolder into one remote folder, and `sanitize_filename_stem` truncates to
/// 56 chars. Two tracks agreeing on their first 56 sanitized characters — or
/// simply sharing a basename in different albums — produce identical remote
/// names, and MTP has no overwrite semantics, so the second upload either
/// clobbers the first or lands as a duplicate the user cannot tell apart.
///
/// Dedup is on the *stem*, not the full name, so `track.m4a` (which becomes
/// `track.mp3` after transcoding) still cannot collide with a real `track.mp3`.
fn dedupe_remote_names(jobs: &mut [Job], transcode: bool) {
    let mut seen: std::collections::HashSet<(String, String)> = std::collections::HashSet::new();
    for job in jobs.iter_mut() {
        // walk() emits a Job for every file it sees, but the workers skip some
        // of them at upload time. A file that never reaches the device must not
        // reserve a name, or the real track gets pushed to "-2" — an
        // `Album.cue` beside `Album.flac` is the common case.
        //
        // The predicate has to track the transcode setting: with transcoding on
        // anything is_audio() will be normalized and uploaded, but with it off
        // only Garmin-native containers survive. Using is_audio() in both modes
        // let `Album.flac` reserve the name under --no-transcode, shipping
        // `Album.mp3` as `Album-2.mp3`; drop the FLAC from the source later and
        // the next sync uploads `Album.mp3` too, leaving two copies on a device
        // that has no overwrite.
        let uploadable = if transcode {
            crate::transcode::is_audio(&job.src)
        } else {
            ext_supported(&job.src)
        };
        if !uploadable {
            continue;
        }
        let path = Path::new(&job.remote_name);
        let stem = path
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ext = path
            .extension()
            .map(|e| format!(".{}", e.to_string_lossy()))
            .unwrap_or_default();
        let dir_key = job.remote_dir.to_ascii_lowercase();
        // Key on the *sanitized* stem, because that is the name that will
        // actually be written: transcode::normalize re-runs
        // sanitize_filename_stem on whatever it is handed, and that collapses
        // repeated dashes and trims. Deduping the raw stem would let
        // "foo-" + "-2" -> "foo--2" -> "foo-2" collide with a real "foo-2".
        let mut candidate = crate::transcode::sanitize_filename_stem(&stem);
        let mut n = 1u32;
        while !seen.insert((dir_key.clone(), candidate.to_ascii_lowercase())) {
            n += 1;
            let suffix = format!("-{n}");
            // Keep the disambiguated name inside Garmin's 56-char stem budget.
            let budget = 56usize.saturating_sub(suffix.len());
            let base: String = stem.chars().take(budget).collect();
            candidate = crate::transcode::sanitize_filename_stem(&format!("{base}{suffix}"));
        }
        if candidate != stem {
            job.remote_name = format!("{candidate}{ext}");
        }
    }
}

fn walk(path: &Path, remote_dir: &str, flatten: bool, out: &mut Vec<Job>) -> Result<()> {
    let meta = std::fs::metadata(path).with_context(|| format!("stat {}", path.display()))?;
    if meta.is_file() {
        if let Some(name) = path.file_name() {
            out.push(Job {
                src: path.to_path_buf(),
                remote_dir: remote_dir.to_string(),
                remote_name: sanitize_name(&name.to_string_lossy()),
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
pub fn has_required_tags(p: &Path) -> bool {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("mp3") => match id3::Tag::read_from_path(p) {
            Ok(tag) => tag.title().is_some() && tag.artist().is_some(),
            Err(_) => false,
        },
        Some("m4a") | Some("m4b") | Some("aac") => match mp4ameta::Tag::read_from_path(p) {
            Ok(tag) => tag.title().is_some() && tag.artist().is_some(),
            Err(_) => false,
        },
        _ => true, // wav: tags optional in practice
    }
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
    let total = jobs.len();
    let mut report = Report::default();
    let _ = tx.send(Event::Planned { total });

    for job in jobs {
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

        let mut backend = match crate::mtp::open(device) {
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
                        let _ = tx.send(Event::Done { at, bytes });
                    }
                }
            }
            Err(e) => {
                report.failed += 1;
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
            },
            Job {
                src: PathBuf::from("/b/track.mp3"),
                remote_dir: "Music".into(),
                remote_name: "track.mp3".into(),
            },
            Job {
                src: PathBuf::from("/c/track.m4a"),
                remote_dir: "Music".into(),
                remote_name: "track.m4a".into(),
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
            },
            Job {
                src: PathBuf::from("/b/foo.mp3"),
                remote_dir: "Music".into(),
                remote_name: "foo.mp3".into(),
            },
            Job {
                src: PathBuf::from("/c/foo-2.mp3"),
                remote_dir: "Music".into(),
                remote_name: "foo-2.mp3".into(),
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
            },
            Job {
                src: PathBuf::from("/a/Album.flac"),
                remote_dir: "Music".into(),
                remote_name: "Album.flac".into(),
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
                },
                Job {
                    src: PathBuf::from("/a/Album.mp3"),
                    remote_dir: "Music".into(),
                    remote_name: "Album.mp3".into(),
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
            },
            Job {
                src: PathBuf::from("/b/track.mp3"),
                remote_dir: "Music/Other".into(),
                remote_name: "track.mp3".into(),
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
