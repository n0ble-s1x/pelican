//! The IPC surface: exactly the commands `ui/` invokes, each short.
//!
//! **No command does its work on the thread it is called on.** A sync
//! command runs on the main thread and would freeze the window for the
//! length of a USB read; an async one runs on a tokio worker *inside a
//! runtime context*, where the MTP backend's own `block_on` panics. So the
//! work goes to a plain OS thread ([`off_thread`]) and the command awaits
//! it. `push` starts its thread and returns the run id at once; the run is
//! heard about on [`dto::PROGRESS_EVENT`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, bail, Context, Result};
use tauri::{AppHandle, Emitter, State};

use pelican_core::garmin::{self, Device};
use pelican_core::ledger::Ledger;
use pelican_core::preview::{self, Room};
use pelican_core::transcode::encoder;
use pelican_core::transfer::{self, Options, PlanEntry, Stop};
use pelican_core::{backup, library, mtp, paths, places, platform, reset, source, watch};

use crate::dto::{
    self, BackupEvent, BackupPayload, BackupStarted, LedgerDto, ListingDto, Payload, PlaceDto,
    PlanRequest, PreviewDto, ProgressEvent, PushStarted, Status, WatchRow,
};
use crate::run::{self, Target};
use crate::Shell;

type Shared<'a> = State<'a, Arc<Shell>>;

/// Run `f` on a fresh OS thread, outside the async runtime, and await it.
async fn off_thread<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(move || {
        std::thread::Builder::new()
            .name("pelican-work".into())
            .spawn(f)
            .map_err(|e| format!("could not start a worker thread: {e}"))?
            .join()
            .map_err(|_| "the operation crashed; nothing was written".to_string())?
    })
    .await
    .map_err(|e| e.to_string())?
}

fn err(e: anyhow::Error) -> String {
    format!("{e:#}")
}

fn data_dir() -> Result<PathBuf> {
    paths::data_dir()
        .ok_or_else(|| anyhow!("no per-user data dir (neither XDG_DATA_HOME nor HOME is set)"))
}

fn serial_of(device: &Device) -> Result<String> {
    device.serial.clone().ok_or_else(|| {
        anyhow!(
            "{} reports no USB serial number, so Pelican cannot keep its ledger of the names \
             used on it, and cannot send to it.",
            device.label()
        )
    })
}

/// The watch to talk to: the one the last `status` saw, so a review made
/// against one watch's ledger is never sent to another.
fn pick(shell: &Shell) -> Result<Device> {
    garmin::pick_device(shell.watch.serial().as_deref())
}

// ── status ───────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn status(shell: Shared<'_>) -> Result<Status, String> {
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    off_thread(move || {
        let _guard = guard;
        let gvfs = platform::detect().map(|c| c.message());
        // A fresh look: forget which watch was seen last, so a swapped
        // watch is found rather than refused.
        shell.watch.forget();
        match read_status(gvfs.clone()) {
            Ok((s, room)) => {
                shell
                    .watch
                    .remember(s.serial.clone(), Some(room), s.model.clone());
                Ok(s)
            }
            Err(e) => Ok(Status::from_error(&e, gvfs)),
        }
    })
    .await
}

fn read_status(gvfs_warning: Option<String>) -> Result<(Status, Room)> {
    let device = garmin::pick_device(None)?;
    // The session is closed when the backend drops, at the end of this
    // statement, before the ledger is read.
    let snap = watch::read(mtp::open(&device)?.as_mut(), &device)?;
    let counts = snap.counts();
    let (ledger, error) = match serial_of(&device) {
        Ok(s) => (Ledger::read(&data_dir()?, &s)?.totals().into(), None),
        Err(e) => (Default::default(), Some(err(e))),
    };
    let room = Room {
        free_bytes: snap.free,
        music_objects: counts.audio_objects,
    };
    let status = Status {
        connected: true,
        model: Some(snap.model),
        serial: snap.serial,
        free_bytes: Some(snap.free),
        capacity_bytes: Some(snap.capacity),
        music_objects: Some(counts.audio_objects as u32),
        max_objects: watch::MAX_OBJECTS as u32,
        ledger,
        gvfs_warning,
        error,
        error_kind: None,
    };
    Ok((status, room))
}

