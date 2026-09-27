//! What a run would do, shaped for a front-end to show before anything is
//! sent: every file with its tags, its verdict and an estimate of its size
//! on the watch, the totals, and whether it all fits.
//!
//! The verdicts are [`transfer::preview`]'s — the same call the run makes —
//! so what is shown is what would happen. The sizes are **estimates**: the
//! exact size of a transcode is only known once ffmpeg has made it, and the
//! run's own capacity check uses the exact numbers.
//!
//! Reads the sources and the ledger. Touches no device and transcodes
//! nothing.

use std::path::{Path, PathBuf};
use std::time::Duration;

use lofty::config::ParseOptions;
use lofty::file::AudioFile;
use lofty::probe::Probe;
use serde::Serialize;

use crate::ledger::Ledger;
use crate::transfer::{self, PlanEntry, Verdict, FREE_MARGIN};
use crate::watch::MAX_OBJECTS;

/// The one output profile's bitrate, in bits per second.
pub const PROFILE_BPS: u64 = 192_000;

/// Allowance per file for the ID3v2.3 tag and the encoder's info frame.
pub const EST_OVERHEAD: u64 = 4 << 10;

/// Estimated size of `src` once transcoded: its duration at the profile's
/// bitrate, plus [`EST_OVERHEAD`]. When the duration cannot be read, a
/// guess from the source's size and format family — see [`estimate`].
pub fn estimate_bytes(src: &Path, source_bytes: u64) -> u64 {
    estimate(duration(src), source_bytes, src)
}

