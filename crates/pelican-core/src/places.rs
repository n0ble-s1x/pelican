//! Where music usually lives on this computer, so nobody has to type a
//! path: home, the Music folder, the configured library, network shares
//! (NFS, SMB/CIFS, SSHFS) and removable drives. Read-only.
//!
//! The parsing is pure and tested on fixture text; [`places`] feeds it the
//! live `/proc/self/mounts`, `/etc/fstab` and `user-dirs.dirs`.
//!
//! A mount point comes straight from the kernel's mount table, so it is not
//! `stat`ed to check it exists: it does, and touching an NFS mount whose
//! server is gone can hang. The home, Music and library folders are checked.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PlaceKind {
    Home,
    Music,
    Network,
    Drive,
    Library,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
    pub kind: PlaceKind,
}

/// Filesystem types that are a share on another machine.
const NETWORK_FS: &[&str] = &[
    "nfs",
    "nfs4",
    "cifs",
    "smb3",
    "smbfs",
    "sshfs",
    "fuse.sshfs",
];

fn is_network(fstype: &str) -> bool {
    NETWORK_FS.contains(&fstype)
}

/// Undo the kernel's octal escapes in a mount table field (`\040` is a
/// space, `\011` a tab, `\012` a newline, `\134` a backslash).
pub fn unescape(field: &str) -> String {
    let b = field.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\'
            && i + 3 < b.len()
            && b[i + 1..i + 4].iter().all(|c| (b'0'..=b'7').contains(c))
        {
            let v = (b[i + 1] - b'0') * 64 + (b[i + 2] - b'0') * 8 + (b[i + 3] - b'0');
            out.push(v);
            i += 4;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// `(mount point, fstype)` for every line of a mounts or fstab table,
/// comments and blank lines skipped.
fn table(text: &str) -> impl Iterator<Item = (String, String)> + '_ {
    text.lines().filter_map(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut f = line.split_whitespace();
        let _source = f.next()?;
        let point = unescape(f.next()?);
        let fstype = f.next()?.to_string();
        Some((point, fstype))
    })
}

/// Network shares and removable drives from a mount table.
///
/// - A network filesystem ([`NETWORK_FS`]) anywhere is a share.
/// - An `autofs` trigger is a share when `fstab` says what mounts there is
///   a network filesystem — a systemd automount (`x-systemd.automount`) of
///   a NAS shows as `autofs` until first touched.
/// - Anything mounted under `/run/media/<user>/` or `/media/<user>/` is a
///   removable drive.
///
/// Deduplicated by mount point, first seen wins; mount table order.
pub fn parse_mounts(mounts: &str, fstab: &str, user: &str) -> Vec<Place> {
    let planned: HashMap<String, String> = table(fstab).collect();
    let drives = [format!("/run/media/{user}/"), format!("/media/{user}/")];
    let mut out: Vec<Place> = Vec::new();
    for (point, fstype) in table(mounts) {
        let kind = if is_network(&fstype)
            || (fstype == "autofs" && planned.get(&point).is_some_and(|t| is_network(t)))
        {
            PlaceKind::Network
        } else if !user.is_empty() && drives.iter().any(|d| point.starts_with(d.as_str())) {
            PlaceKind::Drive
        } else {
            continue;
        };
        if out.iter().any(|p| p.path == Path::new(&point)) {
            continue;
        }
        out.push(Place {
            label: label_of(Path::new(&point)),
            path: PathBuf::from(point),
            kind,
        });
    }
    out
}

fn label_of(p: &Path) -> String {
    p.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| p.display().to_string())
}