// ── library ──────────────────────────────────────────────────────────────

#[tauri::command]
pub async fn library_list(path: String) -> Result<ListingDto, String> {
    off_thread(move || {
        library::list(&PathBuf::from(path))
            .map(ListingDto::from_core)
            .map_err(err)
    })
    .await
}

/// Where music usually lives, so nobody types a path: Home, Music, the
/// library, mounted network shares and removable drives. Reads the mount
/// table and a few folders; never the watch.
#[tauri::command]
pub async fn places(shell: Shared<'_>) -> Result<Vec<PlaceDto>, String> {
    let shell = shell.inner().clone();
    off_thread(move || {
        let cfg = crate::config::load(shell.config_file.as_deref());
        let root = cfg.library_root_or(crate::config::home().as_deref());
        Ok(dto::places(places::places(root.as_deref())))
    })
    .await
}

// ── preview and push ─────────────────────────────────────────────────────

fn plan(req: &PlanRequest) -> Result<Vec<PlanEntry>> {
    let paths: Vec<PathBuf> = req.paths.iter().map(PathBuf::from).collect();
    let sources = source::expand(&paths)?;
    transfer::plan_with(sources, &req.overrides.to_core(), req.mix.as_ref())
}

/// Reads files and the ledger; never the watch. Skips are marked from the
/// ledger of the watch the last `status` saw, and the fit is against the
/// room it reported, so call `status` first, and again after a push.
#[tauri::command]
pub async fn preview(shell: Shared<'_>, req: PlanRequest) -> Result<PreviewDto, String> {
    let shell = shell.inner().clone();
    off_thread(move || {
        let entries = plan(&req).map_err(err)?;
        let ledger = match shell.watch.serial() {
            Some(s) => Some(Ledger::read(&data_dir().map_err(err)?, &s).map_err(err)?),
            None => None,
        };
        let p = preview::build_with(&entries, ledger.as_ref(), &req.resend(), shell.watch.room());
        Ok(PreviewDto::from_core(p))
    })
    .await
}

fn run_id() -> String {
    static N: AtomicU64 = AtomicU64::new(0);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!("run-{ms:x}-{}", N.fetch_add(1, Ordering::Relaxed))
}

/// Start a run and return its id. Everything after the lock (planning,
/// the ffmpeg check, finding the watch) happens on the run's thread and
/// reports failure as the run's `error` event.
#[tauri::command]
pub fn push(app: AppHandle, shell: Shared<'_>, req: PlanRequest) -> Result<PushStarted, String> {
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    let id = run_id();
    let stop = Stop::new();
    shell.watch.set_stop(Some(stop.clone()));

    let run = id.clone();
    let spawned = std::thread::Builder::new()
        .name("pelican-push".into())
        .spawn(move || {
            let _guard = guard;
            let mut emit = |payload| {
                let _ = app.emit(
                    dto::PROGRESS_EVENT,
                    ProgressEvent {
                        run_id: run.clone(),
                        payload,
                    },
                );
            };
            if let Err(e) = push_run(&shell, &req, stop, &mut emit) {
                emit(Payload::Error {
                    message: dto::explain_device_error(&e),
                });
            }
            // What the watch holds has changed; the next preview must not
            // size against the old room.
            shell.watch.forget();
            shell.watch.set_stop(None);
        });
    if let Err(e) = spawned {
        return Err(format!("could not start the run: {e}"));
    }
    Ok(PushStarted { run_id: id })
}

