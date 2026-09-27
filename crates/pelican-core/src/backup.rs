//! Copy the watch's own files to the computer before a factory reset.
//! Read-only on the device.
//!
//! A factory reset is the only way anything Pelican sent ever leaves the
//! watch, and it erases everything else too: activities, health and sleep
//! data, settings. Before a person does one, [`backup`] copies every file
//! under the watch's `GARMIN` folder (Activity, Monitor, Sleep, HRVStatus,
//! Metrics, Records, Totals, Settings, …) into `dest/GARMIN`, recreating
//! the folder tree. The FIT files in there are what fitness apps import.
//!
//! It uses only [`Backend::list_dir`] and [`Backend::download_file`];
//! there is nothing else it could call that writes, and nothing on the
//! local side is overwritten: every file is created new. It can be stopped
//! between files, and a watch that stops answering ends it with the replug
//! instruction rather than waiting out a timeout per file.

use std::io::Write;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Serialize;

use crate::mtp::{Backend, RemoteEntry};
use crate::transfer::Stop;

/// The folder on the watch that holds its own data.
pub const GARMIN_FOLDER: &str = "GARMIN";

/// A backup as it goes. Serialized as the IPC's `pelican://backup` payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Progress {
    /// Walking the watch's `GARMIN` folder to find every file.
    Listing,
    /// One file copied. `index` counts from 0; `path` is the watch path.
    File {
        index: usize,
        total: usize,
        path: String,
        bytes: u64,
    },
    Finished(Summary),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Summary {
    /// Files copied.
    pub files: usize,
    pub bytes: u64,
    /// The folder the `GARMIN` tree was copied into.
    pub dest: PathBuf,
    /// Files that could not be copied, with why. Empty on a full backup.
    pub failed: Vec<Failed>,
    /// Objects whose name the watch will not give (stubs): nothing to copy.
    pub unreadable: usize,
    /// Stopped before every file was copied.
    pub stopped: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Failed {
    pub path: String,
    pub reason: String,
}

/// One file to copy: where it is on the watch, and where it goes.
struct Item {
    remote: String,
    local: PathBuf,
}

/// Copy every file under the watch's `GARMIN` folder into `dest/GARMIN`.
///
/// `dest` is created if missing. Returns `Err` when the backup as a whole
/// cannot go on: nothing under `GARMIN`, a local folder that cannot be
/// made, or a watch that stopped answering (the error carries
/// [`crate::error::Wedged`]). A single file that will not copy is in
/// [`Summary::failed`]; everything else is still copied.
pub fn backup(
    dev: &mut dyn Backend,
    dest: &Path,
    stop: &Stop,
    progress: &mut dyn FnMut(Progress),
) -> Result<Summary> {
    progress(Progress::Listing);
    let mut items = Vec::new();
    let mut failed = Vec::new();
    let mut unreadable = 0usize;
    walk(
        dev,
        GARMIN_FOLDER,
        &dest.join(GARMIN_FOLDER),
        &mut items,
        &mut failed,
        &mut unreadable,
    )?;
    if items.is_empty() {
        bail!(
            "found no files under /{GARMIN_FOLDER} on the watch, so there is nothing to back up. \
             Nothing was written."
        );
    }

    std::fs::create_dir_all(dest).with_context(|| format!("creating {}", dest.display()))?;
    let total = items.len();
    let (mut files, mut bytes, mut stopped) = (0usize, 0u64, false);
    for (index, item) in items.into_iter().enumerate() {
        if stop.is_requested() {
            stopped = true;
            break;
        }
        let data = match dev.download_file(&item.remote) {
            Ok(d) => d,
            Err(e) if crate::error::is_wedged(&e) => {
                return Err(e.context(crate::error::Wedged).context(format!(
                    "the watch stopped answering after {files} of {total} files; \
                     the copies so far are in {}",
                    dest.display()
                )));
            }
            Err(e) => {
                failed.push(Failed {
                    path: item.remote,
                    reason: format!("{e:#}"),
                });
                continue;
            }
        };
        if let Err(e) = write_new(&item.local, &data) {
            failed.push(Failed {
                path: item.remote,
                reason: format!("{e:#}"),
            });
            continue;
        }
        files += 1;
        bytes += data.len() as u64;
        progress(Progress::File {
            index,
            total,
            path: item.remote,
            bytes: data.len() as u64,
        });
    }
    let summary = Summary {
        files,
        bytes,
        dest: dest.to_path_buf(),
        failed,
        unreadable,
        stopped,
    };
    progress(Progress::Finished(summary.clone()));
    Ok(summary)
}

/// Depth-first, folders in listing order. A name that could climb out of
/// `local` (or is not a plain name at all) is refused, not mapped: it is
/// device-controlled text on its way to becoming a path.
fn walk(
    dev: &mut dyn Backend,
    remote: &str,
    local: &Path,
    items: &mut Vec<Item>,
    failed: &mut Vec<Failed>,
    unreadable: &mut usize,
) -> Result<()> {
    let listing: Vec<RemoteEntry> = dev
        .list_dir(remote)
        .with_context(|| format!("listing /{remote}"))?;
    for e in listing {
        if e.is_broken {
            *unreadable += 1;
            continue;
        }
        let child_remote = format!("{remote}/{}", e.name);
        if !safe_name(&e.name) {
            failed.push(Failed {
                path: crate::garmin::strip_control(&child_remote),
                reason: "the name cannot be used as a file name on this computer".into(),
            });
            continue;
        }
        let child_local = local.join(&e.name);
        if e.is_folder {
            walk(dev, &child_remote, &child_local, items, failed, unreadable)?;
        } else {
            items.push(Item {
                remote: child_remote,
                local: child_local,
            });
        }
    }
    Ok(())
}

fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\'])
        && !name.chars().any(char::is_control)
}

