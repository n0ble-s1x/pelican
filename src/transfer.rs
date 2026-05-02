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
use crate::mtp::Backend;

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
    Started(PathBuf),
    Progress { src: PathBuf, transferred: u64, total: u64 },
    Skipped { src: PathBuf, reason: String },
    Done { src: PathBuf, bytes: u64 },
    Failed { src: PathBuf, error: String },
}

#[derive(Default, Debug)]
pub struct Report {
    pub ok: usize,
    pub skipped: usize,
    pub failed: usize,
}

pub fn channel() -> (Sender<Event>, Receiver<Event>) {
    crossbeam_channel::unbounded()
}

const SUPPORTED_EXTS: &[&str] = &["mp3", "m4a", "m4b", "aac", "wav"];

pub fn expand_inputs(inputs: &[PathBuf]) -> Result<Vec<Job>> {
    expand_inputs_into(inputs, MUSIC_FOLDER)
}

/// Plan jobs that target a specific remote folder rather than the default
/// `Music/`. Folders are flattened — every audio file lands directly in
/// `remote_root`, regardless of source-side subfolder depth. Garmin firmware
/// is unreliable when listing newly-created subfolders inside Music/, and
/// the watch's library view is built from ID3 tags anyway, so a flat layout
/// is both more robust and what Garmin's docs recommend.
pub fn expand_inputs_into(inputs: &[PathBuf], remote_root: &str) -> Result<Vec<Job>> {
    expand_inputs_with(inputs, remote_root, true)
}

pub fn expand_inputs_with(
    inputs: &[PathBuf],
    remote_root: &str,
    flatten: bool,
) -> Result<Vec<Job>> {
    let mut jobs = Vec::new();
    for input in inputs {
        walk(input, remote_root, flatten, &mut jobs)?;
    }
    Ok(jobs)
}

fn walk(path: &Path, remote_dir: &str, flatten: bool, out: &mut Vec<Job>) -> Result<()> {
    let meta = std::fs::metadata(path)
        .with_context(|| format!("stat {}", path.display()))?;
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
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            walk(&entry.path(), &next_dir, flatten, out)?;
        }
    }
    Ok(())
}

/// Garmin firmware is picky about a handful of characters in filenames.
/// Strip the worst offenders; keep unicode letters/numbers/spaces/.-_.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect()
}

fn ext_supported(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

fn has_required_tags(p: &Path) -> bool {
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

/// Per-file session pattern: open a fresh MTP backend for each upload.
/// Garmin firmware on the FR165 silently rejects most uploads (leaving a
/// broken metadata stub) when many files are sent over a single session.
/// Closing+reopening between files makes the pipeline reliable.
pub fn run_jobs_per_file(
    device: &crate::garmin::Device,
    inputs: &[PathBuf],
    opts: &Options,
) -> Result<Report> {
    let jobs = expand_inputs(inputs)?;
    let (tx, rx) = channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            for job in jobs {
                let mut backend = match crate::mtp::open(device) {
                    Ok(b) => b,
                    Err(e) => {
                        let _ = tx.send(Event::Failed {
                            src: job.src.clone(),
                            error: format!("opening session: {e:#}"),
                        });
                        continue;
                    }
                };
                run_worker(&mut *backend, vec![job], opts, tx.clone());
                // backend dropped here — closes the MTP session
            }
            drop(tx);
        });
        let mut report = Report::default();
        for evt in rx {
            match evt {
                Event::Started(p) => tracing::info!(file=%p.display(), "uploading"),
                Event::Progress { .. } => {}
                Event::Done { src, bytes } => {
                    report.ok += 1;
                    tracing::info!(file=%src.display(), bytes, "ok");
                }
                Event::Skipped { src, reason } => {
                    report.skipped += 1;
                    tracing::warn!(file=%src.display(), %reason, "skipped");
                }
                Event::Failed { src, error } => {
                    report.failed += 1;
                    tracing::error!(file=%src.display(), %error, "failed");
                }
            }
        }
        Ok(report)
    })
}

pub fn run_jobs(backend: &mut dyn Backend, inputs: &[PathBuf], opts: &Options) -> Result<Report> {
    let (tx, rx) = channel();
    let jobs = expand_inputs(inputs)?;
    std::thread::scope(|s| {
        s.spawn(|| run_worker(backend, jobs, opts, tx));
        let mut report = Report::default();
        for evt in rx {
            match evt {
                Event::Started(p) => tracing::info!(file=%p.display(), "uploading"),
                Event::Progress { .. } => {} // not surfaced in headless mode
                Event::Done { src, bytes } => {
                    report.ok += 1;
                    tracing::info!(file=%src.display(), bytes, "ok");
                }
                Event::Skipped { src, reason } => {
                    report.skipped += 1;
                    tracing::warn!(file=%src.display(), %reason, "skipped");
                }
                Event::Failed { src, error } => {
                    report.failed += 1;
                    tracing::error!(file=%src.display(), %error, "failed");
                }
            }
        }
        Ok(report)
    })
}

