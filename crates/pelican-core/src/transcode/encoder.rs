//! The one output profile, and the one tool that produces it.
//!
//! Every source — FLAC, WAV, ALAC/AAC in M4A, MP3, OGG, Opus, AIFF, WMA —
//! goes through ffmpeg to **CBR 192 kbps, 44.1 kHz, stereo MP3 with an
//! ID3v2.3 tag and no ID3v1 trailer**. That is the profile proven on the
//! reference FR165 (`garmin-library-persistence.md` § Results): uploaded,
//! read back byte-identical, listed and played through.
//!
//! There is no passthrough, even for a source that is already an MP3. A
//! passthrough file carries whatever frames, art, VBR headers and sample
//! rate it came with, and each of those is a way to land a file on the
//! watch that the music app will not show. One profile means one thing to
//! have proven. The afconvert/AAC path is gone for the same reason: it was
//! never confirmed against hardware.

use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, bail, Result};

use super::tags::Resolved;

/// The ffmpeg binary, looked up on `PATH`.
pub const FFMPEG: &str = "ffmpeg";

/// Is ffmpeg runnable here?
pub fn available() -> bool {
    Command::new(FFMPEG)
        .arg("-version")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Fail with the fix spelled out when ffmpeg is missing.
///
/// Called before a run touches the device, so a machine without ffmpeg
/// learns that from one sentence rather than from a session opened, a
/// listing taken and then every file failing.
pub fn require() -> Result<()> {
    if available() {
        return Ok(());
    }
    Err(missing())
}

fn missing() -> anyhow::Error {
    anyhow!(
        "ffmpeg was not found on PATH. Pelican converts every file with it before \
         anything is sent. Install it (Arch/CachyOS: `pacman -S ffmpeg`; \
         Debian/Ubuntu: `apt install ffmpeg`; Fedora: `dnf install ffmpeg`) and re-run."
    )
}

/// The complete ffmpeg argument list for one file, in order.
///
/// Every flag is here for a reason the watch taught us:
/// - `-nostdin`: ffmpeg otherwise reads the terminal for its `q` key and
///   eats keystrokes meant for us.
/// - `-map 0:a:0 -vn`: the first audio stream only. Cover art rides along
///   as a video stream, and an oversized APIC gets a file refused.
/// - `-map_metadata -1`: nothing from the source tag survives — Garmin's
///   indexer rejects files with non-standard frames. Only the `-metadata`
///   pairs from [`Resolved`] are written.
/// - `-c:a libmp3lame -b:a 192k -ar 44100 -ac 2`: the proven profile.
/// - `-id3v2_version 3 -write_id3v1 0`: ID3v2.3, no v1 trailer.
///
/// No `-y`: the output lives in a staging directory created for this run,
/// so an existing file there is a bug, and ffmpeg refusing it is the right
/// answer.
pub fn ffmpeg_args(src: &Path, dst: &Path, tags: &Resolved) -> Vec<OsString> {
    let mut args: Vec<OsString> = ["-hide_banner", "-loglevel", "error", "-nostdin", "-i"]
        .iter()
        .map(OsString::from)
        .collect();
    args.push(src.as_os_str().to_owned());
    for a in [
        "-map",
        "0:a:0",
        "-vn",
        "-map_metadata",
        "-1",
        "-c:a",
        "libmp3lame",
        "-b:a",
        "192k",
        "-ar",
        "44100",
        "-ac",
        "2",
        "-id3v2_version",
        "3",
        "-write_id3v1",
        "0",
    ] {
        args.push(a.into());
    }
    for (k, v) in tags.as_ffmpeg_args() {
        args.push("-metadata".into());
        args.push(format!("{k}={v}").into());
    }
    args.push(dst.as_os_str().to_owned());
    args
}

/// Encode `src` to `dst` with the one profile, writing only `tags`.
///
/// Both paths must be absolute. ffmpeg has no `--`, and it reads its inputs
/// as URLs: a relative `concat:a|b` or `http:x.flac` names a protocol, and
/// a bare `-` is stdin. A path starting with `/` is none of those.
pub fn encode(src: &Path, dst: &Path, tags: &Resolved) -> Result<()> {
    if !src.is_absolute() || !dst.is_absolute() {
        bail!(
            "internal: ffmpeg paths must be absolute ({} → {})",
            src.display(),
            dst.display()
        );
    }
    if dst.extension().and_then(|e| e.to_str()) != Some("mp3") {
        bail!("internal: output {} is not an .mp3", dst.display());
    }
    let out = Command::new(FFMPEG)
        .args(ffmpeg_args(src, dst, tags))
        .stdin(Stdio::null())
        .output()
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => missing(),
            _ => anyhow!("running ffmpeg on {}: {e}", src.display()),
        })?;
    if !out.status.success() {
        bail!(
            "ffmpeg failed for {} :: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolved() -> Resolved {
        Resolved {
            title: "Maiden Voyage".into(),
            artist: Some("Sea of Thieves".into()),
            album: Some("Sea of Thieves".into()),
            track: Some("2".into()),
            date: None,
            genre: None,
        }
    }

    fn strings(args: Vec<OsString>) -> Vec<String> {
        args.into_iter().map(|a| a.into_string().unwrap()).collect()
    }

    /// The exact argument list from the rebuild plan, R1. A reordering is
    /// harmless to ffmpeg but not to this test, on purpose: the profile is
    /// the thing proven on hardware, and any change to it should be a
    /// deliberate edit here too.
    #[test]
    fn args_are_the_one_profile_exactly() {
        let got = strings(ffmpeg_args(
            Path::new("/src/02 - Maiden Voyage.wav"),
            Path::new("/stage/0001.mp3"),
            &resolved(),
        ));
        let want = [
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-i",
            "/src/02 - Maiden Voyage.wav",
            "-map",
            "0:a:0",
            "-vn",
            "-map_metadata",
            "-1",
            "-c:a",
            "libmp3lame",
            "-b:a",
            "192k",
            "-ar",
            "44100",
            "-ac",
            "2",
            "-id3v2_version",
            "3",
            "-write_id3v1",
            "0",
            "-metadata",
            "title=Maiden Voyage",
            "-metadata",
            "artist=Sea of Thieves",
            "-metadata",
            "album_artist=Sea of Thieves",
            "-metadata",
            "album=Sea of Thieves",
            "-metadata",
            "track=2",
            "/stage/0001.mp3",
        ];
        assert_eq!(got, want);
    }

    /// No passthrough: an MP3 source is re-encoded like anything else.
    #[test]
    fn an_mp3_source_is_not_stream_copied() {
        let args = strings(ffmpeg_args(
            Path::new("/src/a.mp3"),
            Path::new("/stage/a.mp3"),
            &resolved(),
        ));
        assert!(!args.iter().any(|a| a == "copy"), "{args:?}");
        assert!(args.windows(2).any(|w| w == ["-c:a", "libmp3lame"]));
    }

    #[test]
    fn relative_paths_are_refused_before_ffmpeg_runs() {
        let err = encode(
            Path::new("concat:a.wav|b.wav"),
            Path::new("/stage/x.mp3"),
            &resolved(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("absolute"), "{err}");
    }

    #[test]
    fn missing_ffmpeg_error_names_the_fix() {
        let msg = missing().to_string();
        assert!(msg.contains("ffmpeg"), "{msg}");
        assert!(msg.contains("pacman -S ffmpeg"), "{msg}");
    }
}
