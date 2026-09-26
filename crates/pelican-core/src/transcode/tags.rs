//! Reading source tags, and writing the strict tag Garmin will accept.
//!
//! Garmin's music indexer silently rejects files whose tag carries
//! non-standard frames — custom Vorbis fields like `QBZ:TID`, `DISCTOTAL`,
//! the `℗` glyph inside `COPYRIGHT`, embedded cover art. So we never carry a
//! source tag across; we read what we want, drop everything else, and write
//! a fresh tag from a seven-field allowlist.
//!
//! Reading used to mean spawning `ffprobe` and parsing its JSON, which made
//! tag support hostage to an external binary and needed a hand-rolled guard
//! against a tag value forging extra output lines. `lofty` reads ID3v2,
//! Vorbis comments, MP4 atoms and APE tags in-process, on every platform,
//! for every format it knows. For the ones it does not (WMA among them) a
//! read failure is not a refusal: the file still has a path, and the path
//! is enough to name it — see [`Resolved`].

use std::path::Path;

use anyhow::{anyhow, Context, Result};
use lofty::config::ParseOptions;
use lofty::file::TaggedFileExt;
use lofty::prelude::{Accessor, ItemKey};
use lofty::probe::Probe;

use super::sanitize_tag_value;

/// The allowlisted fields as the source file carries them. Every one is
/// optional; [`Resolved`] is what fills the gaps.
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
    ///
    /// Cover art is not parsed: it is never written, and on a FLAC carrying a
    /// 1 MB JPEG it is almost the whole cost of the read.
    pub fn read(src: &Path) -> Result<Self> {
        let tagged = Probe::open(src)
            .with_context(|| format!("opening {} for tag read", src.display()))?
            .options(ParseOptions::new().read_cover_art(false))
            .read()
            .with_context(|| format!("parsing tags in {}", src.display()))?;
        Ok(Self::from_tagged(&tagged))
    }

    fn from_tagged(tagged: &lofty::file::TaggedFile) -> Self {
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

        // `clean` runs INSIDE the search, not after it. Cleaning afterwards
        // made the fallthrough above a lie: `find_map` stops at the first tag
        // that carries the key *at all*, so a tag holding "   " — or a bare
        // ℗ that `sanitize_tag_value` strips to nothing — won the search and
        // was then discarded, and the tag holding the real value was never
        // read. A blank value is not a value; keep looking.
        let first = |f: &dyn Fn(&lofty::tag::Tag) -> Option<String>| -> Option<String> {
            tags.iter().find_map(|t| f(t).and_then(clean))
        };

        let album_artist = first(&|t| t.get_string(ItemKey::AlbumArtist).map(str::to_owned));
        let artist = first(&|t| t.artist().map(|s| s.into_owned()));

        Self {
            title: first(&|t| t.title().map(|s| s.into_owned())),
            artist: album_artist.or(artist),
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
        }
    }
}

/// Values the user gave for the whole run (`--artist`, `--album`,
/// `--genre`, `--year`). They beat anything the file or its path says.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Overrides {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
}

/// The tag that will be written, every field decided.
///
/// A file that reaches the watch without a title or artist is on the
/// storage and absent from the music app — the WAV in
/// `garmin-library-persistence.md` § Results is exactly that. So nothing is
/// left to chance: each field resolves **override → source tag → path**,
/// and a file whose title still comes out empty is refused rather than sent
/// to be invisible.
///
/// The path fallbacks, for `…/Artist/Album/02 - Title.wav`:
/// - title: the filename stem, minus a leading track number (`02 - `,
///   `02_`, `02.`, `2 `);
/// - track: that leading number;
/// - album: the parent directory's name;
/// - artist: the grandparent's name — **only** when the file was found by
///   walking a directory named on the command line and the grandparent lies
///   strictly inside it. Otherwise the grandparent is whatever happened to
///   hold the album (a NAS share, `~/Downloads`), which says nothing about
///   who made it, and the album name is the honest fallback: it keeps the
///   album grouped on the watch and claims nothing false.
/// - date, genre: no fallback; omitted unless tagged or overridden.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    /// Just the number, without a `/total` — the watch sorts by it.
    pub track: Option<String>,
    pub date: Option<String>,
    pub genre: Option<String>,
}

impl Resolved {
    /// Read `src` and resolve. `root` is the directory named on the command
    /// line that `src` was found under, or `None` when `src` itself was
    /// named.
    ///
    /// A file lofty cannot parse is not refused here: ffmpeg reads formats
    /// lofty does not, and the path still resolves a title. Whether the
    /// file is audio at all is ffmpeg's call to make.
    pub fn for_file(src: &Path, root: Option<&Path>, ov: &Overrides) -> Result<Self> {
        let tags = Tags::read(src).unwrap_or_else(|e| {
            tracing::warn!(
                file = %src.display(),
                error = %format!("{e:#}"),
                "could not read tags; naming from the path"
            );
            Tags::default()
        });
        Self::resolve(src, root, &tags, ov)
    }

