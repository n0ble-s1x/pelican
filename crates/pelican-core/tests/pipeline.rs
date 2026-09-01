//! End-to-end exercise of the normalization pipeline on real audio.
//!
//! Hardware-free, but not a mock: real files go through the real encoder
//! selection, the real tag rebuild, and come back out readable. This is the
//! layer that used to have no coverage at all — `normalize` shells out and
//! rewrites tags, and neither could regress visibly before now.
//!
//! Anything needing a tool that isn't installed skips rather than fails, so
//! this runs on a bare macOS box and on a Linux box with ffmpeg alike.

use std::path::{Path, PathBuf};

use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey, TagExt};
use lofty::probe::Probe;
use pelican_core::transcode::{self, encoder};

/// A two-second 440 Hz stereo tone as a 16-bit PCM WAV.
fn write_tone_wav(path: &Path) {
    const RATE: u32 = 44_100;
    const SECS: u32 = 2;
    let frames = RATE * SECS;
    let data_len = frames * 4; // 2 ch * 2 bytes

    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); // PCM chunk size
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&2u16.to_le_bytes()); // stereo
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes()); // byte rate
    b.extend_from_slice(&4u16.to_le_bytes()); // block align
    b.extend_from_slice(&16u16.to_le_bytes()); // bits
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f64 / RATE as f64;
        let v = (20_000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
        b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, b).unwrap();
}

/// Tag a staged source file so we can prove the tag survives the pipeline.
fn tag_source(path: &Path) {
    let mut file = Probe::open(path).unwrap().read().unwrap();
    let ty = file.primary_tag_type();
    if file.primary_tag().is_none() {
        file.insert_tag(lofty::tag::Tag::new(ty));
    }
    let tag = file.primary_tag_mut().unwrap();
    tag.set_title("Ghost of Time".to_string());
    tag.set_album("Master and Commander".to_string());
    tag.insert_text(
        ItemKey::AlbumArtist,
        "Iva Davies, Christopher Gordon, Richard Tognetti".to_string(),
    );
    tag.set_artist("Richard Tognetti".to_string());
    // A frame Garmin's indexer rejects, plus a glyph the sanitizer strips.
    tag.insert_text(ItemKey::CopyrightMessage, "\u{2117} 2003 Decca".to_string());
    tag.save_to_path(path, lofty::config::WriteOptions::default())
        .unwrap();
}

fn read_back(path: &Path) -> transcode::tags::Tags {
    transcode::tags::Tags::read(path).unwrap()
}

/// Make a FLAC beside `wav`, or return None when nothing can encode one.
fn make_flac(dir: &Path, wav: &Path) -> Option<PathBuf> {
    let out = dir.join("source.flac");
    let ok = if cfg!(target_os = "macos") {
        std::process::Command::new("afconvert")
            .arg(wav)
            .args(["-f", "flac", "-d", "flac"])
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(wav)
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    ok.then_some(out)
}

#[test]
fn native_wav_passes_through_without_any_encoder() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("A Tone.wav");
    write_tone_wav(&wav);

    let staged = transcode::normalize(&wav, Some("A Tone")).expect("wav must never need a tool");
    assert_eq!(staged.remote_name, "A Tone.wav");
    assert!(staged.path.exists());
    // Copied byte-for-byte: WAV has no tag format Garmin reads, so we must
    // not have touched the audio.
    assert_eq!(
        std::fs::metadata(&wav).unwrap().len(),
        std::fs::metadata(&staged.path).unwrap().len(),
        "passthrough must not re-encode"
    );
}

#[test]
fn staged_file_is_deleted_when_dropped() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone_wav(&wav);

    let path = {
        let staged = transcode::normalize(&wav, None).unwrap();
        let p = staged.path.clone();
        assert!(p.exists());
        p
    };
    assert!(
        !path.exists(),
        "a failed or finished transfer must not leave audio in the cache dir"
    );
}

#[test]
fn tags_are_rebuilt_from_the_allowlist_not_carried_across() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone_wav(&wav);
    let Some(flac) = make_flac(dir.path(), &wav) else {
        eprintln!("skipped: no encoder available to build a FLAC source");
        return;
    };
    tag_source(&flac);

    let staged = match transcode::normalize(&flac, Some("Ghost of Time")) {
        Ok(s) => s,
        // Only tolerate this where nothing can genuinely convert a FLAC.
        // macOS always can — afconvert is part of the OS — so a failure
        // there is a real regression, not an absent tool. Skipping quietly
        // is how the encoder-detection bug hid in the first place.
        Err(e) if !cfg!(target_os = "macos") && encoder::available().is_empty() => {
            eprintln!("skipped: no encoder available to convert FLAC ({e})");
            return;
        }
        Err(e) => panic!("FLAC conversion must work on this platform: {e:#}"),
    };

    let ext = staged
        .path
        .extension()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(
        pelican_core::transfer::ext_supported(Path::new(&staged.remote_name)),
        "output {} is not a container Garmin plays",
        staged.remote_name
    );
    assert!(staged.remote_name.ends_with(&format!(".{ext}")));

    let got = read_back(&staged.path);
    assert_eq!(got.title.as_deref(), Some("Ghost of Time"));
    assert_eq!(got.album.as_deref(), Some("Master and Commander"));
    // album_artist wins over the per-track composer credit, or a soundtrack
    // fragments into one album per composer on the watch.
    assert_eq!(
        got.artist.as_deref(),
        Some("Iva Davies, Christopher Gordon, Richard Tognetti"),
        "album_artist must win over artist"
    );

    // The rejected frame must not have survived into the output at all.
    let out = Probe::open(&staged.path).unwrap().read().unwrap();
    for tag in out.tags() {
        assert!(
            tag.get_string(ItemKey::CopyrightMessage).is_none(),
            "COPYRIGHT survived the rebuild — Garmin's indexer rejects the file"
        );
    }
}

#[test]
fn planning_agrees_with_what_normalize_produces() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone_wav(&wav);

    // Whatever the planner says will happen has to be what actually happens,
    // or the GUI's "converted with X" label lies to the user.
    match encoder::plan(&wav).unwrap() {
        encoder::Pipeline::Passthrough => {
            let staged = transcode::normalize(&wav, None).unwrap();
            assert!(staged.remote_name.ends_with(".wav"));
        }
        encoder::Pipeline::Encode(e) => {
            let staged = transcode::normalize(&wav, None).unwrap();
            assert!(
                staged
                    .remote_name
                    .ends_with(&format!(".{}", e.output_ext())),
                "planner said {} but got {}",
                e.output_ext(),
                staged.remote_name
            );
        }
    }
}

#[test]
fn unsupported_format_is_refused_with_a_reason_naming_the_fix() {
    let dir = tempfile::tempdir().unwrap();
    // A real container we cannot convert without ffmpeg. Contents don't
    // matter: the refusal happens at planning, before anything is read.
    let wma = dir.path().join("track.wma");
    std::fs::write(&wma, b"not really a wma").unwrap();

    if encoder::available().contains(&encoder::Encoder::Ffmpeg) {
        eprintln!("skipped: ffmpeg present, so .wma is supported here");
        return;
    }
    let err = transcode::normalize(&wma, None).unwrap_err().to_string();
    assert!(err.contains(".wma"), "{err}");
    assert!(err.contains("ffmpeg"), "must name the fix: {err}");
}