fn push_run(
    shell: &Shell,
    req: &PlanRequest,
    stop: Stop,
    emit: &mut (dyn FnMut(Payload) + Send),
) -> Result<()> {
    let entries = plan(req)?;
    if entries.is_empty() {
        bail!("no audio files found in what was chosen");
    }
    encoder::require()?;
    let device = pick(shell)?;
    let serial = serial_of(&device)?;
    let data = data_dir()?;
    let cache = paths::cache_dir()
        .ok_or_else(|| anyhow!("no per-user cache dir (neither XDG_CACHE_HOME nor HOME is set)"))?;
    run::push(
        entries,
        Target {
            serial: &serial,
            data: &data,
            cache: &cache,
        },
        Options {
            resend: req.resend,
            resend_sources: req.resend_sources.iter().map(PathBuf::from).collect(),
            ..Options::default()
        },
        || mtp::open(&device),
        &encoder::encode,
        stop,
        emit,
    )
    .context("the run stopped")?;
    Ok(())
}

/// Stop the running push or backup before its next file. The file in
/// flight is finished (and, in a push, proven) first; there is no mid-file
/// stop.
#[tauri::command]
pub fn stop(shell: Shared<'_>) {
    shell.watch.request_stop();
}

// ── watch list and ledger ────────────────────────────────────────────────

#[tauri::command]
pub async fn watch_list(shell: Shared<'_>) -> Result<Vec<WatchRow>, String> {
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    off_thread(move || {
        let _guard = guard;
        (|| -> Result<Vec<WatchRow>> {
            let device = pick(&shell)?;
            let ledger = match device.serial.as_deref() {
                Some(s) => Some(Ledger::read(&data_dir()?, s)?),
                None => None,
            };
            let snap = watch::read(mtp::open(&device)?.as_mut(), &device)?;
            Ok(dto::watch_rows(snap.rows(ledger.as_ref()), ledger.as_ref()))
        })()
        .map_err(|e| dto::explain_device_error(&e))
    })
    .await
}

/// The ledger of the watch the last `status` saw, else of the one plugged
/// in. Finding it is a USB enumeration, not a session, so this does not
/// take the device lock; during a push the ledger's own lock answers.
#[tauri::command]
pub async fn ledger(shell: Shared<'_>) -> Result<LedgerDto, String> {
    let shell = shell.inner().clone();
    off_thread(move || {
        (|| -> Result<LedgerDto> {
            let serial = match shell.watch.serial() {
                Some(s) => s,
                None => serial_of(&garmin::pick_device(None)?)?,
            };
            Ok(LedgerDto::from_core(&Ledger::read(&data_dir()?, &serial)?))
        })()
        .map_err(err)
    })
    .await
}

// ── before a factory reset: the backup ──────────────────────────────────

/// Where a backup goes unless the person picks elsewhere:
/// `~/Documents/Pelican/<model> backup <YYYY-MM-DD>`. Not created. The
/// model is the one the last `status` read, else the USB label.
#[tauri::command]
pub async fn default_backup_dir(shell: Shared<'_>) -> Result<String, String> {
    let shell = shell.inner().clone();
    off_thread(move || {
        let model = shell
            .watch
            .model()
            .or_else(|| garmin::pick_device(None).ok().map(|d| d.label()))
            .unwrap_or_else(|| "Garmin watch".to_string());
        backup::default_dest(&model)
            .map(|p| p.to_string_lossy().into_owned())
            .ok_or_else(|| "HOME is not set, so there is no Documents folder".to_string())
    })
    .await
}

/// The destination a backup may write to: an absolute path that is not an
/// existing file. The core never replaces a file inside it.
fn backup_dest(dest: &str) -> Result<PathBuf, String> {
    let p = PathBuf::from(dest.trim());
    if !p.is_absolute() {
        return Err("choose a folder for the backup".into());
    }
    if p.exists() && !p.is_dir() {
        return Err(format!("{} is a file, not a folder", p.display()));
    }
    Ok(p)
}

