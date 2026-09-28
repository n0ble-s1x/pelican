//! Pelican's desktop window.
//!
//! A Tauri 2 shell over `pelican-core`, serving the checked-in `ui/`
//! frontend: plain HTML, CSS and JS, with no npm, no bundler and no
//! `node_modules`, so the whole dependency surface stays inside what
//! `cargo deny` and `cargo audit` can see. There is no network either (no
//! telemetry, no updater, no remote font), and the CSP in `tauri.conf.json`
//! makes that structural rather than a promise.
//!
//! `commands.rs` is the IPC surface and says why no command does its work
//! on the thread it is called on; `device.rs` is the one-session-at-a-time
//! lock; `dto.rs` is the contract's shapes; `run.rs` is a push; `env.rs` is the NVIDIA-on-Wayland
//! workaround set before the webview exists; `udev.rs` is the one
//! privileged action, installing the USB rule through polkit.
//!
//! Run it with `cargo run -p pelican-shell`, **not** `cargo tauri dev`: a
//! dev server is served over http with no CSP, so developing against it
//! would leave the no-network guarantee untested every day.

mod commands;
mod config;
mod device;
mod dto;
mod env;
mod run;
mod udev;

use std::path::PathBuf;
use std::sync::Arc;

use tauri::Manager;

/// What every command shares.
#[derive(Default)]
pub struct Shell {
    pub watch: device::Watch,
    /// `None` when there is no absolute `HOME` or `XDG_CONFIG_HOME`: the
    /// library root is then not remembered, and setting one says why.
    pub config_file: Option<PathBuf>,
}

fn main() {
    // Before anything creates GTK or the webview: they read it at startup.
    env::apply_nvidia_wayland_workaround();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::status,
            commands::library_list,
            commands::places,
            commands::preview,
            commands::push,
            commands::stop,
            commands::watch_list,
            commands::ledger,
            commands::default_backup_dir,
            commands::backup_watch,
            commands::reset_check,
            commands::reset_ledger,
            commands::udev_rule_status,
            commands::install_udev_rule,
        ])
        .setup(|app| {
            app.manage(Arc::new(Shell {
                watch: device::Watch::default(),
                config_file: config::file(),
            }));
            // Staging left by a run that was killed. A directory walk, so
            // not on the thread running the event loop.
            std::thread::Builder::new()
                .name("pelican-sweep".into())
                .spawn(pelican_core::staging::sweep)?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("running Pelican");
}
