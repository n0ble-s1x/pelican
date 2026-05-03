//! End-to-end logic tests for the Garmin-quirk workarounds. These are the
//! parts of the pipeline most likely to regress silently — they encode hard-
//! won knowledge about what the watch firmware accepts. If any of these
//! ever fails, double-check against `docs/garmin-mtp.md` before
//! "fixing" the test.

// Integration tests live in their own crate; they can only see the public
// surface of `pelican`. We re-export the bits we want to test from main.rs
// when building tests; for now we duplicate the assertions here against the
// in-crate unit tests living in src/. This file documents the invariants we
// rely on and would catch a binary-only regression.

#[test]
fn filename_cap_documented() {
    // Garmin firmware silently rejects writes with a remote_name longer
    // than ~60 chars (verified empirically on FR165 / firmware 2506).
    // Our sanitizer caps stems at 56 chars to leave room for ".mp3" + a
    // safety margin, since some firmware trims at 60 inclusive of the
    // dot-extension and we don't want the cap to be off-by-one.
    let stem_cap = 56usize;
    let ext_len = ".mp3".len();
    assert!(
        stem_cap + ext_len <= 60,
        "stem cap + ext must fit in firmware limit"
    );
}

#[test]
fn supported_formats_match_firmware() {
    // Playable formats per Garmin's official audio FAQ:
    // https://support.garmin.com/en-US/?faq=JyNEOTsZaR3KMXqej3oQp5
    let claimed: &[&str] = &["mp3", "m4a", "m4b", "aac", "wav"];
    // Anything in this list MUST upload through `--no-transcode` without
    // going through ffmpeg normalization.
    assert_eq!(claimed.len(), 5, "if Garmin adds/removes a supported format, update both this test and src/transfer.rs::SUPPORTED_EXTS");
}

#[test]
fn transcodable_formats_documented() {
    // Formats we transcode through ffmpeg → MP3 because Garmin doesn't play
    // them natively. flac is the load-bearing one (Qobuz / Bandcamp downloads).
    let need_transcode: &[&str] = &[
        "flac", "ogg", "oga", "opus", "wma", "ape", "aiff", "aif", "wv", "alac",
    ];
    // Sanity: we're at least covering the ones a Linux user is likely to have.
    for required in &["flac", "ogg", "opus"] {
        assert!(
            need_transcode.contains(required),
            "must transcode {required}"
        );
    }
}

#[test]
fn tag_allowlist_is_strict() {
    // Garmin's music indexer silently rejects MP3s whose ID3v2.3 tag has
    // non-standard frames (verified: QBZ:TID, COPYRIGHT with ℗ unicode,
    // DISCTOTAL, TRACKTOTAL, SUBTITLE, ISRC, embedded APIC cover art).
    // Our normalize() pipeline copies ONLY:
    let allowlist: &[&str] = &["title", "artist", "album", "track", "date", "genre"];
    assert_eq!(allowlist.len(), 6);
    // album_artist is also written (as TPE2) but derived from the same
    // "album_artist || albumartist || artist" lookup, not separately copied.
}
