//! Choosing how a file gets turned into something the watch will play.
//!
//! Three pipelines. Which one a file takes is decided by what the *watch*
//! can play, not by what happens to be installed:
//!
//! 1. **Passthrough**, for MP3/M4A/M4B/AAC/WAV. The watch plays these as-is,
//!    so the audio is copied byte-for-byte and only the tag is rewritten, in
//!    process. No external tool, nothing re-encoded, nothing lost. This is
//!    most of what a Qobuz library actually contains.
//! 2. **ffmpeg → CBR 192 kbps MP3**, for everything else, when installed.
//!    The profile verified on Forerunner 165 Music, firmware 2506.
//! 3. **afconvert → CBR 192 kbps AAC in M4A**, for what it can read, when
//!    ffmpeg is absent. macOS ships `/usr/bin/afconvert`, so this needs
//!    nothing installed. Verified locally that it decodes FLAC/ALAC/AIFF and
//!    encodes AAC, and that it *cannot* encode MP3
//!    (`ExtAudioFileSetProperty ('cfmt') failed`) — hence the different
//!    output container.
//!
//! Garmin lists M4A/AAC as natively playable, but **the afconvert output has
//! not been confirmed against real hardware**; only the ffmpeg MP3 profile
//! has. ffmpeg therefore wins wherever both exist, and afconvert is what a
//! Mac with nothing installed falls back to rather than refusing the file.

use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Result};

use super::tags::Tags;

/// An external tool that can convert audio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoder {
    /// Anywhere, if installed. Reads everything we accept.
    Ffmpeg,
    /// macOS only, always present. Reads a useful subset.
    AfConvert,
}

/// How a particular source file will be made ready for the watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pipeline {
    /// Copy the audio untouched; rewrite only the tag, in process.
    Passthrough,
    /// Decode and re-encode through an external tool.
    Encode(Encoder),
}

/// afconvert's fixed location. It ships with macOS; it is not something a
/// user installs, so there is no PATH lookup to do — and a GUI launched from
/// Finder inherits a four-entry PATH that would not find it anyway.
pub const AFCONVERT_PATH: &str = "/usr/bin/afconvert";

/// Formats `afconvert` was verified to decode on macOS 26.6.2.
///
/// Deliberately shorter than [`super::AUDIO_EXTS`]: CoreAudio advertises Ogg
/// Vorbis and Opus in `afconvert -hf`, but `ExtAudioFile` refused to produce
/// either in testing, so they are not claimed here. WMA, APE and WavPack are
/// absent from CoreAudio entirely.
pub const AFCONVERT_DECODES: &[&str] = &[
    "flac", "alac", "aiff", "aif", "wav", "mp3", "m4a", "m4b", "aac",
];

impl Encoder {
    pub fn name(self) -> &'static str {
        match self {
            Encoder::Ffmpeg => "ffmpeg",
            Encoder::AfConvert => "afconvert",
        }
    }

    /// Container this encoder produces. Both are formats Garmin plays.
    pub fn output_ext(self) -> &'static str {
        match self {
            Encoder::Ffmpeg => "mp3",
            Encoder::AfConvert => "m4a",
        }
    }

    pub fn can_decode(self, ext: &str) -> bool {
        match self {
            // ffmpeg reads everything we are willing to accept.
            Encoder::Ffmpeg => super::AUDIO_EXTS
                .iter()
                .any(|e| e.eq_ignore_ascii_case(ext)),
            Encoder::AfConvert => AFCONVERT_DECODES
                .iter()
                .any(|e| e.eq_ignore_ascii_case(ext)),
        }
    }

    /// Is this encoder usable on this machine right now?
    pub fn available(self) -> bool {
        match self {
            // `normalize` no longer needs ffprobe — lofty reads the tags —
            // so ffmpeg alone is enough.
            Encoder::Ffmpeg => binary_available("ffmpeg", "-version"),
            // afconvert cannot be probed by exit status: it returns 2 for
            // every form of help and for a bare invocation, so a status
            // check reports "missing" on a Mac that has it. It is a system
            // binary at a fixed path, so test for the file instead.
            Encoder::AfConvert => cfg!(target_os = "macos") && is_executable(AFCONVERT_PATH),
        }
    }

    /// Convert `src` into `dst`, writing only `tags`.
    pub fn encode(self, src: &Path, dst: &Path, tags: &Tags) -> Result<()> {
        match self {
            Encoder::Ffmpeg => encode_ffmpeg(src, dst, tags),
            Encoder::AfConvert => encode_afconvert(src, dst, tags),
        }
    }
}