/// The `XDG_*_DIR` entries of a `user-dirs.dirs` file, keyed without the
/// `XDG_` and `_DIR` (`MUSIC`, `DOCUMENTS`, …). Only `"$HOME/…"` and
/// absolute values are taken, as the spec allows.
pub fn parse_user_dirs(text: &str, home: &Path) -> HashMap<String, PathBuf> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let Some(key) = k
            .trim()
            .strip_prefix("XDG_")
            .and_then(|k| k.strip_suffix("_DIR"))
        else {
            continue;
        };
        let v = v.trim().trim_matches('"');
        let path = if let Some(rest) = v.strip_prefix("$HOME") {
            let rest = rest.trim_start_matches('/');
            if rest.is_empty() {
                home.to_path_buf()
            } else {
                home.join(rest)
            }
        } else if v.starts_with('/') {
            PathBuf::from(v)
        } else {
            continue;
        };
        out.insert(key.to_string(), path);
    }
    out
}

fn home() -> Option<PathBuf> {
    let h = PathBuf::from(std::env::var_os("HOME")?);
    h.is_absolute().then_some(h)
}

/// A folder from the user's `user-dirs.dirs` (`MUSIC`, `DOCUMENTS`, …).
pub fn user_dir(key: &str) -> Option<PathBuf> {
    let home = home()?;
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let text = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    parse_user_dirs(&text, &home).remove(key)
}

/// The documents folder: `XDG_DOCUMENTS_DIR`, else `~/Documents`.
pub fn documents_dir() -> Option<PathBuf> {
    user_dir("DOCUMENTS").or_else(|| Some(home()?.join("Documents")))
}

/// The music folder: `XDG_MUSIC_DIR`, else `~/Music`.
pub fn music_dir() -> Option<PathBuf> {
    user_dir("MUSIC").or_else(|| Some(home()?.join("Music")))
}

/// Every place, in the order a sidebar shows them: Home, Music, the
/// library root (when one is configured), then network shares, then
/// drives. Existing folders only; each path once.
pub fn places(library_root: Option<&Path>) -> Vec<Place> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").unwrap_or_default();
    let fstab = std::fs::read_to_string("/etc/fstab").unwrap_or_default();
    let user = std::env::var("USER").unwrap_or_default();
    let mut fixed = Vec::new();
    if let Some(h) = home() {
        fixed.push(Place {
            label: "Home".into(),
            path: h,
            kind: PlaceKind::Home,
        });
    }
    if let Some(m) = music_dir() {
        fixed.push(Place {
            label: "Music".into(),
            path: m,
            kind: PlaceKind::Music,
        });
    }
    if let Some(l) = library_root {
        fixed.push(Place {
            label: "Library".into(),
            path: l.to_path_buf(),
            kind: PlaceKind::Library,
        });
    }
    let fixed: Vec<Place> = fixed.into_iter().filter(|p| p.path.is_dir()).collect();
    merge(fixed, parse_mounts(&mounts, &fstab, &user))
}

