//! Everything that crosses the IPC boundary, and the sink that carries it.
//!
//! Nothing in `pelican-core` is serde-ready except `history`, and that is
//! deliberate: the engine's types answer to the transfer loop, not to a
//! webview. So the shapes the frontend consumes live here, in one file, and
//! the translation from engine types to them is the only place a rename or a
//! unit change has to happen.
//!
//! One channel carries the whole stream, internally tagged, so `ui/app.js`
//! is an exhaustive `switch` with no schema duplication and no build step.

use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;

use pelican_core::history::UploadRecord;
use pelican_core::mtp::RemoteEntry;

/// A file (or a broken stub) the watch reports in `/Music`.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntryDto {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_folder: bool,
    /// The handle exists but `GetObjectInfo` failed for it — almost always a
    /// stub left by an upload the firmware rejected. The UI offers delete and
    /// nothing else for these.
    pub is_broken: bool,
}

impl From<&RemoteEntry> for RemoteEntryDto {
    fn from(e: &RemoteEntry) -> Self {
        Self {
            name: e.name.clone(),
            path: e.path.clone(),
            size: e.size,
            is_folder: e.is_folder,
            is_broken: e.is_broken,
        }
    }
}

/// One local file the scan found, already planned.
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackDto {
    /// Absolute on-disk path. Also the identity the UI sends back to
    /// `start_sync` and `set_now_playing`.
    pub path: String,
    /// Title tag, falling back to the file stem so a row is never blank.
    /// **The fallback is not a title.** `has_title` is the only thing that
    /// says whether the file carries one, and any copy that makes a claim
    /// about the file's tags must read that field, not this one — the app
    /// once printed a stem in the Title column and four rows lower told the
    /// owner the same file had no title.
    pub title: String,
    /// Artist tag, or an empty string. Empty is meaningful — see
    /// `has_artist`.
    pub artist: String,
    pub album: String,
    /// Track number, already reduced from Vorbis's `3/12` form. `None` when
    /// the file carries none — which is a different fact from track 0, and
    /// the ordering below treats it as such.
    pub track: Option<u32>,
    /// Disc number, verbatim. Read for local ordering only; it is not in the
    /// six-field allowlist written to the watch, and `tags.rs` says why.
    pub disc: Option<String>,
    /// Whatever the file calls its date. Used to order albums by year, never
    /// parsed into a claim about a release.
    pub date: Option<String>,
    pub duration_secs: u64,
    /// Presentation label for the format column: "FLAC 24/96", "MP3 320".
    pub fmt: String,
    /// What this file will take up **on the watch** — the converted estimate,
    /// not the source size. A 24/96 FLAC lands about a tenth of its size.
    pub bytes: u64,
    pub source_bytes: u64,
    /// Three facts, not one. They used to be a single `playable_in_library`
    /// boolean, and the UI expanded that one bit back into two specific
    /// claims — "N tracks need a title" and "<names> have no title or
    /// artist" — which were false about every file that had a title and no
    /// artist, and about every file Pelican could not open at all. A screen
    /// may only say what was measured, so the measurement is carried whole.
    ///
    /// `readable` is false when the parser could not open the file. That is
    /// **not** the same as untagged: a truncated FLAC with no `fLaC` marker
    /// is a broken file, and telling its owner it lacks tags sends them to
    /// fix the wrong thing. When it is false the other two are meaningless
    /// and no copy may make a claim about the file's tags.
    pub readable: bool,
    /// Whether the file itself carries a title. Not to be confused with
    /// `title` above, which falls back to the file stem.
    pub has_title: bool,
    /// Whether the file itself carries an artist (or album-artist).
    pub has_artist: bool,
    /// `passthrough` | `ffmpeg` | `afconvert` | `unsupported`.
    pub pipeline: String,
    /// False when no installed encoder can read this format. The row still
    /// renders — with `note` explaining — but cannot be selected.
    pub sendable: bool,
    /// Human-readable consequence of the plan, or the refusal.
    pub note: Option<String>,
}

