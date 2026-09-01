//! End-to-end logic tests for the Garmin-quirk workarounds. These are the
//! parts of the pipeline most likely to regress silently — they encode hard-
//! won knowledge about what the watch firmware accepts. If any of these
//! ever fails, double-check against `docs/garmin-mtp.md` before
//! "fixing" the test.
//!
//! These used to assert against constants re-declared inside this file,
//! because `pelican` was a binary crate and an integration test could not
//! see into it. That made them documentation that could not fail. Now that
//! the engine is a library they bind to the real implementation, so drift
//! in `transcode`/`transfer` breaks the build instead of passing quietly.

use std::path::Path;

use pelican_core::{transcode, transfer};

/// Longest stem the sanitizer will emit, and the value the firmware limit
/// is reasoned against.
const STEM_CAP: usize = 56;

#[test]
fn filename_cap_holds_against_the_firmware_limit() {
    // Garmin firmware silently rejects writes with a remote_name longer
    // than ~60 chars (verified empirically on FR165 / firmware 2506).
    // We cap stems at 56 to leave room for ".mp3" plus a safety margin,
    // since some firmware trims at 60 inclusive of the dot-extension.
    assert!(STEM_CAP + ".mp3".len() <= 60);

    // And the implementation actually honours it, for input far over the
    // limit and for input that is pure punctuation.
    for raw in [
        "11 - Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time - Tognetti Into the Fog",
        &"x".repeat(400),
        "…".repeat(200).as_str(),
    ] {
        let out = transcode::sanitize_filename_stem(raw);
        assert!(
            out.chars().count() <= STEM_CAP,
            "{} chars from {raw:?}",
            out.chars().count()
        );
        assert!(!out.is_empty(), "sanitizer must never return an empty stem");
    }
}

#[test]
fn supported_formats_match_firmware() {
    // Playable formats per Garmin's official audio FAQ:
    // https://support.garmin.com/en-US/?faq=JyNEOTsZaR3KMXqej3oQp5
    // Anything here MUST upload through `--no-transcode` without going
    // near an encoder.
    let claimed = ["mp3", "m4a", "m4b", "aac", "wav"];
    assert_eq!(
        transfer::SUPPORTED_EXTS,
        &claimed,
        "Garmin's native format list changed — update SUPPORTED_EXTS and this test together"
    );
    for ext in claimed {
        assert!(
            transfer::ext_supported(Path::new(&format!("track.{ext}"))),
            "{ext} must be recognised as natively playable"
        );
        // Case must not matter: a file off a FAT volume can arrive shouting.
        assert!(transfer::ext_supported(Path::new(&format!(
            "track.{}",
            ext.to_uppercase()
        ))));
    }
}

#[test]
fn transcodable_formats_are_recognised_as_audio() {
    // Formats we route through an encoder because Garmin won't play them.
    // flac is the load-bearing one (Qobuz / Bandcamp downloads).
    let need_transcode = [
        "flac", "ogg", "oga", "opus", "wma", "ape", "aiff", "aif", "wv", "alac",
    ];
    for ext in need_transcode {
        let p = format!("track.{ext}");
        assert!(
            transcode::is_audio(Path::new(&p)),
            "{ext} must be seen as audio or it is silently skipped"
        );
        assert!(
            !transfer::ext_supported(Path::new(&p)),
            "{ext} must NOT be treated as natively playable"
        );
    }
    // Non-audio must stay out of the plan entirely.
    for ext in ["jpg", "cue", "txt", "log", "pdf"] {
        assert!(!transcode::is_audio(Path::new(&format!("f.{ext}"))));
    }
}

#[test]
fn tag_values_are_stripped_of_what_the_indexer_rejects() {
    // Garmin's music indexer silently rejects MP3s whose ID3v2.3 tag has
    // non-standard frames or exotic glyphs (verified: QBZ:TID, COPYRIGHT
    // with ℗ unicode, DISCTOTAL, TRACKTOTAL, SUBTITLE, ISRC, APIC art).
    for glyph in ['\u{2117}', '\u{00A9}', '\u{2122}', '\u{00AE}'] {
        let out = transcode::sanitize_tag_value(&format!("Album {glyph} 2024"));
        assert!(!out.contains(glyph), "{glyph:?} survived: {out:?}");
    }
    // Control bytes would let a crafted tag forge lines in terminal output.
    let out = transcode::sanitize_tag_value("Track\x1b[31m\x07Name\n");
    assert!(!out.chars().any(char::is_control), "{out:?}");
    // But real text must survive intact — over-sanitizing mangles libraries.
    assert_eq!(
        transcode::sanitize_tag_value("  Café Tacvba  "),
        "Café Tacvba"
    );
}

#[test]
fn sanitized_names_never_collide_within_one_plan() {
    // MTP has no overwrite semantics: two jobs resolving to the same remote
    // name means the watch either clobbers one or shows two files the user
    // cannot tell apart. The 56-char truncation makes this easy to trigger.
    let long = "A".repeat(80);
    let inputs: Vec<std::path::PathBuf> = (0..5)
        .map(|i| std::path::PathBuf::from(format!("/src/disc{i}/{long}.mp3")))
        .collect();

    let dir = tempfile::tempdir().unwrap();
    let mut real = Vec::new();
    for p in &inputs {
        let sub = dir.path().join(p.parent().unwrap().file_name().unwrap());
        std::fs::create_dir_all(&sub).unwrap();
        let f = sub.join(p.file_name().unwrap());
        std::fs::write(&f, b"x").unwrap();
        real.push(f);
    }

    let jobs = transfer::expand_inputs_with(&real, "Music", true, true).unwrap();
    let mut seen = std::collections::HashSet::new();
    for j in &jobs {
        let stem = Path::new(&j.remote_name)
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let final_name = transcode::sanitize_filename_stem(&stem);
        assert!(
            final_name.chars().count() <= STEM_CAP,
            "{final_name} exceeds the cap after dedupe"
        );
        assert!(
            seen.insert((j.remote_dir.clone(), final_name.to_lowercase())),
            "two jobs resolve to the same remote name: {:?}",
            j.remote_name
        );
    }
    assert_eq!(jobs.len(), 5, "every input must still be planned");
}