/// Drain jobs into a Vec<Event>. Used by the GUI which collects results to
/// display in the log panel rather than streaming through a channel.
pub fn run_worker_into(
    backend: &mut dyn Backend,
    jobs: Vec<Job>,
    opts: &Options,
    out: &mut Vec<Event>,
) {
    let (tx, rx) = channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            run_worker(backend, jobs, opts, tx);
        });
        for evt in rx {
            out.push(evt);
        }
    });
}

fn run_worker(backend: &mut dyn Backend, jobs: Vec<Job>, opts: &Options, tx: Sender<Event>) {
    for job in jobs {
        let _ = tx.send(Event::Started(job.src.clone()));
        if !crate::transcode::is_audio(&job.src) {
            let _ = tx.send(Event::Skipped {
                src: job.src.clone(),
                reason: "not an audio file".into(),
            });
            continue;
        }
        // Always normalize — re-mux MP3s for tag-strip, transcode others.
        // Garmin's firmware rejects files with non-standard ID3 frames or
        // exotic audio profiles, so we always rebuild the file with a
        // strict allowlist on the way out.
        let mut transcoded_holder: Option<crate::transcode::Transcoded> = None;
        let (upload_path, upload_name) = if opts.transcode {
            match crate::transcode::normalize(&job.src) {
                Ok(t) => {
                    let p = t.path.clone();
                    let n = t.mp3_name.clone();
                    transcoded_holder = Some(t);
                    (p, n)
                }
                Err(e) => {
                    let _ = tx.send(Event::Failed {
                        src: job.src.clone(),
                        error: format!("normalize: {e:#}"),
                    });
                    continue;
                }
            }
        } else {
            // User opted out of normalization. Only proceed if the source
            // is already a Garmin-supported format; we can't change container.
            if !ext_supported(&job.src) {
                let _ = tx.send(Event::Skipped {
                    src: job.src.clone(),
                    reason: "needs transcode (--no-transcode is set)".into(),
                });
                continue;
            }
            (job.src.clone(), job.remote_name.clone())
        };
        if !has_required_tags(&upload_path) {
            if !opts.skip_tag_check {
                let _ = tx.send(Event::Skipped {
                    src: job.src.clone(),
                    reason: "missing ID3 title/artist (would be hidden on watch)".into(),
                });
                drop(transcoded_holder);
                continue;
            } else {
                tracing::warn!(
                    file = %job.src.display(),
                    "uploading without ID3 title/artist — file will be on the watch but hidden from the music app"
                );
            }
        }
        if let Err(e) = backend.ensure_folder(&job.remote_dir) {
            let _ = tx.send(Event::Failed {
                src: job.src.clone(),
                error: format!("ensure_folder: {e}"),
            });
            drop(transcoded_holder);
            continue;
        }
        let prog_tx = tx.clone();
        let prog_src = job.src.clone();
        let mut on_progress = move |transferred: u64, total: u64| {
            let _ = prog_tx.send(Event::Progress {
                src: prog_src.clone(),
                transferred,
                total,
            });
        };
        match backend.upload(&upload_path, &job.remote_dir, &upload_name, &mut on_progress) {
            Ok(bytes) => {
                // Soft verify: Garmin's GetObjectInfo errors on freshly-
                // written files until the watch's indexer settles, so a
                // missing-size or listing-error result is normal here, not
                // grounds for failure. We only fail on a confirmed mismatch.
                match backend.remote_size(&job.remote_dir, &upload_name) {
                    Ok(Some(actual)) if actual != bytes => {
                        let _ = tx.send(Event::Failed {
                            src: job.src.clone(),
                            error: format!(
                                "post-write size mismatch: expected {bytes}, watch reports {actual}"
                            ),
                        });
                    }
                    _ => {
                        let _ = tx.send(Event::Done { src: job.src.clone(), bytes });
                    }
                }
            }
            Err(e) => {
                let _ = tx.send(Event::Failed {
                    src: job.src.clone(),
                    error: format!("{e:#}"),
                });
            }
        }
        drop(transcoded_holder);
    }
    drop(tx);
}
