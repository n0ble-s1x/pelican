//! Reading a local folder into something the library view can render.
//!
//! Runs on its own thread per call. It touches no device and takes no lock,
//! so a scan and a sync can overlap harmlessly — and, importantly, browsing
//! and playback keep working with no watch attached at all.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use pelican_core::transcode::encoder::Encoder;
use pelican_core::transcode::{self, tags};

use crate::dto::{EventSink, TrackDto, UiEvent};

/// A library is a plausible 50,000 files; a single IPC message is not. The
/// cap keeps one `Scanned` event to a few megabytes of JSON, and `found`
/// tells the UI the truth about what it is not showing.
const MAX_TRACKS: usize = 2000;

/// The profile `encoder::encode` targets, for both pipelines. Used to
/// estimate what a converted file will weigh on the watch.
const ENCODE_KBPS: u64 = 192;

pub fn spawn(sink: Arc<EventSink>, root: PathBuf, encoders: Vec<Encoder>) {
    std::thread::Builder::new()
        .name("pelican-scan".into())
        .spawn(move || {
            let mut files = Vec::new();
            walk(&root, &mut files);
            // read_dir order is unspecified; a library that reshuffles itself
            // between scans is worse than one in an arbitrary but stable
            // order.
            files.sort();

            let found = files.len();
            let truncated = found > MAX_TRACKS;
            files.truncate(MAX_TRACKS);

            let tracks: Vec<TrackDto> = files.iter().map(|p| describe(p, &encoders)).collect();

            // ffmpeg's MP3 profile is the only one confirmed to *play* on
            // real hardware. afconvert works, is always present on macOS, and
            // its M4A output has transferred to the reference FR165 — but
            // arriving and being indexed by the watch's music app are
            // different subsystems (see `UiEvent::Scanned::encoder_verified`),
            // and presenting the two as equally trustworthy would be a claim
            // we have not earned.
            let best = encoders.first().copied();
            sink.send(UiEvent::Scanned {
                root: root.display().to_string(),
                tracks,
                found,
                truncated,
                encoder: best.map(|e| e.name().to_string()),
                encoder_verified: best == Some(Encoder::Ffmpeg),
            });
        })
        .expect("spawning the scan thread");
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        // `file_type` rather than `metadata`: it does not follow symlinks, so
        // a link pointing back up the tree cannot send this into a loop.
        let Ok(ft) = entry.file_type() else { continue };
        if ft.is_dir() {
            walk(&path, out);
        } else if ft.is_file() && transcode::is_audio(&path) {
            out.push(path);
        }
    }
}

fn describe(path: &Path, encoders: &[Encoder]) -> TrackDto {
    let source_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();

    let info = tags::read_fast(path).ok();
    // `read_fast` already parses all of these; `describe` used to throw the
    // track number away, which left album ordering falling back to filename
    // sort — wrong for any album whose files are not zero-padded.
    let track = info.as_ref().and_then(|i| {
        i.tags
            .track
            .as_deref()
            // Vorbis writes "3/12".
            .and_then(|s| s.split('/').next())
            .and_then(|s| s.trim().parse().ok())
    });
    let disc = info.as_ref().and_then(|i| i.tags.disc.clone());
    let date = info.as_ref().and_then(|i| i.tags.date.clone());
    let (title, artist, album, duration_secs, fmt) = match &info {
        Some(i) => (
            i.tags.title.clone().unwrap_or_else(|| stem.clone()),
            i.tags.artist.clone().unwrap_or_default(),
            i.tags.album.clone().unwrap_or_default(),
            i.duration_secs,
            format_label(&ext, i),
        ),
        // Unreadable is not the same as untagged, but from the watch's point
        // of view the consequence is identical: no title, no artist, nothing
        // in the library. So it renders the same way.
        None => (
            stem.clone(),
            String::new(),
            String::new(),
            0,
            ext.to_uppercase(),
        ),
    };
    let playable_in_library = info
        .as_ref()
        .map(|i| i.tags.playable_in_library())
        .unwrap_or(false);

    let (pipeline, sendable, note, bytes) = match plan(path, &ext, encoders) {
        Some(transcode::encoder::Pipeline::Passthrough) => (
            "passthrough",
            true,
            None,
            // Copied byte-for-byte; only the tag is rewritten.
            source_bytes,
        ),
        Some(transcode::encoder::Pipeline::Encode(e)) => {
            let out = e.output_ext().to_uppercase();
            let verified = if e == Encoder::Ffmpeg {
                ""
            } else {
                " This path has not been confirmed on a watch."
            };
            (
                e.name(),
                true,
                Some(format!(
                    "Converts to {out} at {ENCODE_KBPS} kbps on this Mac, using {}.{verified}",
                    e.name()
                )),
                estimate_encoded(duration_secs, source_bytes),
            )
        }
        None => (
            "unsupported",
            false,
            Some(refusal(&ext, encoders)),
            source_bytes,
        ),
    };

    TrackDto {
        path: path.display().to_string(),
        title,
        artist,
        album,
        track,
        disc,
        date,
        duration_secs,
        fmt,
        bytes,
        source_bytes,
        playable_in_library,
        pipeline: pipeline.to_string(),
        sendable,
        note,
    }
}

