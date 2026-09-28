//! Names the owner gives their devices ("Mav's 165").
//!
//! `$XDG_DATA_HOME/pelican/devices.jsonl`, next to the ledgers, one JSON
//! object per line, append-only like the ledger:
//!
//! ```json
//! {"v":1,"at":"2026-09-27T20:00:00Z","event":"name","serial":"0000abcd1234","name":"Mav's 165","model":"Forerunner 165 Music"}
//! ```
//!
//! - A device is keyed by its USB serial, the same key as its ledger
//!   (`ledger-<serial>.jsonl`), so a name always points at one ledger and
//!   an existing ledger needs no migration: naming a watch adds a line here
//!   and touches nothing else.
//! - The latest `name` line for a serial wins; renaming appends another.
//!   Nothing is rewritten or removed.
//! - Each line is fsync'd before the call returns. An unparsable line is a
//!   hard error naming the file and line, as in the ledger.
//! - A name is only a label. Nothing is decided by it: which names are free
//!   on the watch is still the ledger's business alone.

use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use anyhow::{anyhow, bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::ledger::{rfc3339, Ledger};

const FILE: &str = "devices.jsonl";
const VERSION: u32 = 1;

/// The longest name, in characters. Long enough for "BabyCow's Garmin
/// Fenix 9 Pro Solar", short enough to title a window.
pub const MAX_NAME: usize = 40;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Line {
    v: u32,
    at: String,
    event: Kind,
    serial: String,
    name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    model: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Name,
}

/// One named device, as the latest line for its serial says.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Named {
    pub serial: String,
    pub name: String,
    /// The model it reported when last named, if any.
    pub model: Option<String>,
    /// When it was first named, and when last.
    pub named_at: String,
    pub renamed_at: Option<String>,
}

/// Every name in `devices.jsonl`, read once.
#[derive(Debug, Default)]
pub struct Registry {
    by_serial: BTreeMap<String, Named>,
}

impl Registry {
    pub fn path_in(dir: &Path) -> PathBuf {
        dir.join(FILE)
    }

    /// Read the registry. A missing file is an empty registry.
    pub fn read(dir: &Path) -> Result<Self> {
        let path = Self::path_in(dir);
        let mut file = match File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).with_context(|| format!("opening {}", path.display())),
        };
        let mut raw = Vec::new();
        file.read_to_end(&mut raw)
            .with_context(|| format!("reading {}", path.display()))?;
        let mut reg = Self::default();
        for line in parse(&path, &raw)? {
            reg.apply(line);
        }
        Ok(reg)
    }

    fn apply(&mut self, l: Line) {
        match self.by_serial.get_mut(&l.serial) {
            Some(n) => {
                n.name = l.name;
                if l.model.is_some() {
                    n.model = l.model;
                }
                n.renamed_at = Some(l.at);
            }
            None => {
                self.by_serial.insert(
                    l.serial.clone(),
                    Named {
                        serial: l.serial,
                        name: l.name,
                        model: l.model,
                        named_at: l.at,
                        renamed_at: None,
                    },
                );
            }
        }
    }

    pub fn get(&self, serial: &str) -> Option<&Named> {
        self.by_serial.get(serial)
    }

    pub fn name_of(&self, serial: &str) -> Option<&str> {
        self.get(serial).map(|n| n.name.as_str())
    }

    /// Every named device, ordered by serial.
    pub fn all(&self) -> impl Iterator<Item = &Named> {
        self.by_serial.values()
    }
}

/// Name (or rename) the device with this serial. The name is trimmed and
/// checked with [`check_name`]; the line is appended and fsync'd before
/// this returns. Naming a device the name it already has writes nothing.
pub fn set_name(dir: &Path, serial: &str, model: Option<&str>, name: &str) -> Result<Named> {
    // The same rule the ledger's file name uses, so a named serial always
    // has a ledger it can point at.
    Ledger::path_in(dir, serial)?;
    let name = check_name(name)?;
    let current = Registry::read(dir)?;
    if let Some(n) = current.get(serial) {
        if n.name == name {
            return Ok(n.clone());
        }
    }
    let line = Line {
        v: VERSION,
        at: rfc3339(SystemTime::now()),
        event: Kind::Name,
        serial: serial.to_string(),
        name,
        model: model.map(crate::garmin::strip_control),
    };
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    let path = Registry::path_in(dir);
    let existed = path.exists();
    let mut file = OpenOptions::new()
        .append(true)
        .create(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    let mut text = serde_json::to_string(&line).context("encoding a device name")?;
    text.push('\n');
    file.write_all(text.as_bytes())
        .and_then(|()| file.sync_data())
        .with_context(|| format!("writing to {}", path.display()))?;
    if !existed {
        if let Err(e) = File::open(dir).and_then(|d| d.sync_all()) {
            tracing::warn!(dir = %dir.display(), error = %e, "could not sync the data directory");
        }
    }
    let mut reg = current;
    reg.apply(line);
    Ok(reg.get(serial).cloned().expect("just applied"))
}

/// A name as it will be stored: trimmed, not empty, at most [`MAX_NAME`]
/// characters, and no control characters (it is shown in a window title
/// and printed to a terminal).
pub fn check_name(name: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        bail!("a device name cannot be empty");
    }
    if name.chars().any(char::is_control) {
        bail!("a device name cannot contain control characters");
    }
    let n = name.chars().count();
    if n > MAX_NAME {
        bail!("a device name can be at most {MAX_NAME} characters; this one is {n}");
    }
    Ok(name.to_string())
}

