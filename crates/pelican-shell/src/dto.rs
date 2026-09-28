//! The IPC contract's shapes, and the mapping from the core's types onto
//! them. Pure: nothing here touches a device, a window or the disk, so all
//! of it is tested without either.
//!
//! Paths cross as strings. A path that is not UTF-8 cannot be named back
//! through JSON, so a listing leaves it out, and a display-only field
//! (a preview row, a progress event) shows it lossily.

use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use pelican_core::error::{self as core_error, DeviceErrorKind};
use pelican_core::ledger::{self, Ledger};
use pelican_core::mtp::fold_name;
use pelican_core::places::{Place, PlaceKind};
use pelican_core::transcode::tags::{Mix, Overrides};
use pelican_core::transfer::{Outcome, Progress, Resend, Skip, Tally};
use pelican_core::watch::{Origin, Row};
use pelican_core::{backup, library, preview};

/// The push's event channel. Every payload carries its run's `run_id`.
pub const PROGRESS_EVENT: &str = "pelican://progress";

/// The backup's event channel, shaped the same way.
pub const BACKUP_EVENT: &str = "pelican://backup";

// ── requests ─────────────────────────────────────────────────────────────

/// `preview` and `push` take the same request, so what was reviewed is what
/// is sent.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlanRequest {
    pub paths: Vec<String>,
    #[serde(default)]
    pub overrides: OverridesDto,
    #[serde(default)]
    pub mix: Option<Mix>,
    #[serde(default)]
    pub resend: bool,
    /// Send these again although the ledger has them on the watch: the
    /// per-track "Send again" in review. Each is sent under a fresh name.
    #[serde(default)]
    pub resend_sources: Vec<String>,
}