/// Create `path` (and its folders) and write `data`, never replacing a file
/// that is already there.
fn write_new(path: &Path, data: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .with_context(|| format!("creating {}", path.display()))?;
    f.write_all(data)
        .and_then(|()| f.sync_all())
        .with_context(|| format!("writing {}", path.display()))
}

/// `documents/Pelican/<model> backup <date>`, pure. `model` is
/// device-controlled, so anything but a plain name character becomes `-`.
pub fn dir_name(documents: &Path, model: &str, date: &str) -> PathBuf {
    let clean: String = model
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')' | '.') {
                c
            } else {
                '-'
            }
        })
        .collect();
    let clean = clean.trim().trim_matches('.').trim();
    let clean = if clean.is_empty() {
        "Garmin watch"
    } else {
        clean
    };
    documents
        .join("Pelican")
        .join(format!("{clean} backup {date}"))
}

/// Where a backup goes by default: `~/Documents/Pelican/<model> backup
/// <YYYY-MM-DD>` (the XDG documents folder when one is set), with ` (2)`,
/// ` (3)`, … added if that folder already exists. Not created. The date is
/// UTC. `None` without a usable home directory.
pub fn default_dest(model: &str) -> Option<PathBuf> {
    let documents = crate::places::documents_dir()?;
    let today = crate::ledger::rfc3339(std::time::SystemTime::now());
    let base = dir_name(&documents, model, &today[..10]);
    Some(first_free(&base))
}