/// Why a file was skipped. Advisory: the underlying reason string is always
/// carried alongside and is what actually gets rendered.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SkipKind {
    NotAudio,
    NotPlayable,
    MissingTags,
    /// The watch already holds a file under this name. The one skip the user
    /// has to act on: nothing was sent, and nothing will be until they either
    /// remove the file on the watch or send this one under a new name.
    NameTaken,
    Other,
}

/// Why a file failed. Same contract as [`SkipKind`] — a hint for choosing a
/// label and an affordance, never a replacement for the message.
#[derive(Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailKind {
    DeviceBusy,
    OpenSession,
    NoEncoder,
    SizeMismatch,
    /// Every byte streamed and the watch never confirmed the write. The file
    /// may be aboard; the post-run listing is what decides. Distinct from
    /// `Other` because the shell reconciles on it and the UI words it
    /// differently — asserting a flat failure here would be a claim about the
    /// device we cannot support.
    Unconfirmed,
    Other,
}

/// Classification is string matching, and it is fragile.
///
/// `pelican-core` has no typed errors anywhere — every failure is an
/// `anyhow::Error` whose text is the only signal. Rather than sprinkle that
/// fragility across the UI, it is confined to these two functions, and the
/// original string is carried through on every event so the interface never
/// depends on the guess being right. If a message in `transfer.rs`,
/// `mtp.rs` or `encoder.rs` is reworded, the worst outcome is a generic
/// label — not a wrong one, and never a lost message.
pub fn classify_skip(reason: &str) -> SkipKind {
    let r = reason.to_ascii_lowercase();
    if r.contains("not an audio file") {
        SkipKind::NotAudio
    } else if r.contains("not playable as-is") {
        SkipKind::NotPlayable
    } else if r.contains("missing title/artist") {
        SkipKind::MissingTags
    } else if r.contains("is already on your watch") {
        SkipKind::NameTaken
    } else {
        SkipKind::Other
    }
}

pub fn classify_fail(error: &str) -> FailKind {
    let e = error.to_ascii_lowercase();
    // Contention first: an exclusive-access failure surfaces *through*
    // `opening session: `, and "the watch is held by something else" is a
    // far more actionable thing to tell the user than "the open failed".
    if e.contains("exclusive access") || e.contains("busy") || e.contains("access is denied") {
        FailKind::DeviceBusy
    } else if e.contains("the bytes finished streaming but the watch did not confirm") {
        FailKind::Unconfirmed
    } else if e.contains("post-write size mismatch") {
        FailKind::SizeMismatch
    } else if e.starts_with("opening session: ") {
        FailKind::OpenSession
    } else if e.contains("install ffmpeg") || e.contains("cannot be converted") {
        FailKind::NoEncoder
    } else {
        FailKind::Other
    }
}

