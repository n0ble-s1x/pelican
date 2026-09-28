//! What is on the watch right now. Read-only.
//!
//! `pelican status` and `pelican ls` are both one [`read`]: a single
//! session, one `/Music` listing, one storage query. A front-end asks the
//! same questions, so they are answered here rather than in the CLI, and
//! [`audio_objects`] is the one count that both the status line and the
//! push's capacity check use, so the two numbers cannot disagree.

use std::path::Path;

use anyhow::{Context, Result};
use serde::Serialize;

use crate::garmin::{Device, MUSIC_FOLDER};
use crate::ledger::Ledger;
use crate::mtp::{Backend, RemoteEntry};

/// Garmin's documented ceiling on audio files in the music library.
pub const MAX_OBJECTS: usize = 500;

/// How many of `listing`'s entries count toward [`MAX_OBJECTS`].
///
/// The limit is on audio, so a readable `.m3u8` or `.txt` does not count.
/// A stub does: it is an object the library still holds, and whether it is
/// audio cannot be told from a name nobody can read.
pub fn audio_objects(listing: &[RemoteEntry]) -> usize {
    listing
        .iter()
        .filter(|e| !e.is_folder && (e.is_broken || crate::transcode::is_audio(Path::new(&e.name))))
        .count()
}

/// The watch as one read found it.
#[derive(Debug, Clone)]
pub struct Snapshot {
    /// The model the watch reports over MTP, else the USB label.
    pub model: String,
    /// USB serial, control bytes stripped. `None` means no ledger can be
    /// kept for this watch, so nothing can be pushed to it.
    pub serial: Option<String>,
    pub free: u64,
    pub capacity: u64,
    /// `/Music`, as listed. Names are device-controlled text.
    pub music: Vec<RemoteEntry>,
}

/// In a session the caller opened: one free-space query, one `/Music`
/// listing. Writes nothing and creates no folder.
pub fn read(dev: &mut dyn Backend, device: &Device) -> Result<Snapshot> {
    let model = dev.model().unwrap_or_else(|| device.label());
    let (free, capacity) = dev.free_space().context("reading free space")?;
    let music = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    Ok(Snapshot {
        model,
        serial: device.serial.as_deref().map(crate::garmin::strip_control),
        free,
        capacity,
        music,
    })
}

/// The numbers `pelican status` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Counts {
    /// Toward [`MAX_OBJECTS`]; see [`audio_objects`].
    pub audio_objects: usize,
    pub max_objects: usize,
    /// Objects whose metadata cannot be read: the wreckage of an earlier
    /// failed write. They hold a name nobody can see.
    pub stubs: usize,
}

/// Who put an object in `/Music`, as far as this machine can tell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Origin {
    /// This machine's ledger has the name.
    Ledger,
    /// Readable, and not in the ledger: Garmin Express, another tool, or
    /// Pelican on another machine.
    Foreign,
    /// Unreadable; its real name is unknown.
    Stub,
}

/// One `/Music` entry, ready to show.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Row {
    /// Control bytes stripped. A stub's is synthetic (`‹unreadable #N›`).
    pub name: String,
    /// `None` for a stub or a folder: there is no size to trust.
    pub size: Option<u64>,
    pub is_folder: bool,
    pub origin: Origin,
    pub handle: u32,
}

impl Snapshot {
    pub fn counts(&self) -> Counts {
        Counts {
            audio_objects: audio_objects(&self.music),
            max_objects: MAX_OBJECTS,
            stubs: self.music.iter().filter(|e| e.is_broken).count(),
        }
    }

    /// `/Music` in listing order, each entry marked with its [`Origin`].
    /// Without a ledger nothing can be claimed as Pelican's.
    pub fn rows(&self, ledger: Option<&Ledger>) -> Vec<Row> {
        self.music
            .iter()
            .map(|e| {
                let origin = if e.is_broken {
                    Origin::Stub
                } else if ledger.is_some_and(|l| l.has_name(&e.name)) {
                    Origin::Ledger
                } else {
                    Origin::Foreign
                };
                Row {
                    name: crate::garmin::strip_control(&e.name),
                    size: (!e.is_broken && !e.is_folder).then_some(e.size),
                    is_folder: e.is_folder,
                    origin,
                    handle: e.handle,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ledger::{Event, Kind};
    use crate::mtp::fake::FakeDevice;

    fn device() -> Device {
        Device {
            vendor_id: 0x091e,
            product_id: 0x4f0b,
            serial: Some("12\u{1b}34".into()),
            product: None,
        }
    }

    #[test]
    fn one_read_answers_status_and_ls() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.set_space(900, 1000);
        dev.add_file("Music", "PL00001-One.mp3", &[0; 2048]);
        dev.add_file("Music", "Garmin Express.m4a", b"x");
        dev.add_file("Music", "notes.txt", b"x");
        dev.add_file("Music", "evil\u{1b}[2J.mp3", b"x");
        dev.add_stub("Music");
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        l.append(Event::new(Kind::Reserve, 1, "pl00001-one.mp3"))
            .unwrap();

        let snap = read(dev.backend().as_mut(), &device()).unwrap();
        assert_eq!((snap.free, snap.capacity), (900, 1000));
        assert_eq!(snap.serial.as_deref(), Some("1234"));
        assert_eq!(
            snap.counts(),
            Counts {
                audio_objects: 4,
                max_objects: MAX_OBJECTS,
                stubs: 1
            }
        );

        let rows = snap.rows(Some(&l));
        let find = |needle: &str| rows.iter().find(|r| r.name.contains(needle)).unwrap();
        // The ledger's lowercase name matches the device's mixed case.
        assert_eq!(find("PL00001").origin, Origin::Ledger);
        assert_eq!(find("PL00001").size, Some(2048));
        assert_eq!(find("Garmin Express").origin, Origin::Foreign);
        let stub = find("‹unreadable");
        assert_eq!((stub.origin, stub.size), (Origin::Stub, None));
        assert!(rows.iter().all(|r| !r.name.contains('\u{1b}')), "{rows:?}");

        // No ledger: nothing is Pelican's.
        assert!(snap.rows(None).iter().all(|r| r.origin != Origin::Ledger));
    }
}
