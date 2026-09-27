//! Turning the paths on a command line into the files a run will send.
//!
//! Only ever reads. The one thing it records beyond the file itself is
//! *how it was found*: a file named directly has no root, a file found by
//! walking a directory carries that directory. Tag resolution needs the
//! difference — only inside a directory the user pointed at is the folder
//! above an album trusted to be the artist (see
//! [`crate::transcode::tags::Resolved`]).

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::transcode::is_audio;

/// One file to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// Absolute path. Absolute because ffmpeg reads a relative `proto:…`
    /// argument as a URL. A named file is canonicalized; a walked one keeps
    /// the path it was found at under its (canonical) root — for a symlink,
    /// the link, not its target — because the folders around it are what
    /// its tags fall back to.
    pub path: PathBuf,
    /// The directory on the command line this was found under, canonical;
    /// `None` when the file itself was named.
    pub root: Option<PathBuf>,
}

/// Expand command-line paths into sources, in a stable order.
///
/// - A named file is taken if it is audio by extension, and refused with
///   an error otherwise: naming a `.jpg` is a mistake worth saying out
///   loud, not a file to skip quietly.
/// - A named directory is walked recursively. Hidden entries (a leading
///   `.`) and non-audio files are skipped; so are symlinked directories,
///   which is what keeps a link loop from walking forever. Symlinked
///   files are followed.
/// - Directory entries are sorted, so a given tree always plans in the
///   same order.
/// - The same file reached twice (by canonical path) is kept once, at its
///   first position.
///
/// A path that does not exist, or a directory entry that cannot be read,
/// is an error: a file silently missing from a run is a track the user
/// believes is on their watch.
pub fn expand(paths: &[PathBuf]) -> Result<Vec<Source>> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for p in paths {
        let canon = p
            .canonicalize()
            .with_context(|| format!("cannot read {}", p.display()))?;
        if canon.is_dir() {
            walk(&canon, &canon, &mut out, &mut seen)?;
        } else if is_audio(&canon) {
            push(&mut out, &mut seen, canon, None)?;
        } else {
            bail!(
                "{} is not an audio file Pelican recognises (by extension)",
                p.display()
            );
        }
    }
    Ok(out)
}

fn walk(dir: &Path, root: &Path, out: &mut Vec<Source>, seen: &mut HashSet<PathBuf>) -> Result<()> {
    let mut entries = Vec::new();
    for ent in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let ent = ent.with_context(|| format!("reading {}", dir.display()))?;
        if ent.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        entries.push(ent.path());
    }
    entries.sort();
    for path in entries {
        let meta = std::fs::symlink_metadata(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        if meta.is_dir() {
            walk(&path, root, out, seen)?;
        } else if meta.file_type().is_symlink() {
            // Follow a link to a file; never one to a directory.
            if path.is_file() && is_audio(&path) {
                push(out, seen, path, Some(root))?;
            }
        } else if meta.is_file() && is_audio(&path) {
            push(out, seen, path, Some(root))?;
        }
    }
    Ok(())
}

fn push(
    out: &mut Vec<Source>,
    seen: &mut HashSet<PathBuf>,
    path: PathBuf,
    root: Option<&Path>,
) -> Result<()> {
    let key = path
        .canonicalize()
        .with_context(|| format!("resolving {}", path.display()))?;
    if seen.insert(key) {
        out.push(Source {
            path,
            root: root.map(Path::to_path_buf),
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn tree(paths: &[&str]) -> tempfile::TempDir {
        let t = tempfile::tempdir().unwrap();
        for p in paths {
            let full = t.path().join(p);
            fs::create_dir_all(full.parent().unwrap()).unwrap();
            fs::write(full, b"x").unwrap();
        }
        t
    }

    fn rel(t: &tempfile::TempDir, s: &[Source]) -> Vec<String> {
        let base = t.path().canonicalize().unwrap();
        s.iter()
            .map(|s| {
                s.path
                    .strip_prefix(&base)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    }

    #[test]
    fn walks_recursively_sorted_skipping_hidden_and_non_audio() {
        let t = tree(&[
            "Artist/Album/02 - b.flac",
            "Artist/Album/01 - a.wav",
            "Artist/Album/cover.jpg",
            "Artist/Album/.01 - a.wav.part",
            "Artist/.hidden/x.mp3",
            "Artist/Album/album.cue",
        ]);
        let got = expand(&[t.path().to_path_buf()]).unwrap();
        assert_eq!(
            rel(&t, &got),
            ["Artist/Album/01 - a.wav", "Artist/Album/02 - b.flac"]
        );
        let root = t.path().canonicalize().unwrap();
        assert!(got
            .iter()
            .all(|s| s.root.as_deref() == Some(root.as_path())));
    }

    #[test]
    fn a_named_file_has_no_root() {
        let t = tree(&["Sea of Thieves/02 - Maiden Voyage.wav"]);
        let f = t.path().join("Sea of Thieves/02 - Maiden Voyage.wav");
        let got = expand(&[f]).unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].root, None);
        assert!(got[0].path.is_absolute());
    }

    #[test]
    fn a_named_non_audio_file_is_an_error() {
        let t = tree(&["notes.txt"]);
        let err = expand(&[t.path().join("notes.txt")]).unwrap_err();
        assert!(err.to_string().contains("not an audio file"), "{err}");
    }

    #[test]
    fn a_missing_path_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(expand(&[t.path().join("nope.flac")]).is_err());
    }

    #[test]
    fn the_same_file_twice_is_sent_once() {
        let t = tree(&["A/1.mp3"]);
        let f = t.path().join("A/1.mp3");
        let got = expand(&[t.path().join("A"), f.clone(), f]).unwrap();
        assert_eq!(got.len(), 1);
        assert!(got[0].root.is_some(), "first sighting wins");
    }

    #[test]
    fn relative_inputs_come_out_absolute() {
        let t = tree(&["A/1.mp3"]);
        let cwd = std::env::current_dir().unwrap();
        // Build a relative path to the temp dir from wherever tests run.
        let depth = cwd.components().count() - 1;
        let up: PathBuf = std::iter::repeat_n("..", depth).collect();
        let relative = up.join(t.path().strip_prefix("/").unwrap()).join("A/1.mp3");
        let got = expand(&[relative]).unwrap();
        assert!(got[0].path.is_absolute());
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_directories_are_not_followed() {
        let t = tree(&["real/a.mp3"]);
        std::os::unix::fs::symlink(t.path().join("real"), t.path().join("real/loop")).unwrap();
        let got = expand(&[t.path().to_path_buf()]).unwrap();
        assert_eq!(rel(&t, &got), ["real/a.mp3"]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_files_are_followed_and_keep_their_link_path() {
        let t = tree(&["store/a.flac", "lib/placeholder.txt"]);
        std::os::unix::fs::symlink(t.path().join("store/a.flac"), t.path().join("lib/a.flac"))
            .unwrap();
        let got = expand(&[t.path().join("lib")]).unwrap();
        assert_eq!(rel(&t, &got), ["lib/a.flac"]);
        // Reached again through its target: still one source.
        let got = expand(&[t.path().join("lib"), t.path().join("store")]).unwrap();
        assert_eq!(got.len(), 1);
    }
}
