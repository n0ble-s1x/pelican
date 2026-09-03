//! The IPC surface. Every function here is short on purpose.
//!
//! **No `async fn` in this file, ever.** Measured on this machine with an
//! instrumented Tauri 2.11.5 build:
//!
//! ```text
//! sync-cmd:  on_main=true   name="main"            in_tokio_ctx=false
//! async-cmd: on_main=false  name="tokio-rt-worker" in_tokio_ctx=true
//! ```
//!
//! An `async fn` command runs on a tokio worker *inside a runtime context*,
//! and `MtpRsBackend` drives its transport with `rt.block_on` — which panics
//! when a runtime is already entered. A sync command avoids the panic but
//! runs on the macOS main thread, so any real work freezes the wry event
//! loop and the entire window for its duration.
//!
//! Neither is acceptable for device work, so the rule is stricter than
//! either failure mode: **no command body performs device, transcode or
//! filesystem-walk work inline.** Each one validates its arguments, posts a
//! message to a worker thread, and returns in microseconds. Results arrive
//! on the `Channel`.
//!
//! `pick_folder` is the one deliberate exception, and it is not really an
//! exception: NSOpenPanel must run on the main thread, sync commands already
//! are on the main thread, and a modal picker blocks the UI by definition.

use std::path::{Path, PathBuf};
use std::sync::atomic::Ordering;

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::device::DeviceOp;
use crate::dto::UiEvent;
use crate::{config, scan, Shell};

/// Exactly `encodeURIComponent`'s unreserved set.
///
/// The `.` is load-bearing: WKWebView decides how to decode a media resource
/// from the URL **extension**, not from `Content-Type`. Percent-encoding the
/// dot turns `track.flac` into `track%2Eflac`, and identical bytes that play
/// under `.flac` fail with `MediaError 4`. `NON_ALPHANUMERIC` alone would do
/// exactly that.
const URI_COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

const BUSY: &str = "A transfer is running. Wait for it to finish, or press Stop.";

/// Open the event stream. Called once, on load.
#[tauri::command]
pub fn subscribe(shell: State<'_, Shell>, on_event: Channel<UiEvent>) -> Result<(), String> {
    shell.sink.install(on_event);
    // The USB poller has been running since setup and may already have
    // announced into nothing. Ask it to say it again.
    shell.watch.request_announce();
    // If the user opened a folder in a previous session, show it again
    // without making them re-pick. Doing it here rather than through another
    // command keeps the ACL surface at ten entries.
    if let Some(root) = shell.library_root() {
        scan::spawn(shell.sink.clone(), root, shell.encoders.clone());
    }
    Ok(())
}

/// Open an MTP session and read the device. The answer arrives as `Snapshot`.
#[tauri::command]
pub fn connect(shell: State<'_, Shell>, serial: Option<String>) -> Result<(), String> {
    if shell.device.is_busy() {
        return Err(BUSY.into());
    }
    shell.device.post(DeviceOp::Connect { serial })
}

/// Drop the session. The escape hatch when a previous one leaked and the
/// watch is refusing to open.
#[tauri::command]
pub fn disconnect(shell: State<'_, Shell>) -> Result<(), String> {
    shell.device.post(DeviceOp::Disconnect)
}

/// Remove objects from the watch. Irreversible: MTP has no trash and no
/// undo. The confirmation is the UI's job and is written to say so; this
/// layer's job is to refuse an empty or ill-formed batch rather than post a
/// no-op that would still cost a `snapshot`.
#[tauri::command]
pub fn delete_remote(shell: State<'_, Shell>, paths: Vec<String>) -> Result<(), String> {
    if shell.device.is_busy() {
        return Err(BUSY.into());
    }
    let paths: Vec<String> = paths.into_iter().filter(|p| !p.trim().is_empty()).collect();
    if paths.is_empty() {
        return Err("no path given".into());
    }
    shell.device.post(DeviceOp::Delete { paths })
}

/// Drop rows from Pelican's own journal.
///
/// A journal row the device does not list has nothing to delete. Clearing
/// the record is the only action that exists for it, and the UI's copy for
/// this says exactly that — it must never borrow the delete wording, because
/// nothing on the watch changes.
#[tauri::command]
pub fn forget_uploads(shell: State<'_, Shell>, names: Vec<String>) -> Result<(), String> {
    if shell.device.is_busy() {
        return Err(BUSY.into());
    }
    let names: Vec<String> = names.into_iter().filter(|n| !n.trim().is_empty()).collect();
    if names.is_empty() {
        return Err("nothing selected".into());
    }
    shell.device.post(DeviceOp::Forget { names })
}