/// `fixed` then `mounted` (shares before drives), each path once — the
/// first place to claim a path keeps it.
fn merge(fixed: Vec<Place>, mounted: Vec<Place>) -> Vec<Place> {
    let (net, drives): (Vec<Place>, Vec<Place>) = mounted
        .into_iter()
        .partition(|p| p.kind == PlaceKind::Network);
    let mut out: Vec<Place> = Vec::new();
    for p in fixed.into_iter().chain(net).chain(drives) {
        if !out.iter().any(|q| q.path == p.path) {
            out.push(p);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const MOUNTS: &str = "\
/dev/nvme0n1p2 / btrfs rw,relatime 0 0
proc /proc proc rw 0 0
systemd-1 /mnt/nas autofs rw,relatime,fd=41 0 0
systemd-1 /mnt/Backups autofs rw,relatime,fd=42 0 0
192.0.2.10:/export/music /mnt/nas nfs4 rw,vers=4.2 0 0
//nas/music /mnt/smb\\040music cifs rw 0 0
six@box:/srv /home/six/box fuse.sshfs rw 0 0
/dev/sdb1 /run/media/six/WALKMAN\\040SD vfat rw 0 0
/dev/sdc1 /run/media/other/THEIRS vfat rw 0 0
/dev/sdd1 /media/six/USB exfat rw 0 0
sunrpc /var/lib/nfs/rpc_pipefs rpc_pipefs rw 0 0
";

    const FSTAB: &str = "\
# comment
UUID=abc / btrfs defaults 0 0
192.0.2.10:/export/music\t/mnt/nas\tnfs\t_netdev,noauto,x-systemd.automount 0 0
UUID=def /mnt/Backups ext4 noauto,x-systemd.automount 0 0
";

    #[test]
    fn shares_and_drives_from_the_mount_table() {
        let got = parse_mounts(MOUNTS, FSTAB, "six");
        let pairs: Vec<(&str, &str, PlaceKind)> = got
            .iter()
            .map(|p| (p.label.as_str(), p.path.to_str().unwrap(), p.kind))
            .collect();
        assert_eq!(
            pairs,
            [
                // The autofs trigger, and the nfs mount over it, are one place.
                ("nas", "/mnt/nas", PlaceKind::Network),
                ("smb music", "/mnt/smb music", PlaceKind::Network),
                ("box", "/home/six/box", PlaceKind::Network),
                ("WALKMAN SD", "/run/media/six/WALKMAN SD", PlaceKind::Drive),
                ("USB", "/media/six/USB", PlaceKind::Drive),
            ]
        );
    }

    #[test]
    fn a_local_automount_and_another_users_drive_are_not_places() {
        let got = parse_mounts(MOUNTS, FSTAB, "six");
        assert!(got.iter().all(|p| p.path != Path::new("/mnt/Backups")));
        assert!(got.iter().all(|p| !p.path.starts_with("/run/media/other")));
        // No user, no drives.
        assert!(parse_mounts(MOUNTS, FSTAB, "")
            .iter()
            .all(|p| p.kind == PlaceKind::Network));
    }

    #[test]
    fn octal_escapes() {
        assert_eq!(unescape(r"/a\040b\011c\134d"), "/a b\tc\\d");
        assert_eq!(unescape(r"/trailing\04"), r"/trailing\04");
        assert_eq!(unescape(r"/no\999"), r"/no\999");
    }

    #[test]
    fn user_dirs() {
        let text = r#"
# written by xdg-user-dirs-update
XDG_MUSIC_DIR="$HOME/Music"
XDG_DOCUMENTS_DIR="/data/docs"
XDG_DESKTOP_DIR="$HOME/"
XDG_BAD_DIR="relative/path"
NOT_XDG="x"
"#;
        let d = parse_user_dirs(text, Path::new("/home/six"));
        assert_eq!(d["MUSIC"], Path::new("/home/six/Music"));
        assert_eq!(d["DOCUMENTS"], Path::new("/data/docs"));
        assert_eq!(d["DESKTOP"], Path::new("/home/six"));
        assert!(!d.contains_key("BAD"));
        assert_eq!(d.len(), 3);
    }

    #[test]
    fn merged_in_sidebar_order_each_path_once() {
        let place = |label: &str, path: &str, kind| Place {
            label: label.into(),
            path: path.into(),
            kind,
        };
        let fixed = vec![
            place("Home", "/home/six", PlaceKind::Home),
            place("Music", "/home/six/Music", PlaceKind::Music),
            place("Library", "/mnt/nas", PlaceKind::Library),
        ];
        let mounted = vec![
            place("USB", "/media/six/USB", PlaceKind::Drive),
            place("nas", "/mnt/nas", PlaceKind::Network),
            place("box", "/home/six/box", PlaceKind::Network),
        ];
        let kinds: Vec<PlaceKind> = merge(fixed, mounted).iter().map(|p| p.kind).collect();
        assert_eq!(
            kinds,
            [
                PlaceKind::Home,
                PlaceKind::Music,
                PlaceKind::Library,
                PlaceKind::Network,
                PlaceKind::Drive
            ]
        );
    }

    #[test]
    fn serializes_as_the_ipc_shape() {
        let p = Place {
            label: "nas".into(),
            path: "/mnt/nas".into(),
            kind: PlaceKind::Network,
        };
        assert_eq!(
            serde_json::to_string(&p).unwrap(),
            r#"{"label":"nas","path":"/mnt/nas","kind":"network"}"#
        );
    }
}