/// `encoder::plan` re-runs `Encoder::available()` for every file, and on a
/// machine without ffmpeg that means spawning `ffmpeg -version` — and failing
/// — once per track in the library. The decision itself is pure given the
/// encoder list, so the list is probed once at startup and the same rule is
/// applied here from public API (`ext_supported` + `can_decode`).
fn plan(src: &Path, ext: &str, encoders: &[Encoder]) -> Option<transcode::encoder::Pipeline> {
    if pelican_core::transfer::ext_supported(src) {
        return Some(transcode::encoder::Pipeline::Passthrough);
    }
    encoders
        .iter()
        .copied()
        .find(|e| e.can_decode(ext))
        .map(transcode::encoder::Pipeline::Encode)
}

fn refusal(ext: &str, encoders: &[Encoder]) -> String {
    if encoders.is_empty() {
        // What was observed is that spawning `ffmpeg` off this process's PATH
        // failed — and a Finder-launched .app does not inherit the PATH a
        // Homebrew ffmpeg lives on. Do not upgrade that to "not installed".
        format!(".{ext} cannot be converted: Pelican found no encoder it can run. Install ffmpeg.")
    } else {
        format!(
            ".{ext} cannot be converted: {} cannot read it. Install ffmpeg to handle every format Pelican accepts.",
            encoders
                .iter()
                .map(|e| e.name())
                .collect::<Vec<_>>()
                .join(" or ")
        )
    }
}

/// What the converted file will weigh, from duration at the target bitrate.
///
/// The exact size is not knowable until the encoder has written it, and the
/// number's job is to answer "will my selection fit" before anything moves.
/// Falls back to the source size when the duration could not be read, which
/// over-estimates for lossless input — the safe direction for a fit check.
fn estimate_encoded(duration_secs: u64, source_bytes: u64) -> u64 {
    if duration_secs == 0 {
        return source_bytes;
    }
    duration_secs * ENCODE_KBPS * 1000 / 8
}

/// The format column: what this file is, in the terms the design uses.
/// "FLAC 24/96" for lossless, "MP3 320" for lossy.
fn format_label(ext: &str, info: &tags::SourceInfo) -> String {
    let name = ext.to_uppercase();
    let lossless = matches!(ext, "flac" | "wav" | "aiff" | "aif" | "alac" | "ape" | "wv");
    if lossless {
        if let (Some(depth), Some(rate)) = (info.bit_depth, info.sample_rate) {
            let khz = rate as f64 / 1000.0;
            let khz = if (khz - khz.round()).abs() < 0.05 {
                format!("{}", khz.round() as u32)
            } else {
                format!("{khz:.1}")
            };
            return format!("{name} {depth}/{khz}");
        }
    }
    match info.bitrate_kbps {
        Some(k) if k > 0 => format!("{name} {k}"),
        _ => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encoded_estimate_is_duration_times_bitrate() {
        // Five minutes at 192 kbps.
        assert_eq!(estimate_encoded(300, 60_000_000), 300 * 192_000 / 8);
    }

    #[test]
    fn unknown_duration_falls_back_to_the_source_size() {
        // Over-estimating is the safe direction for a "will it fit" check.
        assert_eq!(estimate_encoded(0, 4_321), 4_321);
    }

    #[test]
    fn lossless_label_reads_as_depth_over_khz() {
        let info = tags::SourceInfo {
            tags: Default::default(),
            duration_secs: 0,
            sample_rate: Some(96_000),
            bit_depth: Some(24),
            channels: Some(2),
            bitrate_kbps: Some(4608),
        };
        assert_eq!(format_label("flac", &info), "FLAC 24/96");
    }

    #[test]
    fn lossy_label_reads_as_bitrate() {
        let info = tags::SourceInfo {
            tags: Default::default(),
            duration_secs: 0,
            sample_rate: Some(44_100),
            bit_depth: None,
            channels: Some(2),
            bitrate_kbps: Some(320),
        };
        assert_eq!(format_label("mp3", &info), "MP3 320");
    }

    #[test]
    fn label_degrades_to_the_container_when_nothing_is_known() {
        let info = tags::SourceInfo {
            tags: Default::default(),
            duration_secs: 0,
            sample_rate: None,
            bit_depth: None,
            channels: None,
            bitrate_kbps: None,
        };
        assert_eq!(format_label("m4a", &info), "M4A");
    }

    #[test]
    fn native_formats_never_reach_an_encoder() {
        // Even with ffmpeg present, re-encoding an MP3 would be lossy for
        // nothing — the tag is rewritten in process instead.
        let p = Path::new("/x/song.mp3");
        assert!(matches!(
            plan(p, "mp3", &[Encoder::Ffmpeg]),
            Some(transcode::encoder::Pipeline::Passthrough)
        ));
    }

    #[test]
    fn a_format_no_installed_encoder_reads_is_refused_not_guessed() {
        // afconvert cannot read Ogg Vorbis; with only afconvert present the
        // row must be un-sendable rather than optimistically queued.
        assert!(plan(Path::new("/x/a.ogg"), "ogg", &[Encoder::AfConvert]).is_none());
        assert!(matches!(
            plan(Path::new("/x/a.ogg"), "ogg", &[Encoder::Ffmpeg]),
            Some(transcode::encoder::Pipeline::Encode(Encoder::Ffmpeg))
        ));
    }

    #[test]
    fn flac_falls_back_to_afconvert_when_ffmpeg_is_absent() {
        assert!(matches!(
            plan(Path::new("/x/a.flac"), "flac", &[Encoder::AfConvert]),
            Some(transcode::encoder::Pipeline::Encode(Encoder::AfConvert))
        ));
    }
}