/// The modal folder picker. **Blocks, correctly** — see the module note.
///
/// Picking is also the grant: the chosen directory is what the asset
/// protocol is allowed to serve for the rest of this process. Canonicalize
/// first, because `Scope::is_allowed` resolves symlinks before it matches
/// globs — a `~/Music/NAS -> /Volumes/...` link would not match a `$HOME`
/// pattern, and the files under it would silently refuse to play.
#[tauri::command]
pub fn pick_folder(app: AppHandle, shell: State<'_, Shell>) -> Result<Option<String>, String> {
    let Some(picked) = rfd::FileDialog::new()
        .set_title("Choose a music folder")
        .pick_folder()
    else {
        return Ok(None);
    };
    let root = picked
        .canonicalize()
        .map_err(|e| format!("resolving {}: {e}", picked.display()))?;

    grant(&app, shell, &root)?;
    config::set_library_root(&root);
    Ok(Some(root.display().to_string()))
}

/// Walk a folder and read every audio file's tags. Answers as `Scanned`.
#[tauri::command]
pub fn scan_folder(shell: State<'_, Shell>, path: String) -> Result<(), String> {
    let root = PathBuf::from(&path);
    // The user grants a folder by picking it; nothing else may be read. A
    // script that got into the webview cannot use this to enumerate the disk.
    if !shell.is_granted(&root) {
        return Err(format!(
            "{path} has not been opened in Pelican. Use “Choose folder” first."
        ));
    }
    scan::spawn(shell.sink.clone(), root, shell.encoders.clone());
    Ok(())
}

/// Queue a selection. Progress arrives as the `file*` events.
#[tauri::command]
pub fn start_sync(
    shell: State<'_, Shell>,
    paths: Vec<String>,
    skip_tag_check: bool,
) -> Result<(), String> {
    if paths.is_empty() {
        return Err("nothing selected".into());
    }
    let mut out = Vec::with_capacity(paths.len());
    for p in paths {
        let path = PathBuf::from(&p);
        if !shell.is_granted(&path) {
            return Err(format!("{p} is outside the folder you opened."));
        }
        out.push(path);
    }
    // Claim `busy` here, synchronously, rather than reading it.
    //
    // `is_busy()` was a check against a flag the *worker* sets when it
    // dequeues, and `post` only pushes onto an unbounded channel — so two
    // quick clicks both passed and both ran. The second run's `Planned` then
    // called `startRun`, which clears the notices and the counters, and the
    // first run's report was destroyed. The UI's own `syncing` guard is no
    // help: it goes true when `Planned` arrives, long after `expand_inputs`
    // has walked the tree. `device::start_sync` releases the flag on every
    // exit path, through a guard, because a missed release deadlocks Send for
    // the life of the process.
    if shell
        .device
        .busy
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err(BUSY.into());
    }
    let posted = shell.device.post(DeviceOp::StartSync {
        paths: out,
        skip_tag_check,
    });
    if posted.is_err() {
        // Nothing will dequeue it, so nothing will release the flag.
        shell.device.busy.store(false, Ordering::SeqCst);
    }
    posted
}

