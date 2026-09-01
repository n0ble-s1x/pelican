//! Reading source tags, and writing the strict tag Garmin will accept.
//!
//! Garmin's music indexer silently rejects files whose tag carries
//! non-standard frames — custom Vorbis fields like `QBZ:TID`, `DISCTOTAL`,
//! the `℗` glyph inside `COPYRIGHT`, embedded cover art. So we never carry a
//! source tag across; we read what we want, drop everything else, and write
//! a fresh tag from a six-field allowlist.
//!
//! Reading used to mean spawning `ffprobe` and parsing its JSON, which made
//! tag support hostage to an external binary and needed a hand-rolled guard
//! against a tag value forging extra output lines. `lofty` reads ID3v2,
//! Vorbis comments, MP4 atoms and APE tags in-process, on every platform,
//! for every format we accept.

use std::path::Path;

use anyhow::{Context, Result};
use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;

use super::sanitize_tag_value;

/// The only fields that reach the device.
///
/// `album_artist` is written as well as `artist`, from the same resolved
/// value — see [`Tags::read`] for why they are deliberately the same string.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub track: Option<String>,
    pub date: Option<String>,
    pub genre: Option<String>,
}

impl Tags {
    /// Read a source file's tags, keeping only the allowlist.
    ///
    /// Prefers `album_artist` as the ARTIST we write. Garmin's library groups
    /// tracks by ARTIST+ALBUM, so per-track composer credits ("Iva Davies"
    /// vs "Richard Tognetti") fragment one soundtrack into several albums.
    /// The album-wide credit is the right grouping key.
    pub fn read(src: &Path) -> Result<Self> {
        let tagged = Probe::open(src)
            .with_context(|| format!("opening {} for tag read", src.display()))?
            .read()
            .with_context(|| format!("parsing tags in {}", src.display()))?;

        // Primary tag first, then any other tag the file carries — an MP3
        // can hold both ID3v2 and APE, and a FLAC both Vorbis comments and
        // ID3v2. Falling through means a half-tagged file still works.
        let tags: Vec<_> = tagged
            .primary_tag()
            .into_iter()
            .chain(
                tagged
                    .tags()
                    .iter()
                    .filter(|t| tagged.primary_tag().map(|p| p.tag_type()) != Some(t.tag_type())),
            )
            .collect();

        let first = |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| -> Option<String> {
            tags.iter().find_map(|t| f(t)).and_then(clean)
        };

        let album_artist = first(&|t| t.get_string(ItemKey::AlbumArtist).map(str::to_owned));
        let artist = first(&|t| t.artist().map(|s| s.into_owned()));
        let resolved_artist = album_artist.or(artist);

        Ok(Self {
            title: first(&|t| t.title().map(|s| s.into_owned())),
            artist: resolved_artist,
            album: first(&|t| t.album().map(|s| s.into_owned())),
            track: first(&|t| t.track().map(|n| n.to_string())),
            // Garmin only ever shows a year. Prefer whatever literal string
            // the file carries, so "1979" from a Vorbis DATE survives intact,
            // and fall back to the parsed timestamp's year component.
            date: first(&|t| {
                t.get_string(ItemKey::RecordingDate)
                    .map(str::to_owned)
                    .or_else(|| t.date().map(|d| d.year.to_string()))
            }),
            genre: first(&|t| t.genre().map(|s| s.into_owned())),
        })
    }

    /// True when the watch's music app will be able to show this file.
    /// Untagged files land on disk but stay invisible in the library.
    pub fn playable_in_library(&self) -> bool {
        self.title.is_some() && self.artist.is_some()
    }

    /// `-metadata k=v` arguments for the ffmpeg pipeline.
    pub fn as_ffmpeg_args(&self) -> Vec<(&'static str, String)> {
        let mut out = Vec::new();
        for (k, v) in [
            ("title", &self.title),
            ("artist", &self.artist),
            // Also written as TPE2 for players that read it.
            ("album_artist", &self.artist),
            ("album", &self.album),
            ("track", &self.track),
            ("date", &self.date),
            ("genre", &self.genre),
        ] {
            if let Some(v) = v {
                out.push((k, v.clone()));
            }
        }
        out
    }