fn first_free(base: &Path) -> PathBuf {
    if !base.exists() {
        return base.to_path_buf();
    }
    for n in 2u32.. {
        let mut name = base.as_os_str().to_owned();
        name.push(format!(" ({n})"));
        let p = PathBuf::from(name);
        if !p.exists() {
            return p;
        }
    }
    unreachable!("some suffix is free")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtp::fake::{Call, FakeDevice, Faults};

    fn watch() -> FakeDevice {
        let dev = FakeDevice::new();
        dev.add_folder("GARMIN/Activity");
        dev.add_folder("GARMIN/Monitor");
        dev.add_folder("GARMIN/Settings");
        dev.add_file("GARMIN/Activity", "2026-09-26-07-00-00.fit", b"run");
        dev.add_file("GARMIN/Activity", "2026-09-25-07-00-00.fit", b"ride!");
        dev.add_file("GARMIN/Monitor", "M1.fit", b"steps");
        dev.add_file("GARMIN/Settings", "Settings.fit", b"s");
        dev.add_file("GARMIN", "Device.fit", b"d");
        dev.add_file("Music", "pl00001-A.mp3", b"not backed up");
        dev
    }

    #[test]
    fn copies_the_whole_garmin_tree_and_never_writes_to_the_watch() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("FR165 backup 2026-09-26");
        let dev = watch();
        let before = dev.objects().len();
        let mut events = Vec::new();
        let s = backup(dev.backend().as_mut(), &dest, &Stop::new(), &mut |p| {
            events.push(p)
        })
        .unwrap();

        assert_eq!((s.files, s.bytes), (5, 3 + 5 + 5 + 1 + 1));
        assert!(s.failed.is_empty() && !s.stopped);
        let read = |p: &str| std::fs::read(dest.join(p)).unwrap();
        assert_eq!(read("GARMIN/Activity/2026-09-26-07-00-00.fit"), b"run");
        assert_eq!(read("GARMIN/Monitor/M1.fit"), b"steps");
        assert_eq!(read("GARMIN/Device.fit"), b"d");
        assert!(!dest.join("Music").exists(), "only GARMIN is copied");

        // Only reads reached the device, and nothing on it changed.
        assert!(dev
            .calls()
            .iter()
            .all(|c| matches!(c, Call::ListDir(_) | Call::Download(_))));
        assert_eq!(dev.objects().len(), before);

        assert_eq!(events.first(), Some(&Progress::Listing));
        let files: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                Progress::File { index, total, .. } => Some((*index, *total)),
                _ => None,
            })
            .collect();
        assert_eq!(files, [(0, 5), (1, 5), (2, 5), (3, 5), (4, 5)]);
        assert!(matches!(events.last(), Some(Progress::Finished(f)) if f.files == 5));
        let json = serde_json::to_value(events.last().unwrap()).unwrap();
        assert_eq!(json["kind"], "finished");
        assert_eq!(json["files"], 5);
    }

    #[test]
    fn a_file_that_will_not_copy_is_reported_and_the_rest_still_are() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = watch();
        dev.add_stub("GARMIN/Activity");
        dev.set_faults(Faults {
            fail_downloads: 1,
            ..Default::default()
        });
        let s = backup(
            dev.backend().as_mut(),
            tmp.path(),
            &Stop::new(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(s.files, 4);
        assert_eq!(s.failed.len(), 1);
        assert_eq!(s.unreadable, 1);
    }

    #[test]
    fn never_replaces_a_local_file() {
        let tmp = tempfile::tempdir().unwrap();
        let there = tmp.path().join("GARMIN/Device.fit");
        std::fs::create_dir_all(there.parent().unwrap()).unwrap();
        std::fs::write(&there, b"mine").unwrap();
        let s = backup(
            watch().backend().as_mut(),
            tmp.path(),
            &Stop::new(),
            &mut |_| {},
        )
        .unwrap();
        assert_eq!(std::fs::read(&there).unwrap(), b"mine");
        assert_eq!(s.failed.len(), 1);
        assert_eq!(s.files, 4);
    }

    #[test]
    fn a_stop_ends_it_between_files() {
        let tmp = tempfile::tempdir().unwrap();
        let stop = Stop::new();
        let s2 = stop.clone();
        let s = backup(watch().backend().as_mut(), tmp.path(), &stop, &mut |p| {
            if matches!(p, Progress::File { index: 1, .. }) {
                s2.request();
            }
        })
        .unwrap();
        assert!(s.stopped);
        assert_eq!(s.files, 2);
    }

    #[test]
    fn a_watch_that_stops_answering_ends_it_with_the_replug_instruction() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = watch();
        let mut b = dev.backend();
        let mut events = 0;
        let r = backup(b.as_mut(), tmp.path(), &Stop::new(), &mut |p| {
            events += 1;
            if matches!(p, Progress::File { index: 0, .. }) {
                dev.set_faults(Faults {
                    wedged: true,
                    ..Default::default()
                });
            }
        });
        let e = r.unwrap_err();
        assert!(crate::error::is_wedged(&e), "{e:#}");
        assert!(format!("{e:#}").contains(crate::error::REPLUG), "{e:#}");
        // One download after the wedge, not one per remaining file.
        let downloads = dev
            .calls()
            .iter()
            .filter(|c| matches!(c, Call::Download(_)))
            .count();
        assert_eq!(downloads, 2);
    }

    #[test]
    fn an_empty_garmin_folder_is_an_error_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("b");
        let dev = FakeDevice::new();
        assert!(backup(dev.backend().as_mut(), &dest, &Stop::new(), &mut |_| {}).is_err());
        assert!(!dest.exists());
    }

    #[test]
    fn device_names_cannot_climb_out() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("b");
        let dev = FakeDevice::new();
        dev.add_folder("GARMIN");
        dev.add_file("GARMIN", "..", b"x");
        dev.add_file("GARMIN", "a\u{1b}b.fit", b"x");
        dev.add_file("GARMIN", "ok.fit", b"x");
        let s = backup(dev.backend().as_mut(), &dest, &Stop::new(), &mut |_| {}).unwrap();
        assert_eq!(s.files, 1);
        assert_eq!(s.failed.len(), 2);
        assert!(s.failed.iter().all(|f| !f.path.contains('\u{1b}')));
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 1);
    }

    #[test]
    fn default_names() {
        let d = Path::new("/home/six/Documents");
        assert_eq!(
            dir_name(d, "Forerunner 165 Music", "2026-09-26"),
            Path::new("/home/six/Documents/Pelican/Forerunner 165 Music backup 2026-09-26")
        );
        assert_eq!(
            dir_name(d, "../x/y\u{1b}", "2026-09-26"),
            Path::new("/home/six/Documents/Pelican/-x-y- backup 2026-09-26")
        );
        assert_eq!(
            dir_name(d, "", "2026-09-26"),
            Path::new("/home/six/Documents/Pelican/Garmin watch backup 2026-09-26")
        );
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().join("x backup");
        assert_eq!(first_free(&base), base);
        std::fs::create_dir(&base).unwrap();
        assert_eq!(first_free(&base), tmp.path().join("x backup (2)"));
    }
}