fn parse(path: &Path, raw: &[u8]) -> Result<Vec<Line>> {
    let corrupt = |n: usize, why: String| {
        anyhow!(
            "{} is damaged at line {n}: {why}. Repair that line by hand, or remove it and \
             name the device again.",
            path.display()
        )
    };
    if raw.is_empty() {
        return Ok(Vec::new());
    }
    let mut lines: Vec<&[u8]> = raw.split(|&b| b == b'\n').collect();
    if lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    } else {
        return Err(corrupt(lines.len(), "the line is incomplete".into()));
    }
    let mut out = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        let n = i + 1;
        let l: Line = serde_json::from_slice(line).map_err(|e| corrupt(n, e.to_string()))?;
        if l.v != VERSION {
            return Err(corrupt(
                n,
                format!("format v{} (this Pelican reads v{VERSION})", l.v),
            ));
        }
        out.push(l);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_file_is_an_empty_registry() {
        let tmp = tempfile::tempdir().unwrap();
        let reg = Registry::read(tmp.path()).unwrap();
        assert_eq!(reg.all().count(), 0);
        assert!(!Registry::path_in(tmp.path()).exists());
    }

    #[test]
    fn naming_then_renaming_appends_and_the_latest_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        let first = set_name(
            d,
            "0000abcd1234",
            Some("Forerunner 165 Music"),
            "  Mav's 165 ",
        )
        .unwrap();
        assert_eq!(first.name, "Mav's 165");
        assert_eq!(first.renamed_at, None);
        let again = set_name(d, "0000abcd1234", None, "Mav's run watch").unwrap();
        assert_eq!(again.name, "Mav's run watch");
        assert_eq!(
            again.model.as_deref(),
            Some("Forerunner 165 Music"),
            "a rename keeps the model"
        );
        assert_eq!(again.named_at, first.named_at);

        let reg = Registry::read(d).unwrap();
        assert_eq!(reg.name_of("0000abcd1234"), Some("Mav's run watch"));
        let text = std::fs::read_to_string(Registry::path_in(d)).unwrap();
        assert_eq!(text.lines().count(), 2, "append-only: {text}");
    }

    #[test]
    fn the_same_name_again_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        set_name(tmp.path(), "1", None, "Mav's 165").unwrap();
        set_name(tmp.path(), "1", None, "Mav's 165").unwrap();
        let text = std::fs::read_to_string(Registry::path_in(tmp.path())).unwrap();
        assert_eq!(text.lines().count(), 1);
    }

    #[test]
    fn two_devices_keep_their_own_names() {
        let tmp = tempfile::tempdir().unwrap();
        set_name(tmp.path(), "aaa", Some("Forerunner 165 Music"), "Mav's 165").unwrap();
        set_name(tmp.path(), "bbb", Some("fenix 8"), "BabyCow's Fenix").unwrap();
        let reg = Registry::read(tmp.path()).unwrap();
        assert_eq!(reg.name_of("aaa"), Some("Mav's 165"));
        assert_eq!(reg.name_of("bbb"), Some("BabyCow's Fenix"));
        assert_eq!(reg.all().count(), 2);
    }

    #[test]
    fn naming_touches_no_ledger() {
        let tmp = tempfile::tempdir().unwrap();
        set_name(tmp.path(), "aaa", None, "Mav's 165").unwrap();
        let files: Vec<_> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        assert_eq!(files, vec![FILE.to_string()]);
    }

    #[test]
    fn bad_names_are_refused() {
        assert!(check_name("   ").is_err());
        assert!(check_name("Mav\u{1b}[31m").is_err());
        assert!(check_name(&"x".repeat(MAX_NAME + 1)).is_err());
        assert_eq!(
            check_name(&"é".repeat(MAX_NAME)).unwrap().chars().count(),
            MAX_NAME
        );
    }

    #[test]
    fn a_serial_that_cannot_key_a_ledger_is_refused() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(set_name(tmp.path(), "../x", None, "Mav's 165").is_err());
        assert!(!Registry::path_in(tmp.path()).exists());
    }

    #[test]
    fn a_damaged_line_is_a_hard_error_naming_file_and_line() {
        let tmp = tempfile::tempdir().unwrap();
        set_name(tmp.path(), "aaa", None, "Mav's 165").unwrap();
        let p = Registry::path_in(tmp.path());
        let mut text = std::fs::read_to_string(&p).unwrap();
        text.push_str("{not json}\n");
        std::fs::write(&p, text).unwrap();
        let msg = format!("{:#}", Registry::read(tmp.path()).unwrap_err());
        assert!(
            msg.contains("devices.jsonl") && msg.contains("line 2"),
            "{msg}"
        );
    }

    #[test]
    fn a_cut_off_last_line_is_a_hard_error() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            Registry::path_in(tmp.path()),
            r#"{"v":1,"at":"x","event":"name","serial":"a","name":"b"}"#,
        )
        .unwrap();
        assert!(Registry::read(tmp.path()).is_err());
    }
}
