//! Garmin-compatible M3U8 playlist serialization.
//!
//! Garmin watches read `.m3u`/`.m3u8` files at the root of `/Music`. The
//! format is plain UTF-8 text. `serialize_for_device` writes Garmin-style
//! absolute paths (`0:/MUSIC/<TRACK>`, uppercase) — what the watch's music
//! app expects when the file lands on the device. Mirrors `better-sync`'s
//! default path style. Path-style choice: see `docs/playlists.md`.

const HEADER: &str = "#EXTM3U";

/// Path style for tracks inside the playlist body. Garmin watches accept a
/// few different path conventions; which one *the watch's library indexer*
/// actually parses depends on model + firmware. Tracks the user picks are
/// always file basenames (no directory components) — the style controls
/// what gets prepended and how the case is normalized.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)] // alternative variants kept as the documented seam for future protocol work
pub enum PathStyle {
    /// `repeat-1.mp3` — bare basename, case preserved. Forum reports for
    /// FR945 / 955 say this works under `Music/`.
    BareCasePreserved,
    /// `0:/MUSIC/REPEAT-1.MP3` — uppercase + Garmin device-path prefix.
    /// `better-sync`'s default style (works on FR family + Venu when the
    /// uploaded music files are themselves uppercase).
    UppercaseWithPrefix,
}

/// Serialize a list of track filenames into the Garmin device M3U8 format.
/// Tracks are basenames in `/Music`; `style` controls how each line is
/// rendered. See `PathStyle` and `docs/playlists.md`.
pub fn serialize_for_device(tracks: &[String], style: PathStyle) -> Vec<u8> {
    let mut out = String::with_capacity(tracks.iter().map(|t| t.len() + 16).sum::<usize>() + 16);
    out.push_str(HEADER);
    out.push('\n');
    for t in tracks {
        let name = t.trim().trim_start_matches('/');
        match style {
            PathStyle::BareCasePreserved => out.push_str(name),
            PathStyle::UppercaseWithPrefix => {
                out.push_str("0:/MUSIC/");
                out.push_str(&name.to_ascii_uppercase());
            }
        }
        out.push('\n');
    }
    out.into_bytes()
}

/// Parse an M3U8 file into its track filenames. Comments and the optional
/// `#EXTM3U`/`#EXTINF` lines are stripped. Lines that look like absolute
/// paths or URLs are passed through as-is so the user can still edit by hand.
pub fn parse(bytes: &[u8]) -> Vec<String> {
    let text = String::from_utf8_lossy(bytes);
    text.lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(|l| l.to_string())
        .collect()
}

/// File extension we'll write. M3U8 vs M3U: the only difference is the
/// implied UTF-8 encoding for `m3u8`. Garmin reads both; we always write
/// `m3u8` because vorbis-derived titles often need UTF-8 anyway.
pub const EXT: &str = "m3u8";

/// True if the filename looks like a playlist we should surface.
pub fn is_playlist(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".m3u") || lower.ends_with(".m3u8")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_strips_extm3u() {
        let bytes = b"#EXTM3U\n#EXTINF:30,Title\ntrack.mp3\n";
        assert_eq!(parse(bytes), vec!["track.mp3"]);
    }

    #[test]
    fn parse_handles_crlf_and_blank_lines() {
        let bytes = b"#EXTM3U\r\ntrack-1.mp3\r\n\r\ntrack-2.mp3\r\n";
        assert_eq!(parse(bytes), vec!["track-1.mp3", "track-2.mp3"]);
    }

    #[test]
    fn parse_only_header_returns_empty() {
        assert!(parse(b"#EXTM3U\n").is_empty());
        assert!(parse(b"").is_empty());
    }

    #[test]
    fn parse_strips_all_comment_lines() {
        let bytes = b"#EXTM3U\n#EXTINF:60,Foo\n#EXTBYT:1234\ntrack.mp3\n#anything-else-with-hash\n";
        assert_eq!(parse(bytes), vec!["track.mp3"]);
    }

    #[test]
    fn is_playlist_extension_match() {
        assert!(is_playlist("foo.m3u"));
        assert!(is_playlist("foo.M3U8"));
        assert!(is_playlist("Bar.m3u8"));
        assert!(!is_playlist("foo.mp3"));
        assert!(!is_playlist("m3u8"));
        assert!(!is_playlist(""));
    }

    #[test]
    fn device_format_skips_leading_slash() {
        let tracks = vec!["/already-rooted.mp3".to_string()];
        let bytes = serialize_for_device(&tracks, PathStyle::BareCasePreserved);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(s, "#EXTM3U\nalready-rooted.mp3\n");
    }

    #[test]
    fn device_format_empty_track_list_is_just_header() {
        let bytes = serialize_for_device(&[], PathStyle::BareCasePreserved);
        assert_eq!(bytes, b"#EXTM3U\n");
    }

    #[test]
    fn device_format_uppercase_with_prefix() {
        let tracks = vec!["Some Track.mp3".to_string(), "another.MP3".to_string()];
        let bytes = serialize_for_device(&tracks, PathStyle::UppercaseWithPrefix);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(
            s,
            "#EXTM3U\n0:/MUSIC/SOME TRACK.MP3\n0:/MUSIC/ANOTHER.MP3\n"
        );
    }

    #[test]
    fn device_format_bare_case_preserved() {
        let tracks = vec!["Some Track.mp3".to_string(), "another.MP3".to_string()];
        let bytes = serialize_for_device(&tracks, PathStyle::BareCasePreserved);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert_eq!(s, "#EXTM3U\nSome Track.mp3\nanother.MP3\n");
    }
}