    /// Replace whatever tag `path` carries with exactly this one, as
    /// ID3v2.3. Any existing ID3v1, ID3v2 or APE tag is removed first —
    /// leaving one behind is how a rejected non-standard frame survives.
    pub fn write_id3v23(&self, path: &Path) -> Result<()> {
        use id3::TagLike;

        // Remove first, then write fresh: `write_to_path` replaces the v2
        // tag but would leave a trailing ID3v1 block and its own idea of
        // the genre behind.
        let _ = id3::Tag::remove_from_path(path);

        let mut tag = id3::Tag::new();
        if let Some(v) = &self.title {
            tag.set_title(v);
        }
        if let Some(v) = &self.artist {
            tag.set_artist(v);
            tag.set_album_artist(v);
        }
        if let Some(v) = &self.album {
            tag.set_album(v);
        }
        if let Some(v) = &self.track {
            if let Ok(n) = v.split('/').next().unwrap_or(v).trim().parse::<u32>() {
                tag.set_track(n);
            }
        }
        if let Some(v) = &self.date {
            if let Ok(y) = v.chars().take(4).collect::<String>().parse::<i32>() {
                tag.set_year(y);
            }
        }
        if let Some(v) = &self.genre {
            tag.set_genre(v);
        }
        tag.write_to_path(path, id3::Version::Id3v23)
            .with_context(|| format!("writing ID3v2.3 tag to {}", path.display()))?;
        Ok(())
    }

    /// Replace whatever tag an MP4-family file carries with exactly this
    /// one. Artwork is dropped — an oversized cover is one of the ways a
    /// file gets silently refused.
    pub fn write_mp4(&self, path: &Path) -> Result<()> {
        let mut tag = mp4ameta::Tag::default();
        if let Some(v) = &self.title {
            tag.set_title(v);
        }
        if let Some(v) = &self.artist {
            tag.set_artist(v);
            tag.set_album_artist(v);
        }
        if let Some(v) = &self.album {
            tag.set_album(v);
        }
        if let Some(v) = &self.track {
            if let Ok(n) = v.split('/').next().unwrap_or(v).trim().parse::<u16>() {
                tag.set_track_number(n);
            }
        }
        if let Some(v) = &self.date {
            tag.set_year(v);
        }
        if let Some(v) = &self.genre {
            tag.set_genre(v);
        }
        tag.remove_artworks();
        tag.write_to_path(path)
            .with_context(|| format!("writing MP4 tag to {}", path.display()))?;
        Ok(())
    }
}

/// Sanitize, then drop the value entirely if nothing survived.
fn clean(v: String) -> Option<String> {
    let s = sanitize_tag_value(&v);
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffmpeg_args_write_album_artist_from_the_resolved_artist() {
        let t = Tags {
            artist: Some("Iva Davies, Christopher Gordon".into()),
            title: Some("Ghost of Time".into()),
            ..Default::default()
        };
        let args = t.as_ffmpeg_args();
        let artist = args.iter().find(|(k, _)| *k == "artist").unwrap();
        let album_artist = args.iter().find(|(k, _)| *k == "album_artist").unwrap();
        assert_eq!(
            artist.1, album_artist.1,
            "a soundtrack fragments into several albums if these diverge"
        );
    }

    #[test]
    fn ffmpeg_args_omit_absent_fields_entirely() {
        let t = Tags {
            title: Some("Only a title".into()),
            ..Default::default()
        };
        let args = t.as_ffmpeg_args();
        assert_eq!(args.len(), 1, "empty fields must not become empty tags");
        assert_eq!(args[0].0, "title");
    }

    #[test]
    fn playable_needs_both_title_and_artist() {
        let title_only = Tags {
            title: Some("t".into()),
            ..Default::default()
        };
        assert!(!title_only.playable_in_library());
        let both = Tags {
            title: Some("t".into()),
            artist: Some("a".into()),
            ..Default::default()
        };
        assert!(both.playable_in_library());
    }

    #[test]
    fn clean_drops_values_that_sanitize_to_nothing() {
        assert_eq!(clean("\u{2117}".into()), None);
        assert_eq!(clean("   ".into()), None);
        assert_eq!(clean("Café".into()), Some("Café".to_string()));
    }
}