/// Stop **after the current track**.
///
/// There is no cancellation point inside a file: `Backend::upload` streams to
/// completion or errors. A 60 MB FLAC that is mid-upload will finish. The
/// button is labelled to say so.
#[tauri::command]
pub fn stop_sync(shell: State<'_, Shell>) -> Result<(), String> {
    shell.device.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

/// Hand the `<audio>` element a URL for a local file.
///
/// Pure string work after a scope check — no I/O, nothing to block on, so
/// this one is genuinely safe inline.
///
/// The URL carries the **true on-disk path**, never an opaque id. WKWebView
/// resolves the media type from the extension, so an id-based scheme breaks
/// FLAC and WAV even though the bytes are identical.
#[tauri::command]
pub fn set_now_playing(
    app: AppHandle,
    shell: State<'_, Shell>,
    path: String,
) -> Result<String, String> {
    let p = PathBuf::from(&path);
    if !shell.is_granted(&p) {
        return Err(format!("{path} is outside the folder you opened."));
    }
    // Belt and braces: the protocol handler checks this too, but failing here
    // gives the UI an error string instead of a silent 403 in the console.
    if !app.asset_protocol_scope().is_allowed(&p) {
        return Err(format!("{path} is not readable by the player."));
    }
    Ok(format!(
        "asset://localhost/{}",
        utf8_percent_encode(&path, URI_COMPONENT)
    ))
}

/// The picture cap. A 12 MB hi-res cover would cross IPC as a ~16 MB base64
/// string and be decoded into a 132px frame; above this the honest answer is
/// that Pelican is not going to carry it.
const COVER_CAP: usize = 2 * 1024 * 1024;

/// Read one file's embedded cover art. Answers as `CoverArt`.
///
/// **Not inline**, per the module note: this is file I/O, a sync command runs
/// on the macOS main thread, and a FLAC with a large picture would freeze the
/// window for the length of the read. The scope check is the only thing that
/// happens here; the parse goes to a short-lived worker, exactly as
/// `scan_folder` sends its walk to one.
///
/// `read_fast` is deliberately left alone — it passes `read_cover_art(false)`
/// because a library scan pays the picture's parse cost per file and never
/// uses it. This is the other case: one file, at the moment its art is wanted.
#[tauri::command]
pub fn cover_art(app: AppHandle, shell: State<'_, Shell>, path: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    if !shell.is_granted(&p) {
        return Err(format!("{path} is outside the folder you opened."));
    }
    if !app.asset_protocol_scope().is_allowed(&p) {
        return Err(format!("{path} is not readable by the player."));
    }
    let sink = shell.sink.clone();
    std::thread::Builder::new()
        .name("pelican-cover".into())
        .spawn(move || {
            let art = pelican_core::transcode::tags::read_cover(&p)
                .ok()
                .flatten()
                .filter(|(_, bytes)| bytes.len() <= COVER_CAP)
                .map(|(mime, bytes)| format!("data:{mime};base64,{}", base64(&bytes)));
            // Always answers, including with `None`. A request that produced
            // no event would leave the caller waiting on a promise that never
            // settles, and it would never learn the file simply has no art.
            sink.send(UiEvent::CoverArt { path, art });
        })
        .map_err(|e| format!("could not read cover art: {e}"))?;
    Ok(())
}

/// Standard base64, hand-rolled.
///
/// Sixteen lines against a new crate in a dependency tree this project
/// deliberately keeps small enough for `cargo-deny` to be meaningful. The
/// alphabet and the padding rules are RFC 4648 §4 and are covered by tests.
fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Allow the asset protocol to serve `root`, and remember it for the
/// path checks the commands above perform.
pub fn grant(app: &AppHandle, shell: State<'_, Shell>, root: &Path) -> Result<(), String> {
    app.asset_protocol_scope()
        .allow_directory(root, true)
        .map_err(|e| format!("granting access to {}: {e}", root.display()))?;
    shell.remember_root(root.to_path_buf());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_extension_survives_url_encoding() {
        // If this ever produces "%2Eflac", FLAC playback dies with
        // MediaError 4 and the cause is invisible from the media element.
        let enc = utf8_percent_encode("/Users/x/My Music/tr#1.flac", URI_COMPONENT).to_string();
        assert!(enc.ends_with(".flac"), "{enc}");
        assert!(!enc.contains('/'), "path separators must be encoded: {enc}");
        assert!(!enc.contains(' '), "spaces must be encoded: {enc}");
        assert!(
            !enc.contains('#'),
            "a fragment marker would truncate: {enc}"
        );
    }

    /// RFC 4648 §4 test vectors. The `data:` URL an embedded cover crosses
    /// IPC as is built from this, and a wrong pad byte is a picture that
    /// silently fails to decode in the webview.
    #[test]
    fn base64_matches_the_rfc_vectors() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    /// The two high bytes exercise the '+' and '/' end of the alphabet, which
    /// a JPEG's binary payload will hit within the first few bytes.
    #[test]
    fn base64_covers_the_whole_alphabet() {
        assert_eq!(base64(&[0xff, 0xef, 0xbe]), "/+++");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
        assert_eq!(base64(&[0xff, 0xd8, 0xff]), "/9j/");
    }

    #[test]
    fn unicode_paths_encode_as_utf8() {
        let enc = utf8_percent_encode("/x/Café.mp3", URI_COMPONENT).to_string();
        assert_eq!(enc, "%2Fx%2FCaf%C3%A9.mp3");
    }
}
