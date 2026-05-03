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
    /// Mid-upload progress. Fields are read by the GUI worker (which builds
    /// its own equivalent payload) and ignored by headless `run_jobs_per_file`,
    /// so they show as dead in cargo. Suppress.
    #[allow(dead_code)]
    Progress {
        src: PathBuf,
        transferred: u64,
        total: u64,
    },
    Skipped {
        src: PathBuf,
        reason: String,
    },
    Done {
        src: PathBuf,
        bytes: u64,
    },
    Failed {
        src: PathBuf,
        error: String,
    },
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
        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            walk(&entry.path(), &next_dir, flatten, out)?;
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
        match backend.upload(
            &upload_path,
            &job.remote_dir,
            &upload_name,
            &mut on_progress,
        ) {
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
                        let _ = tx.send(Event::Done {
                            src: job.src.clone(),
                            bytes,
                        });
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let jobs = expand_inputs_with(&[tmp.path().to_path_buf()], "Music", true).unwrap();
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
        let jobs = expand_inputs_with(&[tmp.path().to_path_buf()], "Music", false).unwrap();
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