/// Every encoder present on this machine, best-trusted first.
pub fn available() -> Vec<Encoder> {
    [Encoder::Ffmpeg, Encoder::AfConvert]
        .into_iter()
        .filter(|e| e.available())
        .collect()
}

/// Decide how `src` should be handled, or explain why it cannot be.
///
/// **A format the watch already plays is never re-encoded.** Running one
/// through an encoder would be lossy for no gain — a WAV would come back as
/// AAC, and an MP3 would be rebuilt to strip tag frames we can strip in
/// process anyway. The audio is copied byte-for-byte and only the tag is
/// rewritten, which is both lossless and free of any external tool.
///
/// The encoder exists for the formats Garmin genuinely cannot play.
pub fn plan(src: &Path) -> Result<Pipeline> {
    if crate::transfer::ext_supported(src) {
        return Ok(Pipeline::Passthrough);
    }
    let ext = src
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let encoders = available();
    match encoders.iter().copied().find(|e| e.can_decode(&ext)) {
        Some(e) => Ok(Pipeline::Encode(e)),
        None => Err(unsupported(&ext, &encoders)),
    }
}

/// A refusal that names the format, what is installed, and what would fix it.
fn unsupported(ext: &str, present: &[Encoder]) -> anyhow::Error {
    let have = if present.is_empty() {
        "no encoder is installed".to_string()
    } else {
        format!(
            "the encoder{} available here ({}) cannot read it",
            if present.len() == 1 { "" } else { "s" },
            present
                .iter()
                .map(|e| e.name())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    anyhow!(
        ".{ext} cannot be converted: {have}. \
         Install ffmpeg to handle every format Pelican accepts."
    )
}

/// True when `path` exists and any execute bit is set.
fn is_executable(path: &str) -> bool {
    let Ok(meta) = std::fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn binary_available(bin: &str, probe_arg: &str) -> bool {
    Command::new(bin)
        .arg(probe_arg)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Garmin's reliable profile: CBR 192 kbps, 44.1 kHz, stereo, ID3v2.3 with
/// no ID3v1 trailer and no cover art.
fn encode_ffmpeg(src: &Path, dst: &Path, tags: &Tags) -> Result<()> {
    let mut cmd = Command::new("ffmpeg");
    // `-i` rather than a bare positional: ffmpeg-family tools have no `--`
    // terminator, so a path starting with '-' would be parsed as options.
    cmd.args(["-y", "-hide_banner", "-loglevel", "error", "-i"])
        .arg(src)
        .arg("-vn"); // drop embedded art — an oversized APIC gets the file rejected
    if super::is_mp3(src) {
        // Tag rewrite only; the audio bitstream is already what we want.
        cmd.args(["-c:a", "copy"]);
    } else {
        cmd.args([
            "-ac",
            "2",
            "-ar",
            "44100",
            "-codec:a",
            "libmp3lame",
            "-b:a",
            "192k",
        ]);
    }
    cmd.args([
        "-map_metadata",
        "-1",
        "-id3v2_version",
        "3",
        "-write_id3v1",
        "0",
    ]);
    for (k, v) in tags.as_ffmpeg_args() {
        cmd.arg("-metadata").arg(format!("{k}={v}"));
    }
    cmd.arg(dst);
    run(cmd, "ffmpeg", src)
}

/// CBR 192 kbps AAC in an M4A container, via the encoder every Mac ships.
/// afconvert carries no metadata across, which suits us — the tag is written
/// afterwards from the allowlist, so there is nothing to strip.
fn encode_afconvert(src: &Path, dst: &Path, tags: &Tags) -> Result<()> {
    let mut cmd = Command::new(AFCONVERT_PATH);
    cmd.arg(src)
        .args(["-f", "m4af", "-d", "aac", "-b", "192000"])
        // `-s 0` selects the CBR allocation strategy; VBR output has not
        // been tested against the watch's indexer.
        .args(["-s", "0"])
        .arg(dst);
    run(cmd, "afconvert", src)?;
    tags.write_mp4(dst)
}

fn run(mut cmd: Command, tool: &str, src: &Path) -> Result<()> {
    let out = cmd
        .output()
        .map_err(|e| anyhow!("running {tool} on {}: {e}", src.display()))?;
    if !out.status.success() {
        return Err(anyhow!(
            "{tool} failed for {} :: {}",
            src.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[cfg(target_os = "macos")]
    fn afconvert_is_detected_on_macos() {
        // Regression: `available()` used to probe by exit status, but
        // afconvert returns 2 for `-h`, `--help`, `-hf` and a bare call
        // alike. Every Mac reported "no encoder installed", and the
        // FLAC path silently fell back to being unsupported.
        assert!(
            Encoder::AfConvert.available(),
            "afconvert ships with macOS at {AFCONVERT_PATH} and must be found"
        );
        assert!(available().contains(&Encoder::AfConvert));
    }

    #[test]
    fn afconvert_does_not_claim_formats_coreaudio_refused() {
        // Verified on macOS 26.6.2: afconvert -hf lists Ogg/Opus, but
        // ExtAudioFile refused to produce either. Claiming them would turn a
        // clear "install ffmpeg" into a confusing mid-transfer failure.
        for ext in ["ogg", "oga", "opus", "wma", "ape", "wv"] {
            assert!(
                !Encoder::AfConvert.can_decode(ext),
                "afconvert must not claim .{ext}"
            );
        }
        for ext in ["flac", "alac", "aiff", "wav", "mp3"] {
            assert!(Encoder::AfConvert.can_decode(ext), ".{ext} was verified");
        }
    }

    #[test]
    fn ffmpeg_covers_everything_we_accept() {
        for ext in super::super::AUDIO_EXTS {
            assert!(
                Encoder::Ffmpeg.can_decode(ext),
                "ffmpeg is the fallback for .{ext} and must read it"
            );
        }
    }

    #[test]
    fn encoders_emit_containers_garmin_plays() {
        for e in [Encoder::Ffmpeg, Encoder::AfConvert] {
            let out = PathBuf::from(format!("x.{}", e.output_ext()));
            assert!(
                crate::transfer::ext_supported(&out),
                "{} emits .{} which the watch cannot play",
                e.name(),
                e.output_ext()
            );
        }
    }

    #[test]
    fn native_formats_are_never_re_encoded() {
        // Two things at once: an MP3/WAV library syncs on a machine with no
        // tools installed, and — the regression that prompted this — a WAV
        // is not silently downgraded to lossy AAC just because an encoder
        // happens to be present and can read it.
        for name in ["a.mp3", "a.wav", "a.m4a", "a.m4b", "a.aac", "a.WAV"] {
            let p = PathBuf::from(name);
            assert_eq!(
                plan(&p).unwrap(),
                Pipeline::Passthrough,
                "{name} is playable as-is and must not go through an encoder"
            );
        }
    }

    #[test]
    fn refusal_names_the_format_and_the_fix() {
        let e = unsupported("wma", &[Encoder::AfConvert]);
        let msg = e.to_string();
        assert!(msg.contains(".wma"), "{msg}");
        assert!(msg.contains("afconvert"), "{msg}");
        assert!(msg.contains("ffmpeg"), "must say what would fix it: {msg}");
    }

    #[test]
    fn refusal_reads_correctly_with_no_encoder_at_all() {
        let msg = unsupported("flac", &[]).to_string();
        assert!(msg.contains("no encoder is installed"), "{msg}");
    }
}
