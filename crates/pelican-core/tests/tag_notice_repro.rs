//! Regression: a blank frame in one tag must not hide a real value in
//! another. Found as a UI notice claiming a tagged file had no title.

use std::path::Path;

use lofty::config::WriteOptions;
use lofty::file::{AudioFile, TaggedFileExt};
use lofty::prelude::Accessor;
use lofty::probe::Probe;
use lofty::tag::{Tag, TagType};
use pelican_core::transcode::tags;

fn write_tone_wav(path: &Path) {
    const RATE: u32 = 44_100;
    let frames = RATE / 2;
    let data_len = frames * 4;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
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

/// A WAV carrying its real title/artist in RIFF INFO, plus an ID3v2 tag whose
/// TIT2/TPE1 are blank — exactly what a tagger that stamps empty frames
/// leaves behind. The file IS tagged; every field the watch needs is present.
#[test]
fn a_blank_primary_frame_must_not_shadow_a_real_value_in_another_tag() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("blank-primary.wav");
    write_tone_wav(&p);

    let mut f = Probe::open(&p).unwrap().read().unwrap();

    let mut info = Tag::new(TagType::RiffInfo);
    info.set_title("Drunken Sailor".to_string());
    info.set_artist("Windrose".to_string());
    f.insert_tag(info);

    let mut id3 = Tag::new(TagType::Id3v2);
    id3.set_title("   ".to_string());
    id3.set_artist("   ".to_string());
    f.insert_tag(id3);

    f.save_to_path(&p, WriteOptions::default()).unwrap();

    let got = tags::Tags::read(&p).unwrap();
    // This used to fail: `from_tagged`'s `first` closure ran `find_map`
    // over the tags and only then `clean`ed, so the blank ID3v2 frame won
    // the search and was thrown away, and the RIFF INFO tag holding the
    // real values was never consulted. The path fallback would now paper
    // over it with the filename — which is exactly why the tag read has to
    // be right on its own.
    assert_eq!(got.title.as_deref(), Some("Drunken Sailor"));
    assert_eq!(got.artist.as_deref(), Some("Windrose"));
}