    /// The pure half of [`Resolved::for_file`], split out so every fallback
    /// can be tested without writing tagged audio.
    pub fn resolve(src: &Path, root: Option<&Path>, tags: &Tags, ov: &Overrides) -> Result<Self> {
        let stem = src
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (leading, rest) = split_track_prefix(&stem);
        let dir_name = |p: Option<&Path>| {
            p.and_then(Path::file_name)
                .map(|n| n.to_string_lossy().into_owned())
                .and_then(clean)
        };
        let parent = src.parent();
        let grandparent = parent.and_then(Path::parent);

        let title = tags
            .title
            .clone()
            .or_else(|| clean(rest.to_string()))
            .ok_or_else(|| anyhow!("{}: no title in its tags or its filename", src.display()))?;
        let album = pick(&ov.album, &tags.album).or_else(|| dir_name(parent));
        let walked_artist = match (root, grandparent) {
            (Some(root), Some(gp)) if gp != root && gp.starts_with(root) => dir_name(Some(gp)),
            _ => None,
        };
        let artist = pick(&ov.artist, &tags.artist)
            .or(walked_artist)
            .or_else(|| album.clone());
        let track = tags
            .track
            .as_deref()
            .and_then(track_number)
            .or_else(|| leading.map(|n| n.to_string()));

        Ok(Self {
            title,
            artist,
            album,
            track,
            date: pick(&ov.year, &tags.date),
            genre: pick(&ov.genre, &tags.genre),
        })
    }

    /// `(key, value)` pairs for ffmpeg's `-metadata`, in a fixed order.
    ///
    /// `album_artist` is written from the resolved artist, not read
    /// separately: Garmin groups by it, and a soundtrack whose per-track
    /// artists differ from its album artist fragments into several albums.
    /// Absent fields are left out, never written empty.
    pub fn as_ffmpeg_args(&self) -> Vec<(&'static str, String)> {
        let title = Some(self.title.clone());
        [
            ("title", &title),
            ("artist", &self.artist),
            ("album_artist", &self.artist),
            ("album", &self.album),
            ("track", &self.track),
            ("date", &self.date),
            ("genre", &self.genre),
        ]
        .into_iter()
        .filter_map(|(k, v)| v.clone().map(|v| (k, v)))
        .collect()
    }
}

/// Override if given (and non-blank after sanitizing), else the tag.
fn pick(ov: &Option<String>, tag: &Option<String>) -> Option<String> {
    ov.clone().and_then(clean).or_else(|| tag.clone())
}

/// Split a leading track number off a filename stem.
///
/// Recognises one to three digits followed by a separator — `01 - `, `01_`,
/// `01.`, `1 ` — and returns the number and what follows. Capped at three
/// digits so a title that *starts* with a year, like `2001 A Space Odyssey`,
/// keeps it. Without a separator (`1999`) there is no prefix. If stripping
/// would leave nothing, there is no prefix either — the digits are the name.
fn split_track_prefix(stem: &str) -> (Option<u32>, &str) {
    let digits = stem.bytes().take_while(u8::is_ascii_digit).count();
    if digits == 0 || digits > 3 {
        return (None, stem);
    }
    let after = &stem[digits..];
    let rest = after.trim_start_matches([' ', '-', '_', '.']);
    if rest.len() == after.len() || rest.trim().is_empty() {
        return (None, stem);
    }
    (stem[..digits].parse().ok(), rest)
}

/// `"2/12"` → `"2"`, `" 07 "` → `"7"`. Anything unparsable is dropped
/// rather than written as a track number the watch cannot sort by.
fn track_number(v: &str) -> Option<String> {
    v.split('/')
        .next()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|n| *n > 0)
        .map(|n| n.to_string())
}