/// The single event stream. Internally tagged on `type`; every field
/// camelCased so the JS reads naturally without a mapping layer.
#[derive(Clone, Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum UiEvent {
    /// A Garmin device appeared on USB. Not the same as "connected" — no MTP
    /// session has been opened yet. The frontend decides whether to connect.
    Attached {
        label: String,
        serial: Option<String>,
        product: Option<String>,
    },
    /// No Garmin device on USB. Local browsing and playback keep working.
    Detached,
    /// The full picture of the device, after a successful open. Sent on
    /// connect, after a delete, and once a sync finishes.
    Snapshot {
        free: u64,
        total: u64,
        /// What the watch reports its model as, from MTP `GetDeviceInfo`.
        /// Only knowable once a session is open — the USB descriptors carry
        /// no product string on this device — so it rides the snapshot
        /// rather than `Attached`.
        model: Option<String>,
        entries: Vec<RemoteEntryDto>,
        /// Our own journal of what we sent, keyed by the name written to the
        /// device. It is a record of *our sends*, not an observation of the
        /// watch: `entries` is the observation, and the UI must not present
        /// the two as the same kind of fact.
        uploads: Vec<UploadRecord>,
    },
    /// A local folder finished scanning.
    Scanned {
        root: String,
        tracks: Vec<TrackDto>,
        /// How many audio files were found, before any cap.
        found: usize,
        /// True when `tracks` was truncated.
        truncated: bool,
        /// Best encoder available on this machine, or `None`.
        encoder: Option<String>,
        /// True when the selected encoder's output profile has been observed
        /// **playing** on a watch — not merely arriving.
        ///
        /// "Verified" is deliberately the strong claim: the watch's music app
        /// indexed the result and played it. `docs/macos-port.md` records
        /// upload acceptance and library indexing as different subsystems,
        /// and the MP3 profile is pinned to CBR 192 kbps precisely because
        /// the indexer is fussier than the upload path.
        ///
        /// **Neither encoder clears that bar today, so this is always
        /// `false`.** Nothing Pelican has produced has been confirmed to play
        /// on a watch, on either platform. The field is kept, rather than
        /// deleted, because it is where a future confirmation lands: set it
        /// when a track produced by a named profile is *heard* playing on a
        /// stated model and firmware, and not before. The flag tracks the
        /// observation, not the intent.
        encoder_verified: bool,
    },

    Planned {
        total: u32,
        bytes: u64,
    },
    FileStarted {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
        path: String,
    },
    /// Converting or copying on this Mac. **The device is idle here** — a UI
    /// that says "uploading" during a 40-second FLAC decode looks wedged.
    FileStaging {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
    },
    FileProgress {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
        file_bytes: u64,
        file_total: u64,
        job_bytes: u64,
        job_total: u64,
    },
    FileDone {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
        bytes: u64,
        /// Set only when the collision resolver moved this file to a free
        /// name. The completion line has to state the name actually written —
        /// a user who is not told is a user whose watch holds a track under a
        /// name they never chose.
        renamed_to: Option<String>,
    },
    FileSkipped {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
        /// The local file, so the interface can offer to re-send exactly the
        /// tracks a collision refused.
        path: String,
        /// Verbatim from the engine. Render this, not the kind.
        reason: String,
        kind: SkipKind,
    },
    FileFailed {
        completed: u32,
        total: u32,
        ok: u32,
        skipped: u32,
        failed: u32,
        name: String,
        /// Verbatim, with the whole `.context()` chain intact. The
        /// size-mismatch message in particular states whether the stub was
        /// cleaned up, which is what tells the user if a retry is safe.
        error: String,
        kind: FailKind,
    },
    /// The run drained. `stopped` means the user pressed Stop and the loop
    /// broke between files — not that anything was aborted mid-upload.
    SyncFinished {
        ok: u32,
        skipped: u32,
        failed: u32,
        stopped: bool,
        /// Bytes of the plan that actually reached the watch, and the plan's
        /// total. The meter is byte-weighted, and a run that failed halfway
        /// must not be allowed to finish full: `delivered / planned` is the
        /// fraction that is true.
        delivered_bytes: u64,
        planned_bytes: u64,
    },
    /// A file the run reported as failed or uncertain, which the fresh
    /// listing then found on the watch at the size we sent.
    ///
    /// Emitted **after** the `Snapshot`, deliberately: the UI already has the
    /// listing in hand when the correction arrives, so it can revise the
    /// sentence against evidence the user can also see. It is not folded into
    /// `Snapshot` because a snapshot also fires on connect and after a
    /// delete, where a `landed` field would mean nothing.
    SyncReconciled {
        landed: Vec<String>,
    },
    /// A batch delete drained. One per batch, followed by one `Snapshot`.
    Deleted {
        ok: u32,
        failed: u32,
    },
    /// One file in a batch delete failed. `error` is verbatim from the engine,
    /// same contract as `FileFailed`.
    DeleteFailed {
        name: String,
        error: String,
    },

    /// Answer to `cover_art`. `art` is a `data:` URL, or `None` when the file
    /// carries no embedded picture or its picture is over the size cap.
    ///
    /// `None` means *render nothing*. A placeholder would imply Pelican looked
    /// and found something, and on the Control Center side omitting the
    /// artwork lets macOS show the app's own icon, which is true.
    CoverArt {
        /// The path that was asked about, so the UI can match the answer to
        /// the request without holding a pending map.
        path: String,
        art: Option<String>,
    },

    Error {
        message: String,
        /// Set when the failure was something else holding the device. The
        /// text is the same `{e:#}` chain — `mtp::open` already folds
        /// `platform::explain_exclusive_access()` into it — but the presence
        /// of this field is what tells the UI to render it as a persistent
        /// explanation in the empty state rather than a transient error.
        contention: Option<String>,
    },
}

