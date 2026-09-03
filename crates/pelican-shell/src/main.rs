//! Pelican's macOS shell.
//!
//! A Tauri window over `pelican-core`, serving the checked-in `ui/` frontend.
//! There is no npm, no bundler and no node_modules: the frontend is vanilla
//! HTML, CSS and JS, so the whole dependency surface stays inside the one
//! `cargo-deny` can audit. There is also no network — no telemetry, no
//! updater, no remote font, no CDN — and the CSP in `tauri.conf.json` is
//! written to make that structural rather than a promise.
//!
//! The threading model is the load-bearing part; see `commands.rs` for why
//! no command body may do work, and `device.rs` for how "one MTP session at
//! a time" is made impossible to violate rather than merely documented.
//!
//! Run it with `cargo run -p pelican-shell`, **not** `cargo tauri dev`. The
//! CLI's dev server serves over http and sets no CSP header at all — Tauri
//! only injects the policy on the `tauri://` custom protocol — so developing
//! against it would leave the no-network guarantee untested every day.

mod commands;
mod config;
mod device;
mod dto;
mod scan;
mod watch;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::Manager;

use pelican_core::transcode::encoder::{self, Encoder};

use crate::device::DeviceHandle;
use crate::dto::EventSink;
use crate::watch::WatchHandle;

/// Everything the command layer needs, and nothing it can block on.
pub struct Shell {
    pub sink: Arc<EventSink>,
    pub device: DeviceHandle,
    pub watch: WatchHandle,
    /// Probed once, at startup.
    ///
    /// `encoder::plan` re-probes per call, which means spawning `ffmpeg
    /// -version` for every file in a library scan — and on a machine without
    /// ffmpeg, failing to spawn it every time. The answer cannot change
    /// while the app runs in any way worth tracking.
    pub encoders: Vec<Encoder>,
    /// Folders the user has opened. Canonical, absolute.
    roots: Mutex<Vec<PathBuf>>,
}

impl Shell {
    fn remember_root(&self, root: PathBuf) {
        let mut roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        if !roots.contains(&root) {
            roots.push(root);
        }
    }

    /// True when `p` lives under a folder the user opened.
    ///
    /// Canonicalize before comparing, for the same reason `Scope::is_allowed`
    /// does: a symlink out of the library is a path that looks inside it and
    /// is not. A path that cannot be resolved — deleted, or a dangling link —
    /// is denied rather than guessed at.
    pub fn is_granted(&self, p: &Path) -> bool {
        let Ok(real) = p.canonicalize() else {
            return false;
        };
        let roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        roots.iter().any(|r| real.starts_with(r))
    }

    /// The remembered library root, if it is still there.
    pub fn library_root(&self) -> Option<PathBuf> {
        let roots = self.roots.lock().unwrap_or_else(|e| e.into_inner());
        roots.first().cloned()
    }
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::subscribe,
            commands::connect,
            commands::disconnect,
            commands::delete_remote,
            commands::forget_uploads,
            commands::pick_folder,
            commands::scan_folder,
            commands::start_sync,
            commands::stop_sync,
            commands::set_now_playing,
            commands::cover_art,
        ])
        .setup(|app| {
            let sink = Arc::new(EventSink::default());
            let shell = Shell {
                device: device::spawn(sink.clone()),
                watch: watch::spawn(sink.clone()),
                encoders: encoder::available(),
                roots: Mutex::new(Vec::new()),
                sink,
            };

            // Re-apply last session's grant. The scope itself is never
            // persisted — a serialized capability sitting in a file is a
            // worse thing to own than a path the user can delete — so it is
            // rebuilt here from a folder the user chose, and only if that
            // folder is still where they left it.
            if let Some(root) = config::load().library_root {
                if root.is_dir() {
                    match app.asset_protocol_scope().allow_directory(&root, true) {
                        Ok(()) => shell.remember_root(root),
                        Err(e) => eprintln!("could not restore library access: {e}"),
                    }
                }
            }

            app.manage(shell);

            // Stale conversions from previous runs. A filesystem walk, so it
            // does not belong on the thread running the event loop.
            std::thread::Builder::new()
                .name("pelican-sweep".into())
                .spawn(|| pelican_core::transcode::sweep(Duration::from_secs(24 * 3600)))
                .expect("spawning the cache sweep");

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("running Pelican");
}