/// Copy every file under the watch's `GARMIN` folder into `dest/GARMIN`.
/// Read-only on the watch: it lists and downloads, nothing else. Returns
/// the run id at once; the run is heard about on [`dto::BACKUP_EVENT`] and
/// `stop` ends it between files.
#[tauri::command]
pub fn backup_watch(
    app: AppHandle,
    shell: Shared<'_>,
    dest: String,
) -> Result<BackupStarted, String> {
    let dest = backup_dest(&dest)?;
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    let id = run_id();
    let stop = Stop::new();
    shell.watch.set_stop(Some(stop.clone()));

    let run = id.clone();
    let spawned = std::thread::Builder::new()
        .name("pelican-backup".into())
        .spawn(move || {
            let _guard = guard;
            let emit = |payload| {
                let _ = app.emit(
                    dto::BACKUP_EVENT,
                    BackupEvent {
                        run_id: run.clone(),
                        payload,
                    },
                );
            };
            let r = (|| -> Result<()> {
                let device = pick(&shell)?;
                let mut dev = mtp::open(&device)?;
                backup::backup(dev.as_mut(), &dest, &stop, &mut |p| {
                    emit(dto::map_backup(p))
                })?;
                Ok(())
            })();
            if let Err(e) = r {
                if pelican_core::error::is_wedged(&e) {
                    shell.watch.forget();
                }
                emit(BackupPayload::Error {
                    message: dto::explain_device_error(&e),
                });
            }
            shell.watch.set_stop(None);
        });
    if let Err(e) = spawned {
        return Err(format!("could not start the backup: {e}"));
    }
    Ok(BackupStarted { run_id: id })
}

// ── after a factory reset: the ledger ────────────────────────────────────

/// Re-read `/Music` now and count the audio objects left. Read-only.
#[tauri::command]
pub async fn reset_check(shell: Shared<'_>) -> Result<reset::Check, String> {
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    off_thread(move || {
        let _guard = guard;
        (|| -> Result<reset::Check> {
            let device = pick(&shell)?;
            reset::check(mtp::open(&device)?.as_mut())
        })()
        .map_err(|e| dto::explain_device_error(&e))
    })
    .await
}

/// Start a fresh ledger for this watch, only if the watch, read again
/// here and now, holds no audio. The person's "it's clean" is never taken
/// on its own; the core re-reads `/Music` in this call and refuses while
/// anything is left. Append-only: a `reset` line, and the name counter
/// keeps rising.
#[tauri::command]
pub async fn reset_ledger(shell: Shared<'_>) -> Result<reset::Outcome, String> {
    let shell = shell.inner().clone();
    let guard = shell.watch.lock.try_acquire()?;
    off_thread(move || {
        let _guard = guard;
        (|| -> Result<reset::Outcome> {
            let device = pick(&shell)?;
            let serial = serial_of(&device)?;
            let mut ledger = Ledger::open(&data_dir()?, &serial)?;
            reset::reset_ledger(mtp::open(&device)?.as_mut(), &mut ledger)
        })()
        .map_err(|e| dto::explain_device_error(&e))
    })
    .await
}

// ── the USB rule ─────────────────────────────────────────────────────────

/// Whether the udev rule is installed and is the one this build ships.
#[tauri::command]
pub async fn udev_rule_status() -> Result<crate::udev::RuleStatus, String> {
    off_thread(|| Ok(crate::udev::status())).await
}

/// Install the udev rule through polkit: one fixed command, the rule on its
/// stdin, nothing from the webview in it (`udev.rs`). Waits for as long as
/// the password prompt is open, on its own thread.
#[tauri::command]
pub async fn install_udev_rule() -> Result<crate::udev::InstallResult, String> {
    off_thread(|| Ok(crate::udev::install())).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_goes_only_to_an_absolute_folder() {
        assert!(backup_dest("").is_err());
        assert!(backup_dest("Documents/backup").is_err());
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("f");
        std::fs::write(&file, b"x").unwrap();
        assert!(backup_dest(file.to_str().unwrap()).is_err());
        let new = tmp.path().join("FR165 backup 2026-09-26");
        assert_eq!(backup_dest(new.to_str().unwrap()).unwrap(), new);
        assert_eq!(
            backup_dest(tmp.path().to_str().unwrap()).unwrap(),
            tmp.path()
        );
    }
}
