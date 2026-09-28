//! End to end through the real ffmpeg: source file → resolved tags →
//! staging dir → the one profile. Hardware-free, but not a mock.
//!
//! What the watch was proven to play (`garmin-library-persistence.md`
//! § Results) is a CBR 192 kbps, 44.1 kHz, stereo MPEG-1 Layer III file
//! with an ID3v2.3 tag, no ID3v1 trailer and no art. Each of those is
//! checked here against the bytes ffmpeg actually wrote, not against the
//! arguments we passed it.
//!
//! Skips with a message when ffmpeg is not installed.

use std::path::{Path, PathBuf};

use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::{Accessor, ItemKey, TagExt};
use lofty::probe::Probe;
use lofty::tag::TagType;
use pelican_core::source;
use pelican_core::staging::StagingDir;
use pelican_core::transcode::encoder;
use pelican_core::transcode::tags::{Overrides, Resolved};

/// A two-second 440 Hz tone as 16-bit PCM WAV at `rate`, `channels` wide.
fn write_tone_wav(path: &Path, rate: u32, channels: u16) {
    let frames = rate * 2;
    let block = u32::from(channels) * 2;
    let data_len = frames * block;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&channels.to_le_bytes());
    b.extend_from_slice(&rate.to_le_bytes());
    b.extend_from_slice(&(rate * block).to_le_bytes());
    b.extend_from_slice(&(block as u16).to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = f64::from(i) / f64::from(rate);
        let v = (20_000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
        for _ in 0..channels {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    std::fs::write(path, b).unwrap();
}

fn ffmpeg_or_skip(test: &str) -> bool {
    if encoder::available() {
        return true;
    }
    eprintln!("skipped {test}: ffmpeg is not installed");
    false
}

/// Offset of the first MPEG audio frame: just past the ID3v2 tag.
fn first_frame(mp3: &[u8]) -> usize {
    assert_eq!(&mp3[..3], b"ID3", "no ID3v2 tag at the start");
    // Syncsafe size: four 7-bit bytes.
    let size = mp3[6..10]
        .iter()
        .fold(0usize, |acc, b| (acc << 7) | usize::from(*b & 0x7F));
    10 + size
}

/// Assert everything the profile promises about the encoded bytes.
fn assert_profile(mp3: &[u8]) {
    // ID3v2.3: major version byte 3.
    assert_eq!(mp3[3], 3, "ID3v2.{} written, not ID3v2.3", mp3[3]);
    // No ID3v1 trailer.
    assert!(
        mp3.len() < 128 || &mp3[mp3.len() - 128..mp3.len() - 125] != b"TAG",
        "ID3v1 trailer present"
    );

    let off = first_frame(mp3);
    let h = &mp3[off..off + 4];
    assert_eq!(h[0], 0xFF, "no frame sync after the tag");
    assert_eq!(h[1] & 0xE0, 0xE0, "no frame sync after the tag");
    assert_eq!((h[1] >> 3) & 0b11, 0b11, "not MPEG-1");
    assert_eq!((h[1] >> 1) & 0b11, 0b01, "not Layer III");
    assert_eq!(h[2] >> 4, 0b1011, "bitrate index is not 192 kbps");
    assert_eq!((h[2] >> 2) & 0b11, 0b00, "sample rate is not 44.1 kHz");
    assert_ne!(h[3] >> 6, 0b11, "mono");

    // LAME marks a CBR stream's first frame "Info"; a VBR one "Xing".
    let first = &mp3[off..(off + 418).min(mp3.len())];
    assert!(
        first.windows(4).any(|w| w == b"Info"),
        "no LAME CBR (Info) header"
    );
    assert!(!first.windows(4).any(|w| w == b"Xing"), "stream is VBR");

    // Every later frame header agrees on bitrate: constant, not average.
    let mut at = off;
    let mut frames = 0;
    while at + 4 <= mp3.len() && mp3[at] == 0xFF && mp3[at + 1] & 0xE0 == 0xE0 {
        assert_eq!(mp3[at + 2] >> 4, 0b1011, "frame {frames} is not 192 kbps");
        let padding = usize::from((mp3[at + 2] >> 1) & 1);
        at += 144 * 192_000 / 44_100 + padding;
        frames += 1;
    }
    assert!(frames > 50, "only {frames} frames walked: not a CBR stream");
}

/// R1 + R2 + R8 together, on the rebuild plan's proof case: an untagged
/// WAV in `Sea of Thieves/`, mono and at 48 kHz so the resample and the
/// upmix both have to happen.
#[test]
fn sea_of_thieves_wav_becomes_the_proven_profile() {
    if !ffmpeg_or_skip("sea_of_thieves_wav_becomes_the_proven_profile") {
        return;
    }
    let lib = tempfile::tempdir().unwrap();
    let album = lib.path().join("Sea of Thieves");
    std::fs::create_dir(&album).unwrap();
    let wav = album.join("02 - Maiden Voyage.wav");
    write_tone_wav(&wav, 48_000, 1);
    let before = std::fs::read(&wav).unwrap();

    let src = &source::expand(std::slice::from_ref(&wav)).unwrap()[0];
    let tags = Resolved::for_file(&src.path, src.root.as_deref(), &Overrides::default()).unwrap();

    let cache = tempfile::tempdir().unwrap();
    let staging = StagingDir::create_in(cache.path()).unwrap();
    let out = staging.file(1);
    encoder::encode(&src.path, &out, &tags).unwrap();

    let mp3 = std::fs::read(&out).unwrap();
    assert_profile(&mp3);

    let f = Probe::open(&out).unwrap().read().unwrap();
    let p = f.properties();
    assert_eq!(p.sample_rate(), Some(44_100));
    assert_eq!(p.channels(), Some(2));
    // Not `audio_bitrate()`: lofty derives it from stream length over
    // duration, which the encoder's priming frames skew to ~194 on a clip
    // this short. The frame headers `assert_profile` walked are the
    // authority on the bitrate, and they all say 192.

    let t = f.tag(TagType::Id3v2).expect("an ID3v2 tag");
    assert_eq!(t.title().as_deref(), Some("Maiden Voyage"));
    assert_eq!(t.artist().as_deref(), Some("Sea of Thieves"));
    assert_eq!(t.album().as_deref(), Some("Sea of Thieves"));
    assert_eq!(t.get_string(ItemKey::AlbumArtist), Some("Sea of Thieves"));
    assert_eq!(t.track(), Some(2));
    assert!(f.tag(TagType::Id3v1).is_none());

    assert_eq!(
        std::fs::read(&wav).unwrap(),
        before,
        "the source was modified"
    );
    let dir = staging.path().to_path_buf();
    drop(staging);
    assert!(!dir.exists(), "staging outlived the run");
}

/// A tagged FLAC carrying frames Garmin's indexer rejects and a cover:
/// only the allowlist reaches the output, and the art does not.
#[test]
fn a_tagged_flac_keeps_the_allowlist_and_nothing_else() {
    if !ffmpeg_or_skip("a_tagged_flac_keeps_the_allowlist_and_nothing_else") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("tone.wav");
    write_tone_wav(&wav, 44_100, 2);
    let flac: PathBuf = dir.path().join("source.flac");
    let ok = std::process::Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(&wav)
        .arg(&flac)
        .status()
        .unwrap()
        .success();
    assert!(ok, "could not build the FLAC fixture");

    let mut file = Probe::open(&flac).unwrap().read().unwrap();
    let tag = file
        .primary_tag_mut()
        .expect("FLAC has a Vorbis comment block");
    tag.set_title("Ghost of Time".to_string());
    tag.set_album("Master and Commander".to_string());
    tag.set_artist("Richard Tognetti".to_string());
    tag.insert_text(
        ItemKey::AlbumArtist,
        "Iva Davies, Christopher Gordon, Richard Tognetti".to_string(),
    );
    tag.insert_text(ItemKey::CopyrightMessage, "\u{2117} 2003 Decca".to_string());
    tag.insert_text(ItemKey::Comment, "QBZ:TID 12345".to_string());
    tag.set_track(11);
    tag.set_genre("Soundtrack".to_string());
    tag.insert_text(ItemKey::RecordingDate, "2003".to_string());
    tag.save_to_path(&flac, lofty::config::WriteOptions::default())
        .unwrap();

    let tags = Resolved::for_file(&flac, None, &Overrides::default()).unwrap();
    let staging = StagingDir::create_in(dir.path()).unwrap();
    let out = staging.file(1);
    encoder::encode(&flac, &out, &tags).unwrap();
    assert_profile(&std::fs::read(&out).unwrap());

    let f = Probe::open(&out).unwrap().read().unwrap();
    let t = f.tag(TagType::Id3v2).expect("an ID3v2 tag");
    assert_eq!(t.title().as_deref(), Some("Ghost of Time"));
    assert_eq!(
        t.artist().as_deref(),
        Some("Iva Davies, Christopher Gordon, Richard Tognetti"),
        "album_artist is the grouping key"
    );
    assert_eq!(t.track(), Some(11));
    assert_eq!(t.genre().as_deref(), Some("Soundtrack"));
    assert!(t.get_string(ItemKey::CopyrightMessage).is_none());
    assert!(t.get_string(ItemKey::Comment).is_none());
    assert_eq!(t.picture_count(), 0);
}

/// A title override is not a thing; the run-wide overrides are.
#[test]
fn run_overrides_reach_the_file() {
    if !ffmpeg_or_skip("run_overrides_reach_the_file") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("01 - Intro.wav");
    write_tone_wav(&wav, 44_100, 2);
    let ov = Overrides {
        artist: Some("Tester".into()),
        album: Some("Reach".into()),
        genre: Some("Score".into()),
        year: Some("2010".into()),
    };
    let tags = Resolved::for_file(&wav, None, &ov).unwrap();
    let staging = StagingDir::create_in(dir.path()).unwrap();
    encoder::encode(&wav, &staging.file(1), &tags).unwrap();
    let f = Probe::open(staging.file(1)).unwrap().read().unwrap();
    let t = f.tag(TagType::Id3v2).unwrap();
    assert_eq!(t.title().as_deref(), Some("Intro"));
    assert_eq!(t.artist().as_deref(), Some("Tester"));
    assert_eq!(t.album().as_deref(), Some("Reach"));
    assert_eq!(t.genre().as_deref(), Some("Score"));
    let back = pelican_core::transcode::tags::Tags::read(&staging.file(1)).unwrap();
    assert_eq!(back.date.as_deref(), Some("2010"));
}

/// A mix song, as ffmpeg writes it: the mix is the album, "Various
/// Artists" the album artist, the song keeps its artist, and the track is
/// its place in the mix. Not yet proven on the watch.
#[test]
fn a_mix_song_carries_the_mix_tags() {
    use pelican_core::transcode::tags::{Mix, MIX_ALBUM_ARTIST};

    if !ffmpeg_or_skip("a_mix_song_carries_the_mix_tags") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let album = dir.path().join("Windrose");
    std::fs::create_dir(&album).unwrap();
    let wav = album.join("01 - Intro.wav");
    write_tone_wav(&wav, 44_100, 2);
    let mix = Mix {
        name: "Long Run".into(),
    };
    let tags = Resolved::for_file(&wav, None, &Overrides::default())
        .unwrap()
        .into_mix(&mix, 3, &Overrides::default())
        .unwrap();
    let staging = StagingDir::create_in(dir.path()).unwrap();
    encoder::encode(&wav, &staging.file(1), &tags).unwrap();
    let f = Probe::open(staging.file(1)).unwrap().read().unwrap();
    let t = f.tag(TagType::Id3v2).unwrap();
    assert_eq!(t.title().as_deref(), Some("Intro"));
    assert_eq!(t.artist().as_deref(), Some("Windrose"));
    assert_eq!(t.album().as_deref(), Some("Long Run"));
    assert_eq!(t.get_string(ItemKey::AlbumArtist), Some(MIX_ALBUM_ARTIST));
    assert_eq!(t.track(), Some(3));
}

#[test]
fn ffmpeg_failure_is_an_error_naming_the_file() {
    if !ffmpeg_or_skip("ffmpeg_failure_is_an_error_naming_the_file") {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let bogus = dir.path().join("not audio.flac");
    std::fs::write(&bogus, b"definitely not a flac").unwrap();
    let tags = Resolved::for_file(&bogus, None, &Overrides::default()).unwrap();
    let staging = StagingDir::create_in(dir.path()).unwrap();
    let err = encoder::encode(&bogus, &staging.file(1), &tags).unwrap_err();
    assert!(err.to_string().contains("not audio.flac"), "{err}");
}

/// The whole run with the real encoder, into the fake watch: what lands is
/// the proven profile, proven by read-back, and nothing is left staged.
#[test]
fn a_push_sends_the_profile_and_proves_it() {
    use pelican_core::ledger::Ledger;
    use pelican_core::mtp::fake::FakeDevice;
    use pelican_core::transfer::{self, Env, Options};

    if !ffmpeg_or_skip("a_push_sends_the_profile_and_proves_it") {
        return;
    }
    let tmp = tempfile::tempdir().unwrap();
    let album = tmp.path().join("Sea of Thieves");
    std::fs::create_dir(&album).unwrap();
    write_tone_wav(&album.join("02 - Maiden Voyage.wav"), 48_000, 1);
    let entries = transfer::plan(
        source::expand(std::slice::from_ref(&album)).unwrap(),
        &Overrides::default(),
    );
    let dev = FakeDevice::new();
    let mut ledger = Ledger::open(&tmp.path().join("data"), "1").unwrap();
    let cache = tmp.path().join("cache");
    let report = transfer::push(
        entries,
        &mut ledger,
        || Ok(dev.backend()),
        Options::default(),
        Env {
            staging_base: &cache,
            encode: &encoder::encode,
            progress: &mut |_| {},
            stop: transfer::Stop::new(),
        },
    )
    .unwrap();
    assert_eq!(report.tally().verified, 1);

    let files = dev.files("Music");
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, "pl00001-Maiden Voyage.mp3");
    assert_profile(&files[0].1);
    let staged: Vec<PathBuf> = std::fs::read_dir(cache.join("staging"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    assert!(staged.is_empty(), "{staged:?}");
}