/// Sanitize, then drop the value entirely if nothing survived.
fn clean(v: String) -> Option<String> {
    let s = sanitize_tag_value(&v);
    (!s.is_empty()).then_some(s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn untagged(path: &str, root: Option<&str>) -> Resolved {
        Resolved::resolve(
            Path::new(path),
            root.map(Path::new),
            &Tags::default(),
            &Overrides::default(),
        )
        .unwrap()
    }

    /// The proof case from the rebuild plan: an untagged WAV pushed by
    /// naming it directly. The NAS share above the album says nothing
    /// about the artist, so the album name stands in.
    #[test]
    fn sea_of_thieves_resolves_from_its_path() {
        let r = untagged("/mnt/nas/music/Sea of Thieves/02 - Maiden Voyage.wav", None);
        assert_eq!(r.title, "Maiden Voyage");
        assert_eq!(r.track.as_deref(), Some("2"));
        assert_eq!(r.album.as_deref(), Some("Sea of Thieves"));
        assert_eq!(r.artist.as_deref(), Some("Sea of Thieves"));
        assert_eq!(r.date, None);
        assert_eq!(r.genre, None);
    }

    /// Same file, found by walking the album directory itself: the
    /// grandparent is outside the root, so it still is not the artist.
    #[test]
    fn sea_of_thieves_walked_from_the_album_dir() {
        let r = untagged(
            "/mnt/nas/music/Sea of Thieves/02 - Maiden Voyage.wav",
            Some("/mnt/nas/music/Sea of Thieves"),
        );
        assert_eq!(r.artist.as_deref(), Some("Sea of Thieves"));
    }

    #[test]
    fn grandparent_inside_the_walked_root_is_the_artist() {
        let r = untagged("/lib/Windrose/Wintersaga/03_Mylir.flac", Some("/lib"));
        assert_eq!(r.title, "Mylir");
        assert_eq!(r.track.as_deref(), Some("3"));
        assert_eq!(r.album.as_deref(), Some("Wintersaga"));
        assert_eq!(r.artist.as_deref(), Some("Windrose"));
    }

    /// The root itself is not "inside" the root: walking `~/Music` that
    /// holds `Album/01.wav` directly must not credit the artist "Music".
    #[test]
    fn the_walked_root_itself_is_never_the_artist() {
        let r = untagged(
            "/home/u/Music/Wintersaga/01. Mylir.flac",
            Some("/home/u/Music"),
        );
        assert_eq!(r.artist.as_deref(), Some("Wintersaga"));
    }

    #[test]
    fn title_prefix_forms_are_all_stripped() {
        for (stem, title, track) in [
            ("01 - Maiden Voyage", "Maiden Voyage", "1"),
            ("01_Maiden Voyage", "Maiden Voyage", "1"),
            ("01.Maiden Voyage", "Maiden Voyage", "1"),
            ("01. Maiden Voyage", "Maiden Voyage", "1"),
            ("1 Maiden Voyage", "Maiden Voyage", "1"),
            ("12-Maiden Voyage", "Maiden Voyage", "12"),
        ] {
            let r = untagged(&format!("/a/b/{stem}.wav"), None);
            assert_eq!(r.title, title, "from {stem:?}");
            assert_eq!(r.track.as_deref(), Some(track), "from {stem:?}");
        }
    }

    #[test]
    fn numbers_that_are_the_title_are_kept() {
        // `01Intro` too: without a separator the digits are part of the name,
        // as R2 (amended) says.
        for stem in ["2001 A Space Odyssey", "1999", "42", "01Intro", "4Minutes"] {
            let r = untagged(&format!("/a/b/{stem}.wav"), None);
            assert_eq!(r.title, stem);
            assert_eq!(r.track, None, "{stem:?} has no track prefix");
        }
    }

    #[test]
    fn tags_beat_the_path() {
        let tags = Tags {
            title: Some("Real Title".into()),
            artist: Some("Real Artist".into()),
            album: Some("Real Album".into()),
            track: Some("7/12".into()),
            date: Some("1979".into()),
            genre: Some("Folk".into()),
        };
        let r = Resolved::resolve(
            Path::new("/lib/X/Y/02 - Wrong.wav"),
            Some(Path::new("/lib")),
            &tags,
            &Overrides::default(),
        )
        .unwrap();
        assert_eq!(r.title, "Real Title");
        assert_eq!(r.artist.as_deref(), Some("Real Artist"));
        assert_eq!(r.album.as_deref(), Some("Real Album"));
        assert_eq!(r.track.as_deref(), Some("7"));
        assert_eq!(r.date.as_deref(), Some("1979"));
        assert_eq!(r.genre.as_deref(), Some("Folk"));
    }

    #[test]
    fn overrides_beat_tags() {
        let tags = Tags {
            title: Some("T".into()),
            artist: Some("Tag Artist".into()),
            album: Some("Tag Album".into()),
            genre: Some("Tag Genre".into()),
            date: Some("1999".into()),
            ..Default::default()
        };
        let ov = Overrides {
            artist: Some("Cli Artist".into()),
            album: Some("Cli Album".into()),
            genre: Some("Cli Genre".into()),
            year: Some("2026".into()),
        };
        let r = Resolved::resolve(Path::new("/a/b/c.flac"), None, &tags, &ov).unwrap();
        assert_eq!(r.title, "T", "there is no title override");
        assert_eq!(r.artist.as_deref(), Some("Cli Artist"));
        assert_eq!(r.album.as_deref(), Some("Cli Album"));
        assert_eq!(r.genre.as_deref(), Some("Cli Genre"));
        assert_eq!(r.date.as_deref(), Some("2026"));
    }

    /// An album override also feeds the artist fallback, so a run pushed
    /// with `--album` alone still groups under one artist on the watch.
    #[test]
    fn album_override_feeds_the_artist_fallback() {
        let ov = Overrides {
            album: Some("Mixtape".into()),
            ..Default::default()
        };
        let r =
            Resolved::resolve(Path::new("/x/y/01 - a.wav"), None, &Tags::default(), &ov).unwrap();
        assert_eq!(r.artist.as_deref(), Some("Mixtape"));
    }

    #[test]
    fn a_blank_override_does_not_blank_the_field() {
        let tags = Tags {
            title: Some("T".into()),
            artist: Some("Tag Artist".into()),
            ..Default::default()
        };
        let ov = Overrides {
            artist: Some("  \u{2117} ".into()),
            ..Default::default()
        };
        let r = Resolved::resolve(Path::new("/a/b/c.flac"), None, &tags, &ov).unwrap();
        assert_eq!(r.artist.as_deref(), Some("Tag Artist"));
    }

    #[test]
    fn path_derived_values_are_sanitized() {
        let r = untagged(
            "/lib/Art\u{7}ist/Al\u{2117}bum/01 - Ti\u{1b}tle.wav",
            Some("/lib"),
        );
        assert_eq!(r.title, "Title");
        assert_eq!(r.album.as_deref(), Some("Album"));
        assert_eq!(r.artist.as_deref(), Some("Artist"));
    }

    #[test]
    fn an_empty_title_is_refused() {
        let err = Resolved::resolve(
            Path::new("/a/b/\u{2117}.wav"),
            None,
            &Tags::default(),
            &Overrides::default(),
        )
        .unwrap_err();
        assert!(err.to_string().contains("no title"), "{err}");
    }

    #[test]
    fn a_file_at_the_filesystem_root_still_resolves() {
        let r = untagged("/01 - Lone.wav", None);
        assert_eq!(r.title, "Lone");
        assert_eq!(r.album, None);
        assert_eq!(r.artist, None);
    }

    #[test]
    fn unparsable_track_tags_fall_back_to_the_filename() {
        let tags = Tags {
            track: Some("side A".into()),
            ..Default::default()
        };
        let r = Resolved::resolve(
            Path::new("/a/b/04 - x.wav"),
            None,
            &tags,
            &Overrides::default(),
        )
        .unwrap();
        assert_eq!(r.track.as_deref(), Some("4"));
    }

    #[test]
    fn ffmpeg_args_write_album_artist_from_the_resolved_artist() {
        let r = untagged("/lib/Windrose/Wintersaga/03 - Mylir.flac", Some("/lib"));
        let args = r.as_ffmpeg_args();
        let get = |k| {
            args.iter()
                .find(|(key, _)| *key == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("artist"), get("album_artist"));
        let keys: Vec<_> = args.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            ["title", "artist", "album_artist", "album", "track"],
            "absent fields must be left out, present ones in order"
        );
    }

    #[test]
    fn ffmpeg_args_carry_all_seven_fields_when_known() {
        let r = Resolved {
            title: "t".into(),
            artist: Some("a".into()),
            album: Some("b".into()),
            track: Some("1".into()),
            date: Some("2026".into()),
            genre: Some("g".into()),
        };
        let keys: Vec<_> = r.as_ffmpeg_args().iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            [
                "title",
                "artist",
                "album_artist",
                "album",
                "track",
                "date",
                "genre"
            ]
        );
    }

    #[test]
    fn clean_drops_values_that_sanitize_to_nothing() {
        assert_eq!(clean("\u{2117}".into()), None);
        assert_eq!(clean("   ".into()), None);
        assert_eq!(clean("Café".into()), Some("Café".to_string()));
    }

    #[test]
    fn unreadable_source_resolves_from_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let album = dir.path().join("Sea of Thieves");
        std::fs::create_dir(&album).unwrap();
        let f: PathBuf = album.join("02 - Maiden Voyage.wma");
        std::fs::write(&f, b"not really wma").unwrap();
        let r = Resolved::for_file(&f, None, &Overrides::default()).unwrap();
        assert_eq!(r.title, "Maiden Voyage");
        assert_eq!(r.album.as_deref(), Some("Sea of Thieves"));
    }
}
