//! Browsing the music library on disk, one directory at a time. Read-only.
//!
//! A front-end's "choose" step walks the owner's library (often an NFS
//! share) folder by folder, so each call reads one directory and one level
//! below it — never the whole tree. What counts as audio, and what is
//! hidden, is [`crate::source`]'s rule, so a folder that shows audio here
//! is one a push will find it in.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Serialize;

use crate::transcode::is_audio;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirEntry {
    pub name: String,
    pub path: PathBuf,
    /// Audio files directly inside, not counting subdirectories.
    pub audio_files: u32,
    pub has_subdirs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub path: PathBuf,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Listing {
    /// Canonical.
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<PathBuf>,
    /// Sorted by name, ignoring case.
    pub dirs: Vec<DirEntry>,
    /// Audio files only, sorted by name, ignoring case.
    pub files: Vec<FileEntry>,
}

/// List `dir`: its visible subdirectories, each with how many audio files
/// it holds directly and whether it has subdirectories of its own, and its
/// audio files with their sizes. Hidden entries (a leading `.`) are left
/// out; so are non-audio files.
///
/// `dir` itself must be readable. A subdirectory that cannot be read is
/// still listed, with nothing counted in it.
pub fn list(dir: &Path) -> Result<Listing> {
    let path = dir
        .canonicalize()
        .with_context(|| format!("cannot read {}", dir.display()))?;
    let mut dirs = Vec::new();
    let mut files = Vec::new();
    for (name, p) in visible(&path)? {
        // Follows symlinks: a linked album is browsed like any other.
        let Ok(meta) = std::fs::metadata(&p) else {
            continue;
        };
        if meta.is_dir() {
            let (audio_files, has_subdirs) = peek(&p);
            dirs.push(DirEntry {
                name,
                path: p,
                audio_files,
                has_subdirs,
            });
        } else if meta.is_file() && is_audio(&p) {
            files.push(FileEntry {
                name,
                path: p,
                bytes: meta.len(),
            });
        }
    }
    dirs.sort_by_key(|d| d.name.to_lowercase());
    files.sort_by_key(|f| f.name.to_lowercase());
    Ok(Listing {
        parent: path.parent().map(Path::to_path_buf),
        path,
        dirs,
        files,
    })
}

/// Visible entries of `dir`, as (name, path).
fn visible(dir: &Path) -> Result<Vec<(String, PathBuf)>> {
    let mut out = Vec::new();
    for ent in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
        let ent = ent.with_context(|| format!("reading {}", dir.display()))?;
        let name = ent.file_name().to_string_lossy().into_owned();
        if !name.starts_with('.') {
            out.push((name, ent.path()));
        }
    }
    Ok(out)
}

/// Audio files directly in `dir`, and whether it has a visible subdirectory.
fn peek(dir: &Path) -> (u32, bool) {
    let Ok(entries) = visible(dir) else {
        return (0, false);
    };
    let mut audio = 0u32;
    let mut subdirs = false;
    for (_, p) in entries {
        match std::fs::metadata(&p) {
            Ok(m) if m.is_dir() => subdirs = true,
            Ok(m) if m.is_file() && is_audio(&p) => audio += 1,
            _ => {}
        }
    }
    (audio, subdirs)
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
            fs::write(full, b"12345").unwrap();
        }
        t
    }

    #[test]
    fn lists_one_level_with_counts() {
        let t = tree(&[
            "Windrose/Wintersaga/01 - a.flac",
            "Windrose/Wintersaga/02 - b.FLAC",
            "Windrose/Wintersaga/cover.jpg",
            "Windrose/notes.txt",
            "Sea of Thieves/01 - Grogmire.wav",
            "Sea of Thieves/.02 - partial.wav",
            "Sea of Thieves/.cache/x.mp3",
            "loose.mp3",
            "readme.txt",
            ".hidden.mp3",
            ".Trash/old.mp3",
        ]);
        fs::create_dir(t.path().join("empty")).unwrap();
        let l = list(t.path()).unwrap();

        assert_eq!(l.path, t.path().canonicalize().unwrap());
        assert_eq!(l.parent.as_deref(), l.path.parent());
        let dirs: Vec<_> = l
            .dirs
            .iter()
            .map(|d| (d.name.as_str(), d.audio_files, d.has_subdirs))
            .collect();
        assert_eq!(
            dirs,
            [
                ("empty", 0, false),
                ("Sea of Thieves", 1, false),
                ("Windrose", 0, true),
            ]
        );
        assert_eq!(l.dirs[1].path, l.path.join("Sea of Thieves"));
        let files: Vec<_> = l.files.iter().map(|f| (f.name.as_str(), f.bytes)).collect();
        assert_eq!(files, [("loose.mp3", 5)]);

        let inner = list(&l.dirs[2].path.join("Wintersaga")).unwrap();
        assert_eq!(inner.files.len(), 2, "extensions match without case");
        assert!(inner.dirs.is_empty());
    }

    #[test]
    fn a_missing_directory_is_an_error() {
        let t = tempfile::tempdir().unwrap();
        assert!(list(&t.path().join("nope")).is_err());
    }

    #[test]
    fn the_listing_writes_nothing() {
        let t = tree(&["A/1.mp3"]);
        let before: Vec<_> = fs::read_dir(t.path().join("A")).unwrap().collect();
        list(t.path()).unwrap();
        let after: Vec<_> = fs::read_dir(t.path().join("A")).unwrap().collect();
        assert_eq!(before.len(), after.len());
    }
}
