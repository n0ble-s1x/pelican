//! Garmin-compatible M3U8 playlist serialization.
//!
//! Garmin watches read `.m3u`/`.m3u8` files at the root of `/Music`. The
//! format is plain UTF-8 text, one filename per line. The watch's music app
//! treats each filename as relative to `/Music`. We accept the optional
//! `#EXTM3U` header on read and write it on output — it doesn't hurt the
//! firmware and helps tools like rhythmbox/foobar2000 recognize the file.

const HEADER: &str = "#EXTM3U";

/// Serialize a list of track filenames into the on-disk M3U8 format.
/// Filenames must already be sanitized to match what's actually in /Music.
pub fn serialize(tracks: &[String]) -> Vec<u8> {
    let mut out = String::with_capacity(tracks.iter().map(|t| t.len() + 1).sum::<usize>() + 16);
    out.push_str(HEADER);
    out.push('\n');
    for t in tracks {
        out.push_str(t.trim());
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
    fn round_trip() {
        let tracks = vec!["track-01.mp3".to_string(), "track-02.mp3".to_string()];
        let bytes = serialize(&tracks);
        let parsed = parse(&bytes);
        assert_eq!(parsed, tracks);
    }

    #[test]
    fn parse_strips_extm3u() {
        let bytes = b"#EXTM3U\n#EXTINF:30,Title\ntrack.mp3\n";
        assert_eq!(parse(bytes), vec!["track.mp3"]);
    }
}