impl PlanRequest {
    /// The run-wide toggle and the per-track list, as the core takes them.
    pub fn resend(&self) -> Resend {
        Resend::new(
            self.resend,
            self.resend_sources.iter().map(std::path::PathBuf::from),
        )
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverridesDto {
    pub artist: Option<String>,
    pub album: Option<String>,
    pub genre: Option<String>,
    pub year: Option<String>,
}

impl OverridesDto {
    /// A blank field is no override: an empty text box means "leave it".
    pub fn to_core(&self) -> Overrides {
        let keep = |v: &Option<String>| {
            v.as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        Overrides {
            artist: keep(&self.artist),
            album: keep(&self.album),
            genre: keep(&self.genre),
            year: keep(&self.year),
        }
    }
}

// ── status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LedgerTotals {
    pub verified: u32,
    pub failed: u32,
    pub unresolved: u32,
    /// Factory resets recorded; the other totals count since the last one.
    pub resets: u32,
}

impl From<ledger::Totals> for LedgerTotals {
    fn from(t: ledger::Totals) -> Self {
        Self {
            verified: t.verified as u32,
            failed: t.failed as u32,
            unresolved: t.unresolved as u32,
            resets: t.resets as u32,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    pub connected: bool,
    /// The name the owner gave this watch on this computer, if any.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub serial: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capacity_bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub music_objects: Option<u32>,
    pub max_objects: u32,
    pub ledger: LedgerTotals,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gvfs_warning: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    /// Why the watch could not be read; absent when connected. `wedged`
    /// means it stopped answering, and `error` is then the replug
    /// instruction word for word.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<DeviceErrorKind>,
}

impl Status {
    pub fn disconnected(
        error: String,
        kind: DeviceErrorKind,
        gvfs_warning: Option<String>,
    ) -> Self {
        Self {
            connected: false,
            max_objects: pelican_core::watch::MAX_OBJECTS as u32,
            gvfs_warning,
            error: Some(error),
            error_kind: Some(kind),
            ..Self::default()
        }
    }

    /// The status for a failed read: the error sorted into its kind (a
    /// busy watch that gvfs holds is `gvfs`), with its fix as the text.
    pub fn from_error(e: &anyhow::Error, gvfs_warning: Option<String>) -> Self {
        let kind = core_error::classify_with(e, gvfs_warning.is_some());
        Self::disconnected(explain_device_error(e), kind, gvfs_warning)
    }
}

/// The fix, for an error that stops the watch being read.
///
/// The core's own text already names the cable and MTP mode for "no watch
/// found", and the gvfs remedy for contention. What it cannot know is that
/// a permission failure on Linux almost always means the udev rule is not
/// installed, so that is added here.
///
/// A watch that stopped answering gets the replug instruction and nothing
/// else: the timeout underneath means nothing to the person who has to
/// pull the cable.
pub fn explain_device_error(e: &anyhow::Error) -> String {
    if core_error::is_wedged(e) {
        return core_error::REPLUG.to_string();
    }
    let msg = format!("{e:#}");
    let m = msg.to_ascii_lowercase();
    if m.contains("permission denied") || m.contains("access denied") || m.contains("eacces") {
        format!(
            "{msg}. Pelican cannot open the watch without the udev rule: copy \
             udev/70-garmin-mtp.rules to /etc/udev/rules.d/, run \
             `sudo udevadm control --reload`, then unplug and replug the watch."
        )
    } else {
        msg
    }
}

// ── library ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirDto {
    pub name: String,
    pub path: String,
    pub audio_files: u32,
    pub has_subdirs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FileDto {
    pub name: String,
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ListingDto {
    pub path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub dirs: Vec<DirDto>,
    pub files: Vec<FileDto>,
}

fn utf8(p: &Path) -> Option<String> {
    p.to_str().map(str::to_string)
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

impl ListingDto {
    /// Entries whose path is not UTF-8 are left out: the UI could show
    /// them, but it could never hand them back to be sent.
    pub fn from_core(l: library::Listing) -> Self {
        Self {
            path: lossy(&l.path),
            parent: l.parent.as_deref().and_then(utf8),
            dirs: l
                .dirs
                .into_iter()
                .filter_map(|d| {
                    Some(DirDto {
                        path: utf8(&d.path)?,
                        name: d.name,
                        audio_files: d.audio_files,
                        has_subdirs: d.has_subdirs,
                    })
                })
                .collect(),
            files: l
                .files
                .into_iter()
                .filter_map(|f| {
                    Some(FileDto {
                        path: utf8(&f.path)?,
                        name: f.name,
                        bytes: f.bytes,
                    })
                })
                .collect(),
        }
    }
}

// ── places ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlaceDto {
    pub label: String,
    pub path: String,
    pub kind: PlaceKind,
}

/// A place whose path is not UTF-8 is left out, as in a listing: it could
/// not be handed back to `library_list`.
pub fn places(p: Vec<Place>) -> Vec<PlaceDto> {
    p.into_iter()
        .filter_map(|p| {
            Some(PlaceDto {
                path: utf8(&p.path)?,
                label: p.label,
                kind: p.kind,
            })
        })
        .collect()
}

// ── preview ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewFileDto {
    pub source: String,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub year: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genre: Option<String>,
    pub source_bytes: u64,
    /// An estimate; the exact size is known only after the transcode.
    pub est_bytes: u64,
    pub verdict: preview::Decision,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PreviewDto {
    pub files: Vec<PreviewFileDto>,
    pub totals: preview::Totals,
    pub fits: preview::Fits,
}

impl PreviewDto {
    pub fn from_core(p: preview::Preview) -> Self {
        Self {
            files: p
                .files
                .into_iter()
                .map(|f| PreviewFileDto {
                    source: lossy(&f.source),
                    title: f.title,
                    artist: f.artist,
                    album: f.album,
                    track: f.track,
                    year: f.year,
                    genre: f.genre,
                    source_bytes: f.source_bytes,
                    est_bytes: f.est_bytes,
                    verdict: f.verdict,
                    reason: f.reason,
                })
                .collect(),
            totals: p.totals,
            fits: p.fits,
        }
    }
}

// ── push ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PushStarted {
    pub run_id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DoneKind {
    Verified,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Payload {
    Transcoding {
        index: usize,
        total: usize,
        source: String,
    },
    Sending {
        index: usize,
        total: usize,
        source: String,
        remote: String,
    },
    Uploading {
        index: usize,
        bytes: u64,
        total_bytes: u64,
    },
    Done {
        index: usize,
        source: String,
        outcome: DoneKind,
        #[serde(skip_serializing_if = "Option::is_none")]
        remote: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sha256: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Finished {
        verified: usize,
        skipped: usize,
        failed: usize,
        stopped: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProgressEvent {
    pub run_id: String,
    #[serde(flatten)]
    pub payload: Payload,
}

impl From<Tally> for Payload {
    fn from(t: Tally) -> Self {
        Payload::Finished {
            verified: t.verified,
            skipped: t.skipped,
            failed: t.failed,
            stopped: t.stopped,
        }
    }
}

/// The core's event as the contract's, or `None` for the two the contract
/// leaves out (`connecting`, `attempt_failed`: a failed attempt's final word
/// is its file's `done`).
pub fn map_progress(p: Progress) -> Option<Payload> {
    Some(match p {
        Progress::Transcoding {
            index, of, source, ..
        } => Payload::Transcoding {
            index,
            total: of,
            source: lossy(&source),
        },
        Progress::Sending {
            index,
            of,
            source,
            remote,
            ..
        } => Payload::Sending {
            index,
            total: of,
            source: lossy(&source),
            remote,
        },
        Progress::Uploading {
            index, sent, total, ..
        } => Payload::Uploading {
            index,
            bytes: sent,
            total_bytes: total,
        },
        Progress::Done(r) => {
            let reason = r.outcome.reason();
            let (outcome, remote, sha256) = match r.outcome {
                Outcome::Verified { remote, sha256, .. } => {
                    (DoneKind::Verified, Some(remote), Some(sha256))
                }
                Outcome::Skipped(Skip::AlreadyOnWatch { remote }) => {
                    (DoneKind::Skipped, Some(remote), None)
                }
                Outcome::Skipped(_) => (DoneKind::Skipped, None, None),
                // The last name tried is the one worth showing; every one
                // of them is burned and in the ledger.
                Outcome::Failed { mut remotes, .. } => (DoneKind::Failed, remotes.pop(), None),
            };
            Payload::Done {
                index: r.index,
                source: lossy(&r.source),
                outcome,
                remote,
                sha256,
                reason,
            }
        }
        Progress::Finished(t) => t.into(),
        Progress::Connecting { .. } | Progress::AttemptFailed { .. } => return None,
    })
}

/// Upload progress fires per chunk: thousands of events per album. Each
/// one is a JSON message the webview parses to move a bar by a pixel, so
/// the rate is capped. Every other event, and the last tick of each upload,
/// always goes through, so the bar lands on full.
#[derive(Debug)]
pub struct Throttle {
    every: Duration,
    last: Option<Instant>,
}

impl Throttle {
    pub fn new(every: Duration) -> Self {
        Self { every, last: None }
    }

    pub fn admit(&mut self, p: &Payload, now: Instant) -> bool {
        let Payload::Uploading {
            bytes, total_bytes, ..
        } = p
        else {
            return true;
        };
        let last_tick = *total_bytes > 0 && bytes >= total_bytes;
        let due = self
            .last
            .is_none_or(|t| now.duration_since(t) >= self.every);
        if last_tick || due {
            self.last = Some(now);
            true
        } else {
            false
        }
    }
}

// ── backup ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupStarted {
    pub run_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupFailure {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BackupPayload {
    Listing,
    File {
        index: usize,
        total: usize,
        path: String,
        bytes: u64,
    },
    Finished {
        files: usize,
        bytes: u64,
        dest: String,
        /// Files that could not be copied, and why. The rest were.
        failed: Vec<BackupFailure>,
        /// Objects the watch listed but would not describe.
        unreadable: usize,
        stopped: bool,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BackupEvent {
    pub run_id: String,
    #[serde(flatten)]
    pub payload: BackupPayload,
}

/// Paths on the watch are device-controlled text: control bytes go.
pub fn map_backup(p: backup::Progress) -> BackupPayload {
    let clean = pelican_core::garmin::strip_control;
    match p {
        backup::Progress::Listing => BackupPayload::Listing,
        backup::Progress::File {
            index,
            total,
            path,
            bytes,
        } => BackupPayload::File {
            index,
            total,
            path: clean(&path),
            bytes,
        },
        backup::Progress::Finished(s) => BackupPayload::Finished {
            files: s.files,
            bytes: s.bytes,
            dest: lossy(&s.dest),
            failed: s
                .failed
                .into_iter()
                .map(|f| BackupFailure {
                    path: clean(&f.path),
                    reason: clean(&f.reason),
                })
                .collect(),
            unreadable: s.unreadable,
            stopped: s.stopped,
        },
    }
}

// ── watch list and ledger ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WatchRow {
    pub status: Origin,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

fn nonempty(s: &str) -> Option<String> {
    let s = pelican_core::garmin::strip_control(s);
    (!s.trim().is_empty()).then_some(s)
}

/// `/Music` rows, each `ledger` row joined to what this machine's ledger
/// says it holds. A stub's synthetic name is kept: it is the only handle
/// the UI has on it. A folder is marked with a trailing `/`.
pub fn watch_rows(rows: Vec<Row>, ledger: Option<&Ledger>) -> Vec<WatchRow> {
    let tags: std::collections::HashMap<String, &ledger::Event> = ledger
        .map(|l| {
            l.events()
                .iter()
                .filter(|e| e.event != ledger::Kind::Reset)
                .map(|e| (fold_name(&e.remote), e))
                .collect()
        })
        .unwrap_or_default();
    rows.into_iter()
        .map(|r| {
            let event = match r.origin {
                Origin::Ledger => tags.get(&fold_name(&r.name)).copied(),
                _ => None,
            };
            WatchRow {
                status: r.origin,
                name: Some(if r.is_folder {
                    format!("{}/", r.name)
                } else {
                    r.name
                }),
                bytes: r.size,
                title: event.and_then(|e| nonempty(&e.title)),
                artist: event.and_then(|e| e.artist.as_deref().and_then(nonempty)),
                album: event.and_then(|e| e.album.as_deref().and_then(nonempty)),
            }
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerRow {
    pub counter: u64,
    pub remote: String,
    pub event: ledger::Kind,
    pub at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LedgerDto {
    pub path: String,
    pub rows: Vec<LedgerRow>,
}

impl LedgerDto {
    pub fn from_core(l: &Ledger) -> Self {
        Self {
            path: lossy(l.path()),
            rows: l
                .events()
                .iter()
                .map(|e| LedgerRow {
                    counter: e.counter,
                    remote: e.remote.clone(),
                    event: e.event,
                    at: e.at.clone(),
                    title: nonempty(&e.title),
                    artist: e.artist.as_deref().and_then(nonempty),
                    album: e.album.as_deref().and_then(nonempty),
                    reason: e.reason.as_deref().and_then(nonempty),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelican_core::ledger::{Event, Kind};
    use pelican_core::transfer::FileReport;
    use serde_json::json;
    use std::path::PathBuf;

    fn event(run: &str, p: Progress) -> serde_json::Value {
        let payload = map_progress(p).expect("mapped");
        serde_json::to_value(ProgressEvent {
            run_id: run.into(),
            payload,
        })
        .unwrap()
    }

    fn done(outcome: Outcome) -> Progress {
        Progress::Done(FileReport {
            index: 3,
            source: PathBuf::from("/m/A/01 - One.flac"),
            outcome,
        })
    }

    #[test]
    fn in_flight_events_take_the_contract_names() {
        assert_eq!(
            event(
                "r1",
                Progress::Transcoding {
                    index: 2,
                    n: 1,
                    of: 5,
                    source: "/m/a.wav".into()
                }
            ),
            json!({"run_id": "r1", "kind": "transcoding", "index": 2, "total": 5, "source": "/m/a.wav"})
        );
        assert_eq!(
            event(
                "r1",
                Progress::Sending {
                    index: 2,
                    n: 1,
                    of: 4,
                    source: "/m/a.wav".into(),
                    remote: "pl00007-a.mp3".into(),
                    attempt: 2
                }
            ),
            json!({"run_id": "r1", "kind": "sending", "index": 2, "total": 4,
                   "source": "/m/a.wav", "remote": "pl00007-a.mp3"})
        );
        assert_eq!(
            event(
                "r1",
                Progress::Uploading {
                    index: 2,
                    remote: "pl00007-a.mp3".into(),
                    sent: 10,
                    total: 20
                }
            ),
            json!({"run_id": "r1", "kind": "uploading", "index": 2, "bytes": 10, "total_bytes": 20})
        );
    }

    #[test]
    fn the_two_informational_events_are_not_forwarded() {
        assert_eq!(
            map_progress(Progress::Connecting { files: 3, bytes: 9 }),
            None
        );
        assert_eq!(
            map_progress(Progress::AttemptFailed {
                index: 0,
                source: "/a".into(),
                remote: "pl00001-a.mp3".into(),
                reason: "hash mismatch".into(),
                retrying: true,
            }),
            None
        );
    }

    #[test]
    fn a_verified_file_carries_its_proven_hash() {
        assert_eq!(
            event(
                "r",
                done(Outcome::Verified {
                    remote: "pl00012-One.mp3".into(),
                    bytes: 4096,
                    sha256: "ab".repeat(32),
                })
            ),
            json!({"run_id": "r", "kind": "done", "index": 3, "source": "/m/A/01 - One.flac",
                   "outcome": "verified", "remote": "pl00012-One.mp3", "sha256": "ab".repeat(32)})
        );
    }

    #[test]
    fn skips_and_failures_say_why() {
        assert_eq!(
            event(
                "r",
                done(Outcome::Skipped(Skip::AlreadyOnWatch {
                    remote: "pl00003-One.mp3".into()
                }))
            ),
            json!({"run_id": "r", "kind": "done", "index": 3, "source": "/m/A/01 - One.flac",
                   "outcome": "skipped", "remote": "pl00003-One.mp3",
                   "reason": "already on watch as pl00003-One.mp3"})
        );
        assert_eq!(
            event("r", done(Outcome::Skipped(Skip::Stopped {}))),
            json!({"run_id": "r", "kind": "done", "index": 3, "source": "/m/A/01 - One.flac",
                   "outcome": "skipped", "reason": "stopped before it was sent"})
        );
        assert_eq!(
            event(
                "r",
                done(Outcome::Failed {
                    reason: "read-back hash differs".into(),
                    remotes: vec!["pl00004-One.mp3".into(), "pl00005-One.mp3".into()],
                })
            ),
            json!({"run_id": "r", "kind": "done", "index": 3, "source": "/m/A/01 - One.flac",
                   "outcome": "failed", "remote": "pl00005-One.mp3",
                   "reason": "read-back hash differs"})
        );
    }

    #[test]
    fn finished_and_error_are_the_contract_shape() {
        assert_eq!(
            event(
                "r",
                Progress::Finished(Tally {
                    verified: 20,
                    skipped: 3,
                    failed: 1,
                    stopped: true
                })
            ),
            json!({"run_id": "r", "kind": "finished", "verified": 20, "skipped": 3,
                   "failed": 1, "stopped": true})
        );
        let e = ProgressEvent {
            run_id: "r".into(),
            payload: Payload::Error {
                message: "no room".into(),
            },
        };
        assert_eq!(
            serde_json::to_value(e).unwrap(),
            json!({"run_id": "r", "kind": "error", "message": "no room"})
        );
    }

    #[test]
    fn upload_ticks_are_capped_but_the_last_one_always_lands() {
        let mut t = Throttle::new(Duration::from_millis(50));
        let up = |bytes| Payload::Uploading {
            index: 0,
            bytes,
            total_bytes: 100,
        };
        let t0 = Instant::now();
        assert!(t.admit(&up(1), t0), "the first tick");
        assert!(!t.admit(&up(2), t0 + Duration::from_millis(10)));
        assert!(t.admit(&up(3), t0 + Duration::from_millis(60)));
        assert!(t.admit(&up(100), t0 + Duration::from_millis(61)), "full");
        let other = Payload::Finished {
            verified: 1,
            skipped: 0,
            failed: 0,
            stopped: false,
        };
        assert!(t.admit(&other, t0 + Duration::from_millis(62)));
    }

    #[test]
    fn requests_parse_and_blank_overrides_are_none() {
        let r: PlanRequest = serde_json::from_value(json!({
            "paths": ["/m/A"],
            "overrides": {"artist": "  ", "year": "1999"},
            "mix": {"name": "Long Run"},
            "resend": true
        }))
        .unwrap();
        assert!(r.resend_sources.is_empty(), "optional for older callers");
        assert_eq!(r.paths, ["/m/A"]);
        assert_eq!(r.mix.as_ref().map(|m| m.name.as_str()), Some("Long Run"));
        assert!(r.resend);
        let ov = r.overrides.to_core();
        assert_eq!(ov.artist, None);
        assert_eq!(ov.year.as_deref(), Some("1999"));

        let r: PlanRequest = serde_json::from_value(json!({
            "paths": [], "overrides": {}, "mix": null, "resend": false
        }))
        .unwrap();
        assert!(r.mix.is_none());
        assert!(
            serde_json::from_value::<PlanRequest>(json!({"paths": [], "delete": true})).is_err()
        );
    }

    #[test]
    fn a_disconnected_status_names_the_fix_and_omits_the_rest() {
        let e = anyhow::anyhow!("no Garmin device found");
        let s = Status::from_error(&e, None);
        assert_eq!(
            serde_json::to_value(s).unwrap(),
            json!({"connected": false, "max_objects": 500,
                   "ledger": {"verified": 0, "failed": 0, "unresolved": 0, "resets": 0},
                   "error": "no Garmin device found", "error_kind": "not_found"})
        );
        let e = anyhow::anyhow!("opening USB interface: Permission denied (os error 13)");
        let m = explain_device_error(&e);
        assert!(m.contains("udev/70-garmin-mtp.rules"), "{m}");
        let e = anyhow::anyhow!("no Garmin device found on USB");
        assert_eq!(explain_device_error(&e), "no Garmin device found on USB");
    }

    #[test]
    fn a_wedged_watch_says_to_replug_and_nothing_else() {
        let io = std::io::Error::new(std::io::ErrorKind::TimedOut, "bulk in");
        let e = anyhow::Error::new(io).context("opening MTP session to Forerunner 165");
        let s = serde_json::to_value(Status::from_error(&e, None)).unwrap();
        assert_eq!(s["error_kind"], "wedged");
        assert_eq!(s["error"], core_error::REPLUG);
        // Wherever else a device error surfaces (a run's `error`, a
        // command's rejection), the text is the same instruction.
        let e = anyhow::Error::new(core_error::Wedged).context("the run stopped");
        assert_eq!(explain_device_error(&e), core_error::REPLUG);
    }

    #[test]
    fn every_error_kind_reaches_status() {
        let kind = |e: anyhow::Error, gvfs: Option<&str>| {
            serde_json::to_value(Status::from_error(&e, gvfs.map(str::to_string))).unwrap()
                ["error_kind"]
                .clone()
        };
        let denied = || anyhow::anyhow!("opening USB interface: Permission denied (os error 13)");
        assert_eq!(kind(denied(), None), "permission");
        let busy = || anyhow::anyhow!("claiming interface: exclusive access, resource busy");
        assert_eq!(kind(busy(), None), "busy");
        assert_eq!(kind(busy(), Some("gvfs-mtp holds it")), "gvfs");
        assert_eq!(kind(anyhow::anyhow!("something odd"), None), "other");
        // A permission failure keeps its udev fix in the text.
        let s = Status::from_error(&denied(), None);
        assert!(s.error.unwrap().contains("udev/70-garmin-mtp.rules"));
        // Connected: no kind at all.
        let v = serde_json::to_value(Status {
            connected: true,
            ..Status::default()
        })
        .unwrap();
        assert!(v.get("error_kind").is_none());
    }

    #[test]
    fn per_track_send_again_reaches_the_core() {
        let r: PlanRequest = serde_json::from_value(json!({
            "paths": ["/m/A", "/m/B/03 - Three.flac"],
            "mix": {"name": "Long Run"},
            "resend": false,
            "resend_sources": ["/m/A/01 - One.flac"]
        }))
        .unwrap();
        let resend = r.resend();
        assert!(resend.covers(Path::new("/m/A/01 - One.flac")));
        assert!(!resend.covers(Path::new("/m/A/02 - Two.flac")));
        let all = PlanRequest {
            resend: true,
            ..PlanRequest::default()
        };
        assert!(all.resend().covers(Path::new("/anything.flac")));
    }

    #[test]
    fn places_take_the_contract_shape() {
        let v = serde_json::to_value(places(vec![
            Place {
                label: "Home".into(),
                path: PathBuf::from("/home/user"),
                kind: PlaceKind::Home,
            },
            Place {
                label: "nas".into(),
                path: PathBuf::from("/mnt/nas"),
                kind: PlaceKind::Network,
            },
        ]))
        .unwrap();
        assert_eq!(
            v,
            json!([
                {"label": "Home", "path": "/home/user", "kind": "home"},
                {"label": "nas", "path": "/mnt/nas", "kind": "network"}
            ])
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_place_that_is_not_utf8_is_left_out() {
        use std::os::unix::ffi::OsStrExt;
        let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/run/media/six/\xff"));
        let out = places(vec![Place {
            label: "USB".into(),
            path: bad,
            kind: PlaceKind::Drive,
        }]);
        assert!(out.is_empty());
    }

    #[test]
    fn backup_events_take_the_contract_shape() {
        let ev = |p| {
            serde_json::to_value(BackupEvent {
                run_id: "b1".into(),
                payload: p,
            })
            .unwrap()
        };
        assert_eq!(
            ev(map_backup(backup::Progress::Listing)),
            json!({"run_id": "b1", "kind": "listing"})
        );
        assert_eq!(
            ev(map_backup(backup::Progress::File {
                index: 0,
                total: 5,
                path: "GARMIN/Activity/\u{1b}[2Ja.fit".into(),
                bytes: 12,
            })),
            json!({"run_id": "b1", "kind": "file", "index": 0, "total": 5,
                   "path": "GARMIN/Activity/[2Ja.fit", "bytes": 12})
        );
        let finished = map_backup(backup::Progress::Finished(backup::Summary {
            files: 4,
            bytes: 99,
            dest: PathBuf::from("/home/user/Documents/Pelican/FR165 backup 2026-09-26"),
            failed: vec![backup::Failed {
                path: "GARMIN/..".into(),
                reason: "not a plain file name".into(),
            }],
            unreadable: 1,
            stopped: false,
        }));
        assert_eq!(
            ev(finished),
            json!({"run_id": "b1", "kind": "finished", "files": 4, "bytes": 99,
                   "dest": "/home/user/Documents/Pelican/FR165 backup 2026-09-26",
                   "failed": [{"path": "GARMIN/..", "reason": "not a plain file name"}],
                   "unreadable": 1, "stopped": false})
        );
        assert_eq!(
            ev(BackupPayload::Error {
                message: core_error::REPLUG.into()
            }),
            json!({"run_id": "b1", "kind": "error", "message": core_error::REPLUG})
        );
    }

    #[test]
    fn a_reset_line_is_a_ledger_row_and_joins_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let mut l = Ledger::open(tmp.path(), "42").unwrap();
        let mut e = Event::new(Kind::Verified, 1, "pl00001-One.mp3");
        e.source_sha256 = "aa".into();
        l.append(e).unwrap();
        assert!(l.reset("factory reset confirmed").unwrap());
        let dto = serde_json::to_value(LedgerDto::from_core(&l)).unwrap();
        let rows = dto["rows"].as_array().unwrap();
        assert_eq!(rows.last().unwrap()["event"], "reset");
        assert_eq!(rows.last().unwrap()["remote"], "");
        let t: LedgerTotals = l.totals().into();
        assert_eq!((t.verified, t.resets), (0, 1));
    }

    #[test]
    fn ledger_rows_join_the_listing_by_folded_name() {
        let tmp = tempfile::tempdir().unwrap();
        {
            let mut l = Ledger::open(tmp.path(), "42").unwrap();
            let mut e = Event::new(Kind::Reserve, 1, "pl00001-One.mp3");
            e.title = "One".into();
            e.artist = Some("Band".into());
            e.album = Some("First".into());
            l.append(e.clone()).unwrap();
            e.event = Kind::Verified;
            l.append(e).unwrap();
        }
        let l = Ledger::read(tmp.path(), "42").unwrap();
        let row = |name: &str, size, origin| Row {
            name: name.into(),
            size,
            is_folder: false,
            origin,
            handle: 1,
        };
        let out = watch_rows(
            vec![
                row("PL00001-ONE.MP3", Some(9), Origin::Ledger),
                row("other.mp3", Some(5), Origin::Foreign),
                row("\u{2039}unreadable #7\u{203a}", None, Origin::Stub),
            ],
            Some(&l),
        );
        assert_eq!(
            serde_json::to_value(&out).unwrap(),
            json!([
                {"status": "ledger", "name": "PL00001-ONE.MP3", "bytes": 9,
                 "title": "One", "artist": "Band", "album": "First"},
                {"status": "foreign", "name": "other.mp3", "bytes": 5},
                {"status": "stub", "name": "\u{2039}unreadable #7\u{203a}"}
            ])
        );

        let dto = LedgerDto::from_core(&l);
        assert_eq!(dto.rows.len(), 2);
        let first = serde_json::to_value(&dto.rows[0]).unwrap();
        assert_eq!(first["event"], "reserve");
        assert_eq!(first["counter"], 1);
        assert_eq!(first["title"], "One");
        assert!(first.get("reason").is_none());
    }

    #[test]
    fn a_listing_keeps_utf8_paths_only() {
        use std::os::unix::ffi::OsStrExt;
        let bad = PathBuf::from(std::ffi::OsStr::from_bytes(b"/m/\xff.flac"));
        let l = library::Listing {
            path: "/m".into(),
            parent: Some("/".into()),
            dirs: vec![library::DirEntry {
                name: "A".into(),
                path: "/m/A".into(),
                audio_files: 3,
                has_subdirs: false,
            }],
            files: vec![
                library::FileEntry {
                    name: "\u{fffd}.flac".into(),
                    path: bad,
                    bytes: 1,
                },
                library::FileEntry {
                    name: "b.flac".into(),
                    path: "/m/b.flac".into(),
                    bytes: 2,
                },
            ],
        };
        assert_eq!(
            serde_json::to_value(ListingDto::from_core(l)).unwrap(),
            json!({"path": "/m", "parent": "/",
                   "dirs": [{"name": "A", "path": "/m/A", "audio_files": 3, "has_subdirs": false}],
                   "files": [{"name": "b.flac", "path": "/m/b.flac", "bytes": 2}]})
        );
    }
}