/// The pure half of [`estimate_bytes`].
///
/// Without a duration, uncompressed PCM shrinks to 192/1411 of its size,
/// lossless-compressed audio to about a quarter, and a lossy source is
/// assumed to grow by half (a 128 kbps file re-encoded at 192). Rough on
/// purpose — the run checks the real sizes before any write.
pub fn estimate(duration: Option<Duration>, source_bytes: u64, src: &Path) -> u64 {
    if let Some(d) = duration.filter(|d| !d.is_zero()) {
        let audio = (d.as_secs_f64() * PROFILE_BPS as f64 / 8.0).ceil() as u64;
        return audio + EST_OVERHEAD;
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let (num, den) = match ext.as_str() {
        "wav" | "aiff" | "aif" | "aifc" => (192, 1411),
        "flac" | "ape" | "wv" | "alac" => (1, 4),
        _ => (3, 2),
    };
    source_bytes.saturating_mul(num) / den + EST_OVERHEAD
}

/// The source's playing time, if its container says.
fn duration(src: &Path) -> Option<Duration> {
    let tagged = Probe::open(src)
        .ok()?
        .options(ParseOptions::new().read_tags(false).read_cover_art(false))
        .read()
        .ok()?;
    Some(tagged.properties().duration())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Decision {
    Send,
    Skip,
    Refused,
}

/// One planned file, as a front-end shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFile {
    pub source: PathBuf,
    /// The resolved title; for a refused file, its file name.
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub source_bytes: u64,
    /// An estimate; see the module docs.
    pub est_bytes: u64,
    pub verdict: Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Totals {
    pub send: u32,
    pub skip: u32,
    pub refused: u32,
    /// Estimated bytes of the files that would be sent.
    pub est_bytes: u64,
}

/// What the watch has room for, from a status read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Room {
    pub free_bytes: u64,
    /// Audio objects on `/Music` now ([`crate::watch::audio_objects`]).
    pub music_objects: usize,
}

/// Whether the files to send fit, by the run's own rule (free space less
/// [`FREE_MARGIN`], at most [`MAX_OBJECTS`]) applied to the estimates.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Fits {
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objects_after: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Preview {
    pub files: Vec<PreviewFile>,
    pub totals: Totals,
    pub fits: Fits,
}

/// Decide every entry and size it. `room` is `None` when no watch is
/// connected; the fit is then not established and says so.
pub fn build(
    entries: &[PlanEntry],
    ledger: Option<&Ledger>,
    resend: bool,
    room: Option<Room>,
) -> Preview {
    build_with(entries, ledger, &transfer::Resend::from(resend), room)
}

/// [`build`] with a per-file [`transfer::Resend`] — what a review's
/// per-track "send again" asks for.
pub fn build_with(
    entries: &[PlanEntry],
    ledger: Option<&Ledger>,
    resend: &transfer::Resend,
    room: Option<Room>,
) -> Preview {
    let verdicts = transfer::preview_with(entries, ledger, resend);
    let mut totals = Totals::default();
    let files: Vec<PreviewFile> = entries
        .iter()
        .zip(verdicts)
        .map(|(e, v)| {
            let path = &e.source.path;
            let source_bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
            let est_bytes = estimate_bytes(path, source_bytes);
            let (verdict, reason) = match v {
                Verdict::Send { .. } => {
                    totals.send += 1;
                    totals.est_bytes += est_bytes;
                    (Decision::Send, None)
                }
                Verdict::Skip(skip) => {
                    totals.skip += 1;
                    (Decision::Skip, Some(skip.reason()))
                }
                Verdict::Refused { reason } => {
                    totals.refused += 1;
                    (Decision::Refused, Some(reason))
                }
            };
            let mut f = PreviewFile {
                source: path.clone(),
                title: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                artist: None,
                album: None,
                track: None,
                year: None,
                genre: None,
                source_bytes,
                est_bytes,
                verdict,
                reason,
            };
            if let Ok(t) = &e.tags {
                f.title = t.title.clone();
                f.artist = t.artist.clone();
                f.album = t.album.clone();
                f.track = t.track.clone();
                f.year = t.date.clone();
                f.genre = t.genre.clone();
            }
            f
        })
        .collect();
    let fits = fits(&totals, room);
    Preview {
        files,
        totals,
        fits,
    }
}

/// [`Fits`] for `totals` against `room`.
pub fn fits(totals: &Totals, room: Option<Room>) -> Fits {
    let Some(room) = room else {
        return Fits {
            ok: false,
            free_bytes: None,
            objects_after: None,
            reason: Some("no watch connected, so space was not checked".into()),
        };
    };
    let objects_after = room.music_objects + totals.send as usize;
    let mut reason = None;
    if totals.est_bytes.saturating_add(FREE_MARGIN) > room.free_bytes {
        reason = Some(format!(
            "about {} needed with a {} margin, and {} is free",
            mib(totals.est_bytes),
            mib(FREE_MARGIN),
            mib(room.free_bytes)
        ));
    } else if objects_after > MAX_OBJECTS {
        reason = Some(format!(
            "the music library would hold {objects_after} files, past Garmin's limit of \
             {MAX_OBJECTS}"
        ));
    }
    Fits {
        ok: reason.is_none(),
        free_bytes: Some(room.free_bytes),
        objects_after: Some(u32::try_from(objects_after).unwrap_or(u32::MAX)),
        reason,
    }
}

fn mib(bytes: u64) -> String {
    format!("{:.1} MiB", bytes as f64 / f64::from(1u32 << 20))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Source;
    use crate::transcode::tags::Overrides;

    /// A valid PCM WAV of `secs` seconds of silence, 44.1 kHz stereo 16-bit.
    fn wav(path: &Path, secs: u32) {
        let data = 44_100 * 4 * secs;
        let mut b = Vec::new();
        b.extend(b"RIFF");
        b.extend((36 + data).to_le_bytes());
        b.extend(b"WAVEfmt ");
        b.extend(16u32.to_le_bytes());
        b.extend(1u16.to_le_bytes()); // PCM
        b.extend(2u16.to_le_bytes());
        b.extend(44_100u32.to_le_bytes());
        b.extend((44_100u32 * 4).to_le_bytes());
        b.extend(4u16.to_le_bytes());
        b.extend(16u16.to_le_bytes());
        b.extend(b"data");
        b.extend(data.to_le_bytes());
        b.resize(b.len() + data as usize, 0);
        std::fs::write(path, b).unwrap();
    }

    #[test]
    fn a_known_duration_is_the_profile_bitrate() {
        let got = estimate(Some(Duration::from_secs(60)), 999, Path::new("a.flac"));
        assert_eq!(got, 60 * 24_000 + EST_OVERHEAD);
    }

    #[test]
    fn an_unknown_duration_falls_back_to_the_size() {
        let wav = estimate(None, 1_411_000, Path::new("a.WAV"));
        assert_eq!(wav, 192_000 + EST_OVERHEAD);
        let flac = estimate(None, 4_000, Path::new("a.flac"));
        assert_eq!(flac, 1_000 + EST_OVERHEAD);
        let mp3 = estimate(Some(Duration::ZERO), 2_000, Path::new("a.mp3"));
        assert_eq!(mp3, 3_000 + EST_OVERHEAD);
    }

    #[test]
    fn a_real_wav_is_estimated_from_its_duration() {
        let t = tempfile::tempdir().unwrap();
        let f = t.path().join("one.wav");
        wav(&f, 2);
        let size = std::fs::metadata(&f).unwrap().len();
        assert_eq!(estimate_bytes(&f, size), 2 * 24_000 + EST_OVERHEAD);
        // Not audio at all: the size fallback, not a failure.
        let g = t.path().join("junk.wav");
        std::fs::write(&g, vec![7u8; 14_110]).unwrap();
        assert_eq!(estimate_bytes(&g, 14_110), 1_920 + EST_OVERHEAD);
    }

    #[test]
    fn the_preview_totals_and_fit_follow_the_verdicts() {
        let t = tempfile::tempdir().unwrap();
        let album = t.path().join("Sea of Thieves");
        std::fs::create_dir(&album).unwrap();
        wav(&album.join("01 - Grogmire.wav"), 1);
        std::fs::copy(
            album.join("01 - Grogmire.wav"),
            album.join("02 - Again.wav"),
        )
        .unwrap();
        std::fs::write(album.join("\u{2117}.wav"), b"x").unwrap();
        let sources = crate::source::expand(std::slice::from_ref(&album)).unwrap();
        let entries = transfer::plan(sources, &Overrides::default());

        let room = Room {
            free_bytes: 10 << 20,
            music_objects: 499,
        };
        let p = build(&entries, None, false, Some(room));
        let verdicts: Vec<_> = p.files.iter().map(|f| f.verdict).collect();
        assert_eq!(
            verdicts,
            [Decision::Send, Decision::Skip, Decision::Refused]
        );
        assert_eq!(p.files[0].title, "Grogmire");
        assert_eq!(p.files[0].track.as_deref(), Some("1"));
        assert_eq!(p.files[0].est_bytes, 24_000 + EST_OVERHEAD);
        assert_eq!(
            p.totals,
            Totals {
                send: 1,
                skip: 1,
                refused: 1,
                est_bytes: 24_000 + EST_OVERHEAD
            }
        );
        assert!(p.fits.ok, "{:?}", p.fits);
        assert_eq!(p.fits.objects_after, Some(500));

        let full = Room {
            music_objects: 500,
            ..room
        };
        let f = build(&entries, None, false, Some(full)).fits;
        assert!(!f.ok && f.reason.unwrap().contains("500"));
        let tight = Room {
            free_bytes: FREE_MARGIN,
            ..room
        };
        assert!(!build(&entries, None, false, Some(tight)).fits.ok);
        assert!(!build(&entries, None, false, None).fits.ok);

        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["files"][0]["verdict"], "send");
        assert_eq!(json["files"][2]["verdict"], "refused");
        assert!(json["files"][1]["reason"]
            .as_str()
            .unwrap()
            .starts_with("same audio as"));
        assert!(json["files"][0].get("reason").is_none());
    }

    #[test]
    fn a_refused_file_is_shown_by_its_file_name() {
        let entries = vec![PlanEntry {
            source: Source {
                path: PathBuf::from("/nowhere/\u{2117}.wav"),
                root: None,
            },
            tags: Err("no title".into()),
        }];
        let p = build(&entries, None, false, None);
        assert_eq!(p.files[0].title, "\u{2117}.wav");
        assert_eq!(p.files[0].reason.as_deref(), Some("no title"));
    }
}