/// Holds the one `Channel` the frontend opened, so background threads can
/// emit without owning it.
///
/// The channel arrives from `subscribe`, which is called after the threads
/// are already running. Rather than order the startup carefully, [`send`]
/// reports whether it reached anyone, and the watch poller uses that to
/// re-announce device state on its next tick. Self-healing beats sequencing.
#[derive(Default)]
pub struct EventSink {
    chan: Mutex<Option<Channel<UiEvent>>>,
}

impl EventSink {
    pub fn install(&self, ch: Channel<UiEvent>) {
        *self.lock() = Some(ch);
    }

    /// Returns false when there is no subscriber, or the webview went away.
    pub fn send(&self, ev: UiEvent) -> bool {
        let guard = self.lock();
        match guard.as_ref() {
            Some(ch) => ch.send(ev).is_ok(),
            None => false,
        }
    }

    /// A panic in one emit must not wedge every other thread's ability to
    /// report, so poisoning is recovered from rather than propagated.
    fn lock(&self) -> std::sync::MutexGuard<'_, Option<Channel<UiEvent>>> {
        self.chan.lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// `{e:#}`, always. The `.context()` chain is where every actionable string
/// in this codebase lives; `{e}` throws all of it away.
pub fn err(e: &anyhow::Error) -> String {
    format!("{e:#}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contention_beats_the_open_session_prefix() {
        // The real message is "opening session: ... could not open interface
        // for exclusive access ...". Both patterns match; the useful one must
        // win, because "something is holding your watch" is actionable and
        // "the open failed" is not.
        let msg = "opening session: opening MTP session to Forerunner 165: \
                   could not open interface for exclusive access";
        assert!(matches!(classify_fail(msg), FailKind::DeviceBusy));
    }

    #[test]
    fn size_mismatch_is_its_own_kind() {
        let msg = "post-write size mismatch: expected 4194304, watch reports 0";
        assert!(matches!(classify_fail(msg), FailKind::SizeMismatch));
    }

    #[test]
    fn missing_encoder_is_recognised_from_the_engine_wording() {
        // Exactly what `encoder::unsupported` produces today.
        let msg = ".ogg cannot be converted: the encoder available here \
                   (afconvert) cannot read it. Install ffmpeg to handle every \
                   format Pelican accepts.";
        assert!(matches!(classify_fail(msg), FailKind::NoEncoder));
    }

    /// `transfer::describe_upload_failure` writes this sentence, and the
    /// shell reconciles the run on the kind it produces. If the wording there
    /// changes without this changing, a drained upload silently becomes a
    /// plain failure and stops being revised by the listing.
    #[test]
    fn a_drained_upload_is_its_own_kind() {
        let msg = "the bytes finished streaming but the watch did not confirm the write — \
                   the file may be on your watch; check the list below \
                   (uploading /x/t.m4a (streamed 4096 of 4096 bytes): kIOReturnAborted)";
        assert!(matches!(classify_fail(msg), FailKind::Unconfirmed));
    }

    #[test]
    fn unknown_errors_fall_back_rather_than_guess() {
        assert!(matches!(
            classify_fail("the cable fell out"),
            FailKind::Other
        ));
        assert!(matches!(classify_skip("no idea"), SkipKind::Other));
    }

    #[test]
    fn skips_match_the_engines_exact_wording() {
        assert!(matches!(
            classify_skip("not an audio file"),
            SkipKind::NotAudio
        ));
        assert!(matches!(
            classify_skip("not playable as-is, and normalization is off"),
            SkipKind::NotPlayable
        ));
        assert!(matches!(
            classify_skip("missing title/artist — would be hidden on the watch"),
            SkipKind::MissingTags
        ));
    }

    /// The one skip the user has to act on. The sentence is written by
    /// `transfer::name_taken_reason` and produced by both halves of the
    /// collision guard — the plan-time resolver and the write-time refusal —
    /// so this one match covers both.
    #[test]
    fn a_refused_collision_is_recognised_from_the_engine_wording() {
        let msg = "a file called Track.mp3 is already on your watch — sending this one \
                   over it would destroy both copies, so it was not sent";
        assert!(matches!(classify_skip(msg), SkipKind::NameTaken));
    }
}
