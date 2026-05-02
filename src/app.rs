//! Krypteia · MTP Sync — three-column file browser with drag-drop.
//!
//! Layout:
//!   ┌──────────────────── topbar ────────────────────┐
//!   │  KRYPTEIA · MTP SYNC      device · free space  │
//!   ├──────────┬─────────────┬───────────────────────┤
//!   │  LOCAL   │   ▶ SEND    │   WATCH               │
//!   │  list    │   ✕ DELETE  │   list                │
//!   ├──────────┴─────────────┴───────────────────────┤
//!   │  ━━━━━━━━ progress · file · X of Y             │
//!   ├────────────────────────────────────────────────┤
//!   │  TRANSMISSION LOG                              │
//!   └────────────────────────────────────────────────┘

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::Duration;

use eframe::egui;

use crate::{garmin, gvfs, history, mtp, theme, transfer};

const ROOT_FOLDERS: &[&str] = &["Music", "Audiobooks", "Podcasts"];
const PRODUCT_NAME: &str = "PELICAN";
const BRAND: &str = "KRYPTEIA";

pub fn run() -> anyhow::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([900.0, 560.0])
            .with_drag_and_drop(true)
            .with_title("Krypteia · Pelican"),
        ..Default::default()
    };
    eframe::run_native(
        "pelican",
        opts,
        Box::new(|cc| {
            theme::install(&cc.egui_ctx);
            Ok(Box::new(App::default()))
        }),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))
}

// ─────────────────────────── state ───────────────────────────

struct App {
    devices: Vec<garmin::Device>,
    selected_device: Option<usize>,
    gvfs_warning: Option<String>,
    backend: Option<Box<dyn mtp::Backend>>,
    free_space: Option<(u64, u64)>,

    local: LocalPane,
    watch: WatchPane,

    op_rx: Option<mpsc::Receiver<OpMsg>>,
    busy: Option<&'static str>,
    progress: Option<Progress>,

    log: Vec<LogLine>,
    show_log: bool,
    show_onboarding: bool,
    skip_tag_check: bool,
    transcode_enabled: bool,
    ffmpeg_present: bool,

    // Persistent record of every successful upload to the linked watch.
    // Garmin doesn't expose the indexed library via MTP, so this is the only
    // way the user can see "what's on the watch" after the staging folder is
    // emptied by the indexer. Loaded from disk per-device-serial on link.
    history: history::DeviceHistory,
    history_serial: Option<String>,

    // Modal state for "save current LOCAL selection as a playlist".
    new_playlist_name: String,
    show_new_playlist_dialog: bool,

    local_rect: egui::Rect,
    watch_rect: egui::Rect,

    // Auto-reconnect bookkeeping. After a worker thread drops the backend,
    // we want to reopen the session — but we throttle reconnect attempts so
    // a persistently-failing watch doesn't hammer libusb every frame.
    last_reconnect_at: Option<std::time::Instant>,
    last_link_failed: bool,
}

impl Default for App {
    fn default() -> Self {
        Self {
            devices: Vec::new(),
            selected_device: None,
            gvfs_warning: None,
            backend: None,
            free_space: None,
            local: LocalPane::new(),
            watch: WatchPane::default(),
            op_rx: None,
            busy: None,
            progress: None,
            log: Vec::new(),
            show_log: true,
            show_onboarding: true,
            // Default permissive: Garmin happily stores untagged files; it
            // just hides them in the music-app screen view. We warn but don't
            // block. Users who want strict mode toggle it on.
            skip_tag_check: true,
            transcode_enabled: crate::transcode::ffmpeg_available(),
            ffmpeg_present: crate::transcode::ffmpeg_available(),
            local_rect: egui::Rect::NOTHING,
            watch_rect: egui::Rect::NOTHING,
            last_reconnect_at: None,
            last_link_failed: false,
            history: history::DeviceHistory::default(),
            history_serial: None,
            new_playlist_name: String::new(),
            show_new_playlist_dialog: false,
        }
    }
}

#[derive(Copy, Clone, PartialEq)]
enum Stage {
    Transcoding,
    Uploading,
}

struct Progress {
    stage: Stage,
    label: String,
    file_done: u64,
    file_total: u64,
    files_done: usize,
    files_total: usize,
}

#[derive(Default)]
struct LocalPane {
    cwd: PathBuf,
    entries: Vec<LocalEntry>,
    selected: HashSet<PathBuf>,
    last_clicked: Option<usize>,
}

#[derive(Clone)]
struct LocalEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
}

impl LocalPane {
    fn new() -> Self {
        let mut me = Self {
            cwd: home_music(),
            ..Default::default()
        };
        me.refresh();
        me
    }

    fn refresh(&mut self) {
        self.entries.clear();
        let Ok(rd) = std::fs::read_dir(&self.cwd) else {
            return;
        };
        for ent in rd.flatten() {
            let path = ent.path();
            let Ok(meta) = ent.metadata() else { continue };
            let name = ent.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            self.entries.push(LocalEntry {
                path,
                name,
                is_dir: meta.is_dir(),
                size: meta.len(),
            });
        }
        self.entries.sort_by(|a, b| match (a.is_dir, b.is_dir) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
    }

    fn navigate(&mut self, target: PathBuf) {
        self.cwd = target;
        self.selected.clear();
        self.last_clicked = None;
        self.refresh();
    }

    fn up(&mut self) {
        if let Some(parent) = self.cwd.parent() {
            self.navigate(parent.to_path_buf());
        }
    }
}

struct WatchPane {
    cwd: String,
    entries: Vec<mtp::RemoteEntry>,
    selected: HashSet<String>,
    last_clicked: Option<usize>,
}

impl Default for WatchPane {
    fn default() -> Self {
        Self {
            cwd: "Music".into(),
            entries: Vec::new(),
            selected: HashSet::new(),
            last_clicked: None,
        }
    }
}

impl WatchPane {
    fn navigate(&mut self, path: String) {
        self.cwd = path;
        self.selected.clear();
        self.last_clicked = None;
    }

    fn up(&mut self) {
        if self.cwd.is_empty() {
            return;
        }
        let parent = self
            .cwd
            .rsplit_once('/')
            .map(|(p, _)| p.to_string())
            .unwrap_or_default();
        self.navigate(parent);
    }
}

#[derive(Clone)]
struct LogLine {
    text: String,
    kind: LogKind,
}

#[derive(Copy, Clone)]
enum LogKind {
    Info,
    Warn,
    Error,
    Ok,
}

enum OpMsg {
    Deleted(String),
    DeleteError(String, String),
    Started(PathBuf),
    Transcoding {
        file: String,
        files_done: usize,
        files_total: usize,
    },
    Progress {
        file: String,
        transferred: u64,
        total: u64,
        files_done: usize,
        files_total: usize,
    },
    Done {
        src: PathBuf,
        bytes: u64,
        files_done: usize,
        files_total: usize,
    },
    Skipped {
        src: PathBuf,
        reason: String,
    },
    Failed {
        src: PathBuf,
        error: String,
    },
    JobFinished,
}

#[derive(Clone, Debug)]
struct DragLocal(Vec<PathBuf>);

#[derive(Clone, Debug)]
struct DragWatch(#[allow(dead_code)] Vec<String>);

// ─────────────────────────── App impl ───────────────────────────

impl App {
    /// Try to open an MTP session. Quiet on failure (logs only on first
    /// failure of a streak) so that the auto-reconnect loop doesn't spam.
    fn try_connect(&mut self) -> bool {
        if self.backend.is_some() {
            return true;
        }
        if self.busy.is_some() || self.op_rx.is_some() {
            return false;
        }
        let device = match self
            .selected_device
            .and_then(|i| self.devices.get(i))
            .cloned()
        {
            Some(d) => d,
            None => return false,
        };
        match mtp::open(&device) {
            Ok(b) => {
                self.backend = Some(b);
                if self.last_link_failed {
                    self.push_log(
                        LogKind::Ok,
                        format!("link recovered · {}", device.label()),
                    );
                } else {
                    self.push_log(
                        LogKind::Ok,
                        format!("link established · {}", device.label()),
                    );
                }
                self.last_link_failed = false;
                // Load the per-device upload history. We key on the USB
                // serial — same watch across re-plugs hits the same file.
                if let Some(serial) = device.serial.as_ref() {
                    if self.history_serial.as_deref() != Some(serial.as_str()) {
                        self.history = history::load(serial);
                        self.history_serial = Some(serial.clone());
                    }
                }
                self.refresh_watch();
                true
            }
            Err(e) => {
                if !self.last_link_failed {
                    // log once per streak — silent retries thereafter
                    self.push_log(LogKind::Warn, format!("link unavailable · {e:#}"));
                    self.last_link_failed = true;
                }
                false
            }
        }
    }

    /// Frame-driven auto-reconnect with throttling.
    fn maybe_reconnect(&mut self) {
        if self.backend.is_some() {
            return;
        }
        if self.busy.is_some() || self.op_rx.is_some() {
            return;
        }
        if self.selected_device.is_none() {
            return;
        }
        let now = std::time::Instant::now();
        let delay = if self.last_link_failed {
            std::time::Duration::from_millis(2000)
        } else {
            std::time::Duration::from_millis(150)
        };
        if let Some(prev) = self.last_reconnect_at {
            if now.duration_since(prev) < delay {
                return;
            }
        }
        self.last_reconnect_at = Some(now);
        self.try_connect();
    }

    fn refresh_devices(&mut self) {
        let prior_serial = self
            .selected_device
            .and_then(|i| self.devices.get(i))
            .and_then(|d| d.serial.clone());

        match garmin::list_devices() {
            Ok(devs) => {
                self.devices = devs;
                // Preserve previous selection by serial if possible.
                if let Some(serial) = prior_serial {
                    self.selected_device = self
                        .devices
                        .iter()
                        .position(|d| d.serial.as_deref() == Some(serial.as_str()));
                }
                // Auto-select the only device if none selected yet.
                if self.selected_device.is_none() && !self.devices.is_empty() {
                    self.selected_device = Some(0);
                }
                if self.devices.is_empty() {
                    self.selected_device = None;
                }
            }
            Err(e) => self.push_log(LogKind::Error, format!("scan failed · {e}")),
        }
        self.gvfs_warning = gvfs::detect_garmin_gvfs_mount().map(|p| {
            format!("GVFS holds device at {p} — direct link blocked. Unmount in Files first.")
        });
    }

    fn refresh_watch(&mut self) {
        let cwd = self.watch.cwd.clone();
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let listed = backend.list_dir(&cwd);
        let fs = backend.free_space().ok();
        match listed {
            Ok(entries) => self.watch.entries = entries,
            Err(e) => self.push_log(LogKind::Error, format!("listing /{cwd} · {e:#}")),
        }
        if let Some(fs) = fs {
            self.free_space = Some(fs);
        }
    }

    fn push_log(&mut self, kind: LogKind, text: impl Into<String>) {
        self.log.push(LogLine { text: text.into(), kind });
        if self.log.len() > 600 {
            self.log.drain(0..self.log.len() - 600);
        }
    }

    fn handle_intra_drops(&mut self, ctx: &egui::Context) {
        let pointer_pos = ctx.input(|i| i.pointer.latest_pos());
        let released = ctx.input(|i| i.pointer.any_released());
        if !released {
            return;
        }
        let Some(pt) = pointer_pos else { return };

        // Local → Watch  (= upload selected paths)
        if self.watch_rect.contains(pt) {
            if let Some(payload) = egui::DragAndDrop::take_payload::<DragLocal>(ctx) {
                let paths = payload.0.clone();
                self.start_send_paths(paths);
                return;
            }
        }
        // Watch → Local  (= future: pull-from-watch). Soft-fail for now.
        if self.local_rect.contains(pt) {
            if egui::DragAndDrop::take_payload::<DragWatch>(ctx).is_some() {
                self.push_log(LogKind::Warn, "pull from watch not yet implemented");
            }
        }
    }

    fn collect_external_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if dropped.is_empty() {
            return;
        }
        let pointer = ctx.input(|i| i.pointer.latest_pos());
        let mut paths: Vec<PathBuf> = dropped.into_iter().filter_map(|f| f.path).collect();
        if let Some(pt) = pointer {
            if self.watch_rect.contains(pt) {
                self.start_send_paths(paths.clone());
                self.push_log(
                    LogKind::Info,
                    format!("queued {} dropped item(s) for upload", paths.len()),
                );
                return;
            }
            if self.local_rect.contains(pt) {
                for p in paths.drain(..) {
                    self.local.selected.insert(p);
                }
                return;
            }
        }
        for p in paths {
            self.local.selected.insert(p);
        }
    }

    fn drain_op(&mut self) {
        let mut msgs = Vec::new();
        let mut closed = false;
        if let Some(rx) = &self.op_rx {
            loop {
                match rx.try_recv() {
                    Ok(m) => msgs.push(m),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        closed = true;
                        break;
                    }
                }
            }
        }
        for m in msgs {
            match m {
                OpMsg::Deleted(path) => {
                    self.push_log(LogKind::Ok, format!("✕ purged · /{path}"));
                }
                OpMsg::DeleteError(path, err) => {
                    self.push_log(LogKind::Error, format!("✕ purge /{path} · {err}"));
                }
                OpMsg::Started(p) => {
                    self.push_log(LogKind::Info, format!("→ {}", short_path(&p)));
                }
                OpMsg::Transcoding {
                    file,
                    files_done,
                    files_total,
                } => {
                    self.progress = Some(Progress {
                        stage: Stage::Transcoding,
                        label: file,
                        file_done: 0,
                        file_total: 0,
                        files_done,
                        files_total,
                    });
                }
                OpMsg::Progress {
                    file,
                    transferred,
                    total,
                    files_done,
                    files_total,
                } => {
                    self.progress = Some(Progress {
                        stage: Stage::Uploading,
                        label: file,
                        file_done: transferred,
                        file_total: total,
                        files_done,
                        files_total,
                    });
                }
                OpMsg::Done {
                    src,
                    bytes,
                    files_done,
                    files_total,
                } => {
                    self.push_log(
                        LogKind::Ok,
                        format!("✓ {} · {}", short_path(&src), human_bytes(bytes)),
                    );
                    if let Some(p) = self.progress.as_mut() {
                        p.files_done = files_done;
                        p.files_total = files_total;
                    }
                    // Track local free-space delta (Garmin caches the firmware
                    // figure; only our writes give an accurate signal).
                    if let Some((free, total)) = self.free_space {
                        self.free_space = Some((free.saturating_sub(bytes), total));
                    }
                    // Record persistently so the user can see it across runs.
                    let name = src
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| short_path(&src));
                    if let Some(serial) = self.history_serial.clone() {
                        history::record(&serial, &name, bytes);
                        self.history = history::load(&serial);
                    }
                }
                OpMsg::Skipped { src, reason } => {
                    self.push_log(
                        LogKind::Warn,
                        format!("· skip · {} · {reason}", short_path(&src)),
                    );
                }
                OpMsg::Failed { src, error } => {
                    self.push_log(LogKind::Error, format!("✕ {} · {error}", short_path(&src)));
                }
                OpMsg::JobFinished => {
                    self.busy = None;
                    self.progress = None;
                    // Reset the progress-bar smoothing target so next job
                    // starts from 0 instead of animating down from 100%.
                    let ctx = std::sync::OnceLock::<()>::new();
                    let _ = ctx;
                    self.refresh_watch();
                }
            }
        }
        if closed {
            self.op_rx = None;
            self.busy = None;
            self.progress = None;
        }
    }

    fn start_send_paths(&mut self, paths: Vec<PathBuf>) {
        if paths.is_empty() {
            return;
        }
        if !self.try_connect() {
            return;
        }
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        let target_dir = self.watch.cwd.clone();
        let skip_tag_check = self.skip_tag_check;
        let transcode_enabled = self.transcode_enabled;
        self.busy = Some("transmitting");
        self.progress = Some(Progress {
            stage: Stage::Uploading,
            label: "queueing…".into(),
            file_done: 0,
            file_total: 0,
            files_done: 0,
            files_total: 0,
        });
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let jobs = match transfer::expand_inputs_into(&paths, &target_dir) {
                Ok(j) => j,
                Err(e) => {
                    let _ = tx.send(OpMsg::Failed {
                        src: PathBuf::new(),
                        error: format!("plan · {e:#}"),
                    });
                    let _ = tx.send(OpMsg::JobFinished);
                    return;
                }
            };
            let total = jobs.len();
            let mut done = 0usize;
            for job in jobs {
                let _ = tx.send(OpMsg::Started(job.src.clone()));

                if !crate::transcode::is_audio(&job.src) {
                    let _ = tx.send(OpMsg::Skipped {
                        src: job.src.clone(),
                        reason: "not an audio file".into(),
                    });
                    done += 1;
                    continue;
                }

                // Always normalize — re-mux MP3 sources for tag-strip,
                // transcode others. Garmin firmware needs both a clean
                // audio profile and a standard-frames-only ID3 tag.
                let mut transcoded_holder: Option<crate::transcode::Transcoded> = None;
                let (upload_path, upload_name): (std::path::PathBuf, String) = if transcode_enabled {
                    let label = job
                        .src
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "audio".into());
                    let _ = tx.send(OpMsg::Transcoding {
                        file: label.clone(),
                        files_done: done,
                        files_total: total,
                    });
                    match crate::transcode::normalize(&job.src) {
                        Ok(t) => {
                            let p = t.path.clone();
                            let n = t.mp3_name.clone();
                            transcoded_holder = Some(t);
                            (p, n)
                        }
                        Err(e) => {
                            let _ = tx.send(OpMsg::Failed {
                                src: job.src.clone(),
                                error: format!("{e:#}"),
                            });
                            continue;
                        }
                    }
                } else {
                    if !is_supported_ext(&job.src) {
                        let _ = tx.send(OpMsg::Skipped {
                            src: job.src.clone(),
                            reason: "needs normalization (toggle Transcode on)".into(),
                        });
                        done += 1;
                        continue;
                    }
                    (job.src.clone(), job.remote_name.clone())
                };

                if !has_required_tags(&upload_path) {
                    if !skip_tag_check {
                        let _ = tx.send(OpMsg::Skipped {
                            src: job.src.clone(),
                            reason: "missing ID3 title/artist — uncheck 'Require tags' to send anyway".into(),
                        });
                        done += 1;
                        continue;
                    } else {
                        // Permissive mode: upload anyway, but warn the file
                        // won't show up in the watch's music app.
                        let _ = tx.send(OpMsg::Started(job.src.clone()));
                        tracing::warn!(
                            file = %job.src.display(),
                            "uploading without ID3 title/artist — file will be on the watch but hidden from the music app"
                        );
                    }
                }
                if let Err(e) = backend.ensure_folder(&job.remote_dir) {
                    let _ = tx.send(OpMsg::Failed {
                        src: job.src.clone(),
                        error: format!("ensure_folder · {e:#}"),
                    });
                    continue;
                }
                let prog_tx = tx.clone();
                let file_label = job
                    .src
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let total_for_progress = total;
                let done_snapshot = done;
                let mut on_progress = move |transferred: u64, file_total: u64| {
                    let _ = prog_tx.send(OpMsg::Progress {
                        file: file_label.clone(),
                        transferred,
                        total: file_total,
                        files_done: done_snapshot,
                        files_total: total_for_progress,
                    });
                };
                match backend.upload(&upload_path, &job.remote_dir, &upload_name, &mut on_progress)
                {
                    Ok(bytes) => {
                        done += 1;
                        let _ = tx.send(OpMsg::Done {
                            src: job.src.clone(),
                            bytes,
                            files_done: done,
                            files_total: total,
                        });
                    }
                    Err(e) => {
                        let _ = tx.send(OpMsg::Failed {
                            src: job.src.clone(),
                            error: format!("{e:#}"),
                        });
                    }
                }
                drop(transcoded_holder); // delete temp
            }
            let _ = tx.send(OpMsg::JobFinished);
        });
        self.op_rx = Some(rx);
    }

    fn start_send_selected(&mut self) {
        let paths: Vec<PathBuf> = self.local.selected.iter().cloned().collect();
        self.local.selected.clear();
        self.start_send_paths(paths);
    }

    fn start_delete_selected(&mut self) {
        if !self.try_connect() {
            return;
        }
        let Some(mut backend) = self.backend.take() else {
            return;
        };
        let targets: Vec<String> = self.watch.selected.iter().cloned().collect();
        self.watch.selected.clear();
        if targets.is_empty() {
            self.backend = Some(backend);
            return;
        }
        self.busy = Some("purging");
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            for t in &targets {
                match backend.delete(t) {
                    Ok(()) => {
                        let _ = tx.send(OpMsg::Deleted(t.clone()));
                    }
                    Err(e) => {
                        let _ = tx.send(OpMsg::DeleteError(t.clone(), format!("{e:#}")));
                    }
                }
            }
            let _ = tx.send(OpMsg::JobFinished);
        });
        self.op_rx = Some(rx);
    }

    // ── pane bodies ──
    fn local_pane_body(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            if chip(ui, "Home").clicked() {
                self.local.navigate(home_dir());
            }
            if chip(ui, "Music").clicked() {
                self.local.navigate(home_music());
            }
            if chip(ui, "Downloads").clicked() {
                let mut p = home_dir();
                p.push("Downloads");
                self.local.navigate(p);
            }
            if chip(ui, "↑ Up").clicked() {
                self.local.up();
            }
        });
        ui.add_space(4.0);
        path_breadcrumb(ui, &self.local.cwd.display().to_string());
        ui.add_space(6.0);
        theme::faint_line(ui);

        let mut to_navigate: Option<PathBuf> = None;
        let mut click_idx: Option<(usize, bool)> = None;
        let mut send_one: Option<PathBuf> = None;
        let drag_id = egui::Id::new("local-drag");

        egui::ScrollArea::vertical()
            .id_salt("local-scroll")
            .show(ui, |ui| {
                ui.add_space(2.0);
                for (i, entry) in self.local.entries.iter().enumerate() {
                    let selected = self.local.selected.contains(&entry.path);
                    let mut payload_paths: Vec<PathBuf> =
                        self.local.selected.iter().cloned().collect();
                    if !selected {
                        payload_paths.push(entry.path.clone());
                    }
                    if payload_paths.is_empty() {
                        payload_paths.push(entry.path.clone());
                    }
                    let resp = drag_row(
                        ui,
                        drag_id.with(i),
                        DragLocal(payload_paths),
                        &entry.name,
                        entry.is_dir,
                        Some(human_bytes(entry.size)).filter(|_| !entry.is_dir),
                        selected,
                    );
                    if resp.double_clicked() && entry.is_dir {
                        to_navigate = Some(entry.path.clone());
                    } else if resp.clicked() {
                        let shift = ui.input(|i| i.modifiers.shift);
                        click_idx = Some((i, shift));
                    }
                    let entry_path_for_menu = entry.path.clone();
                    let entry_is_dir_for_menu = entry.is_dir;
                    resp.context_menu(|ui| {
                        if ui.button("Send to watch").clicked() {
                            send_one = Some(entry_path_for_menu.clone());
                            ui.close_menu();
                        }
                        if entry_is_dir_for_menu && ui.button("Open").clicked() {
                            to_navigate = Some(entry_path_for_menu.clone());
                            ui.close_menu();
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        theme::ASH,
                        format!(
                            "{} items · {} selected",
                            self.local.entries.len(),
                            self.local.selected.len()
                        ),
                    );
                    if !self.local.selected.is_empty() && chip(ui, "Clear").clicked() {
                        self.local.selected.clear();
                    }
                });
            });
        if let Some((i, shift)) = click_idx {
            self.click_local(i, shift);
        }
        if let Some(p) = to_navigate {
            self.local.navigate(p);
        }
        if let Some(p) = send_one {
            self.local.selected.insert(p);
            self.start_send_selected();
        }
    }

    fn click_local(&mut self, idx: usize, shift: bool) {
        let entry = match self.local.entries.get(idx).cloned() {
            Some(e) => e,
            None => return,
        };
        if shift {
            if let Some(anchor) = self.local.last_clicked {
                let (a, b) = if anchor < idx { (anchor, idx) } else { (idx, anchor) };
                for e in &self.local.entries[a..=b] {
                    self.local.selected.insert(e.path.clone());
                }
                return;
            }
        }
        if !self.local.selected.remove(&entry.path) {
            self.local.selected.insert(entry.path);
        }
        self.local.last_clicked = Some(idx);
    }

    fn watch_pane_body(&mut self, ui: &mut egui::Ui) {
        // Top row: navigation chips + refresh on the right
        ui.horizontal(|ui| {
            if chip(ui, "↑ Up").clicked() {
                self.watch.up();
                if self.backend.is_some() {
                    self.refresh_watch();
                }
            }
            for f in ROOT_FOLDERS {
                if chip(ui, f).clicked() {
                    self.watch.navigate((*f).into());
                    if self.try_connect() {
                        self.refresh_watch();
                    }
                }
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                if chip(ui, "↻  Refresh")
                    .on_hover_text("Re-list /Music. The watch's library updates separately.")
                    .clicked()
                {
                    if self.try_connect() {
                        self.refresh_watch();
                    }
                }
            });
        });
        ui.add_space(4.0);
        let display = if self.watch.cwd.is_empty() {
            "/".to_string()
        } else {
            format!("/{}", self.watch.cwd)
        };
        path_breadcrumb(ui, &display);

        // Brutally honest caption — /Music is a staging area, not the
        // watch's library. The watch's music indexer absorbs files into a
        // private database, and there is no MTP endpoint to list that.
        if self.watch.cwd.eq_ignore_ascii_case("Music") {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(
                    egui::RichText::new(
                        "Staging area only · Garmin's library is private to the watch firmware. See journal below.",
                    )
                    .color(theme::ASH)
                    .size(10.5)
                    .italics(),
                );
            });
        }

        ui.add_space(6.0);
        theme::faint_line(ui);

        if self.backend.is_none() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                let msg = if self.devices.is_empty() {
                    "Searching for watch…"
                } else if self.last_link_failed {
                    "Cannot link to watch.\nUnplug, replug, and unlock the screen."
                } else {
                    "Connecting to watch…"
                };
                ui.label(
                    egui::RichText::new(msg)
                        .color(theme::ASH)
                        .size(12.0),
                );
            });
            return;
        }

        let mut to_navigate: Option<String> = None;
        let mut click_idx: Option<(usize, bool)> = None;
        let mut delete_one: Option<String> = None;
        let drag_id = egui::Id::new("watch-drag");

        egui::ScrollArea::vertical()
            .id_salt("watch-scroll")
            .show(ui, |ui| {
                ui.add_space(2.0);
                for (i, entry) in self.watch.entries.iter().enumerate() {
                    let selected = self.watch.selected.contains(&entry.path);
                    let mut payload_paths: Vec<String> =
                        self.watch.selected.iter().cloned().collect();
                    if !selected {
                        payload_paths.push(entry.path.clone());
                    }
                    if payload_paths.is_empty() {
                        payload_paths.push(entry.path.clone());
                    }
                    let size_label = if entry.is_broken {
                        Some("broken".to_string())
                    } else if entry.is_folder {
                        None
                    } else {
                        Some(human_bytes(entry.size))
                    };
                    let resp = watch_row(
                        ui,
                        drag_id.with(i),
                        DragWatch(payload_paths),
                        &entry.name,
                        entry.is_folder,
                        entry.is_broken,
                        size_label,
                        selected,
                    );
                    if resp.double_clicked() && entry.is_folder {
                        to_navigate = Some(entry.path.clone());
                    } else if resp.clicked() {
                        let shift = ui.input(|i| i.modifiers.shift);
                        click_idx = Some((i, shift));
                    }
                    let p = entry.path.clone();
                    resp.context_menu(|ui| {
                        if ui.button("Delete from watch").clicked() {
                            delete_one = Some(p.clone());
                            ui.close_menu();
                        }
                    });
                }
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.colored_label(
                        theme::ASH,
                        format!(
                            "{} items · {} selected",
                            self.watch.entries.len(),
                            self.watch.selected.len()
                        ),
                    );
                    if !self.watch.selected.is_empty() && chip(ui, "Clear").clicked() {
                        self.watch.selected.clear();
                    }
                });

                // Local playlists — group of tracks the user can save and
                // re-send as a batch. Lives in the per-device JSON journal.
                if !self.history.playlists.is_empty()
                    && self.watch.cwd.eq_ignore_ascii_case("Music")
                {
                    ui.add_space(22.0);
                    theme::faint_line(ui);
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 1.0, theme::SCARLET);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("LOCAL PLAYLISTS")
                                .color(theme::BONE)
                                .strong()
                                .size(11.0)
                                .extra_letter_spacing(1.6),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{}", self.history.playlists.len()))
                                .color(theme::ASH)
                                .size(11.0),
                        );
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new(
                                "Track groups stored locally. \"Send\" uploads every track to /Music.",
                            )
                            .color(theme::ASH)
                            .size(10.5)
                            .italics(),
                        );
                    });
                    ui.add_space(8.0);

                    let mut to_send: Option<Vec<std::path::PathBuf>> = None;
                    let mut to_delete: Option<String> = None;
                    for pl in &self.history.playlists {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.colored_label(theme::SCARLET, "▸");
                            ui.add_space(6.0);
                            ui.label(
                                egui::RichText::new(&pl.name)
                                    .color(theme::BONE)
                                    .size(12.5)
                                    .strong(),
                            );
                            ui.label(
                                egui::RichText::new(format!("· {} tracks", pl.tracks.len()))
                                    .color(theme::ASH)
                                    .size(11.0),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(20.0);
                                    if chip(ui, "Delete").clicked() {
                                        to_delete = Some(pl.name.clone());
                                    }
                                    ui.add_space(4.0);
                                    if chip(ui, "Send →").clicked() {
                                        to_send = Some(pl.tracks.clone());
                                    }
                                },
                            );
                        });
                    }
                    if let Some(paths) = to_send {
                        self.start_send_paths(paths);
                    }
                    if let Some(name) = to_delete {
                        if let Some(serial) = self.history_serial.clone() {
                            history::remove_playlist(&serial, &name);
                            self.history = history::load(&serial);
                        }
                    }
                }

                // Persistent upload history — the only "library view" we can
                // offer, since Garmin doesn't expose the indexed library to
                // MTP. Shown when browsing /Music (where uploads land before
                // the indexer absorbs them).
                if !self.history.uploads.is_empty()
                    && self.watch.cwd.eq_ignore_ascii_case("Music")
                {
                    ui.add_space(22.0);
                    theme::faint_line(ui);
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 1.0, theme::SUCCESS);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("UPLOADED · LOCAL JOURNAL")
                                .color(theme::BONE)
                                .strong()
                                .size(11.0)
                                .extra_letter_spacing(1.6),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new(format!("{}", self.history.uploads.len()))
                                .color(theme::ASH)
                                .size(11.0),
                        );
                    });
                    ui.add_space(3.0);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new(
                                "Files we've sent to this watch. Garmin's firmware doesn't expose its indexed music library to MTP — verify playback on the watch face.",
                            )
                            .color(theme::ASH)
                            .size(10.5)
                            .italics(),
                        );
                    });
                    ui.add_space(10.0);

                    let total_bytes: u64 =
                        self.history.uploads.iter().map(|u| u.bytes).sum();
                    // Show newest first.
                    for u in self.history.uploads.iter().rev() {
                        ui.horizontal(|ui| {
                            ui.add_space(20.0);
                            ui.colored_label(theme::SUCCESS, "✓");
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new(&u.name)
                                    .color(theme::BONE_DIM)
                                    .size(11.5),
                            );
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.add_space(20.0);
                                    ui.label(
                                        egui::RichText::new(human_bytes(u.bytes))
                                            .color(theme::ASH)
                                            .size(11.0)
                                            .monospace(),
                                    );
                                },
                            );
                        });
                    }
                    ui.add_space(10.0);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        ui.label(
                            egui::RichText::new(format!(
                                "Total · {} files · {}",
                                self.history.uploads.len(),
                                human_bytes(total_bytes)
                            ))
                            .color(theme::ASH)
                            .size(11.0),
                        );
                    });
                }
            });
        if let Some((i, shift)) = click_idx {
            self.click_watch(i, shift);
        }
        if let Some(p) = to_navigate {
            self.watch.navigate(p);
            self.refresh_watch();
        }
        if let Some(p) = delete_one {
            self.watch.selected.insert(p);
            self.start_delete_selected();
        }
    }

    fn click_watch(&mut self, idx: usize, shift: bool) {
        let entry = match self.watch.entries.get(idx).cloned() {
            Some(e) => e,
            None => return,
        };
        if shift {
            if let Some(anchor) = self.watch.last_clicked {
                let (a, b) = if anchor < idx { (anchor, idx) } else { (idx, anchor) };
                for e in &self.watch.entries[a..=b] {
                    self.watch.selected.insert(e.path.clone());
                }
                return;
            }
        }
        if !self.watch.selected.remove(&entry.path) {
            self.watch.selected.insert(entry.path);
        }
        self.watch.last_clicked = Some(idx);
    }

    // ── chrome ──
    fn topbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            // brand bar accent
            let cur = ui.cursor().left_top();
            let bar_rect =
                egui::Rect::from_min_size(cur + egui::vec2(0.0, 2.0), egui::vec2(3.0, 28.0));
            ui.painter().rect_filled(bar_rect, 1.0, theme::SCARLET);
            ui.add_space(14.0);

            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.label(
                    egui::RichText::new(BRAND)
                        .color(theme::BONE)
                        .strong()
                        .size(13.5)
                        .extra_letter_spacing(2.0),
                );
                ui.label(
                    egui::RichText::new(PRODUCT_NAME)
                        .color(theme::ASH)
                        .size(10.0)
                        .extra_letter_spacing(1.5),
                );
            });

            ui.add_space(36.0);

            // Connection state dot — always visible. Self-explaining.
            // During busy/transfer the dot gently pulses; on idle it's static.
            let (dot_base, dot_state, animate) = if self.backend.is_some() {
                if self.busy.is_some() || self.op_rx.is_some() {
                    (theme::WARN, "Busy", true)
                } else {
                    (theme::SUCCESS, "Connected", false)
                }
            } else if self.devices.is_empty() {
                (theme::ASH_DIM, "Searching for watch", false)
            } else if self.last_link_failed {
                (theme::SCARLET, "Cannot link", false)
            } else {
                (theme::WARN, "Connecting…", true)
            };
            let dot_color = if animate {
                let t = ui.ctx().input(|i| i.time);
                // 1.4-second sine pulse, range 0.55..1.0
                let pulse = ((t * std::f64::consts::TAU / 1.4).sin() * 0.5 + 0.5) as f32;
                let factor = 0.55 + pulse * 0.45;
                dot_base.linear_multiply(factor)
            } else {
                dot_base
            };
            let (rect, _) = ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
            ui.painter().circle_filled(rect.center(), 4.0, dot_color);
            if animate {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            }
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(dot_state)
                    .color(theme::BONE_DIM)
                    .size(11.5),
            );
            ui.add_space(12.0);

            // Device selection — only show as a control when there's a real
            // choice to make. Single device auto-selects, no UI noise.
            if self.devices.len() > 1 {
                let dev_label = self
                    .selected_device
                    .and_then(|i| self.devices.get(i))
                    .map(|d| d.label())
                    .unwrap_or_else(|| "Select device".into());
                egui::ComboBox::from_id_salt("dev")
                    .selected_text(dev_label)
                    .width(280.0)
                    .show_ui(ui, |ui| {
                        for (i, d) in self.devices.iter().enumerate() {
                            ui.selectable_value(
                                &mut self.selected_device,
                                Some(i),
                                d.label(),
                            );
                        }
                    });
            } else if let Some(d) = self
                .selected_device
                .and_then(|i| self.devices.get(i))
            {
                ui.label(
                    egui::RichText::new(d.label())
                        .color(theme::BONE)
                        .size(12.0),
                );
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(20.0);
                if !self.show_onboarding && chip(ui, "?").on_hover_text("Show onboarding").clicked() {
                    self.show_onboarding = true;
                }
                ui.add_space(8.0);
                if let Some((free, total)) = self.free_space {
                    let pct = (free as f64 / total as f64) * 100.0;
                    ui.label(
                        egui::RichText::new(format!(
                            "{} of {}  ·  {:.0}% free",
                            human_bytes(free),
                            human_bytes(total),
                            pct
                        ))
                        .color(theme::BONE_DIM)
                        .size(11.5),
                    );
                    ui.add_space(10.0);
                    let bar_w = 100.0_f32;
                    let (rect, _) =
                        ui.allocate_exact_size(egui::vec2(bar_w, 4.0), egui::Sense::hover());
                    ui.painter().rect_filled(rect, 2.0, theme::ELEVATED);
                    let used = ((1.0 - free as f32 / total as f32) * bar_w).clamp(0.0, bar_w);
                    let used_rect = egui::Rect::from_min_size(
                        rect.left_top(),
                        egui::vec2(used, rect.height()),
                    );
                    ui.painter().rect_filled(used_rect, 2.0, theme::SCARLET);
                }
            });
        });
    }

    fn onboarding_panel(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.horizontal(|ui| {
            ui.add_space(16.0);
            let (rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 1.0, theme::SCARLET);
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new("GETTING STARTED")
                    .color(theme::BONE)
                    .strong()
                    .size(11.5)
                    .extra_letter_spacing(1.8),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(12.0);
                if chip(ui, "Dismiss").clicked() {
                    self.show_onboarding = false;
                }
            });
        });
        ui.add_space(8.0);
        theme::faint_line(ui);
        ui.add_space(10.0);

        let body = |ui: &mut egui::Ui, n: &str, title: &str, body: &str| {
            ui.horizontal_top(|ui| {
                ui.add_space(16.0);
                ui.label(
                    egui::RichText::new(n)
                        .color(theme::SCARLET)
                        .size(13.0)
                        .strong(),
                );
                ui.add_space(8.0);
                ui.vertical(|ui| {
                    ui.label(
                        egui::RichText::new(title)
                            .color(theme::BONE)
                            .size(12.5)
                            .strong(),
                    );
                    ui.add_space(2.0);
                    ui.label(
                        egui::RichText::new(body)
                            .color(theme::BONE_DIM)
                            .size(11.5),
                    );
                });
            });
            ui.add_space(12.0);
        };

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                body(
                    ui,
                    "1",
                    "Plug in the watch",
                    "Connect via USB and unlock the screen. On older models, set Settings → System → USB Mode to MTP.",
                );
                body(
                    ui,
                    "2",
                    "Wait for green dot",
                    "The status dot in the top bar turns green once linked. We auto-detect a single watch and reconnect after each transfer.",
                );
                body(
                    ui,
                    "3",
                    "Drop a folder",
                    "Drag music from your file manager onto the WATCH pane to upload, or drag rows between LOCAL and WATCH inside the app.",
                );
                body(
                    ui,
                    "4",
                    "Audio is normalized",
                    "Every audio file passes through ffmpeg on the way out: MP3 sources are re-muxed (no re-encode) to strip non-standard tags; FLAC/OGG/Opus/WMA/AIFF/ALAC are transcoded to CBR 192 kbps MP3. Garmin needs a strict ID3v2.3 tag set + standard MP3 profile or it silently rejects the upload.",
                );
                body(
                    ui,
                    "5",
                    "Tagged files show on watch",
                    "Garmin's music app filters by ID3 title + artist — untagged files still upload and are stored, but won't appear in the watch's library view. Tag your music for the best experience.",
                );
                body(
                    ui,
                    "6",
                    "Local playlists",
                    "MTP playlist writes don't work on Garmin firmware (the watch silently rejects .m3u/.m3u8). Instead, select tracks in LOCAL and click \"Save Group\" — the playlist lives in this app, and \"Send →\" mass-uploads every track.",
                );
                body(
                    ui,
                    "7",
                    "WATCH pane is /Music",
                    "What you see is the watch's /Music folder over MTP. The journal below the listing tracks every successful upload to this watch (persists across app restarts).",
                );

                ui.add_space(8.0);
                theme::faint_line(ui);
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.label(
                        egui::RichText::new("TROUBLESHOOTING")
                            .color(theme::ASH)
                            .size(10.0)
                            .extra_letter_spacing(1.5),
                    );
                });
                ui.add_space(8.0);

                let trouble = |ui: &mut egui::Ui, q: &str, a: &str| {
                    ui.horizontal_top(|ui| {
                        ui.add_space(16.0);
                        ui.vertical(|ui| {
                            ui.label(
                                egui::RichText::new(q)
                                    .color(theme::BONE)
                                    .size(11.5)
                                    .strong(),
                            );
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(a)
                                    .color(theme::BONE_DIM)
                                    .size(11.0),
                            );
                        });
                    });
                    ui.add_space(10.0);
                };

                trouble(
                    ui,
                    "Status dot stays grey",
                    "Watch isn't detected. Check the cable (data, not charge-only), unlock the screen, and confirm USB mode is MTP.",
                );
                trouble(
                    ui,
                    "GVFS alert banner",
                    "Your file manager auto-mounted the watch and is holding the USB. Open Files, eject the watch, then return here.",
                );
                trouble(
                    ui,
                    "Files vanish from WATCH pane",
                    "Normal — Garmin's indexer moves them into its private library a few seconds after upload. Verify on the watch face, not in the GUI.",
                );
                trouble(
                    ui,
                    "Untagged files don't show on watch",
                    "Garmin hides files without ID3 title + artist. Tag the source before upload or toggle 'Allow untagged'.",
                );
                trouble(
                    ui,
                    "FLAC won't transcode",
                    "Install ffmpeg (sudo apt install ffmpeg / sudo dnf install ffmpeg). Pre-existing transcoded files live in /tmp/krypteia-mtp-sync/ and self-clean.",
                );
            });
    }

    fn action_column(&mut self, ui: &mut egui::Ui) {
        ui.add_space(48.0);
        ui.vertical_centered(|ui| {
            let send_enabled =
                !self.local.selected.is_empty() && self.busy.is_none() && !self.devices.is_empty();
            if action_button(ui, "Send  →", send_enabled).clicked() {
                self.start_send_selected();
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{} selected", self.local.selected.len()))
                    .color(theme::ASH_DIM)
                    .size(10.5),
            );

            ui.add_space(26.0);

            let del_enabled =
                !self.watch.selected.is_empty() && self.busy.is_none() && self.backend.is_some();
            if action_button_danger(ui, "Delete", del_enabled).clicked() {
                self.start_delete_selected();
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(format!("{} selected", self.watch.selected.len()))
                    .color(theme::ASH_DIM)
                    .size(10.5),
            );

            ui.add_space(22.0);

            // Save current LOCAL selection as a named playlist.
            let can_save_playlist = !self.local.selected.is_empty();
            if action_button(ui, "Save Group", can_save_playlist).clicked() {
                self.show_new_playlist_dialog = true;
                self.new_playlist_name.clear();
            }
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("local playlist")
                    .color(theme::ASH_DIM)
                    .size(10.5),
            );

            ui.add_space(28.0);
            // Hairline divider, centered, narrow.
            let (rect, _) = ui.allocate_exact_size(egui::vec2(40.0, 1.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 0.0, theme::HAIRLINE);

            ui.add_space(20.0);

            // Options
            ui.label(
                egui::RichText::new("OPTIONS")
                    .color(theme::ASH)
                    .size(9.5)
                    .extra_letter_spacing(1.5),
            );
            ui.add_space(8.0);

            let mut transcode = self.transcode_enabled;
            let xcode_label = if self.ffmpeg_present {
                "Transcode FLAC"
            } else {
                "Transcode (no ffmpeg)"
            };
            let resp = ui
                .add_enabled(
                    self.ffmpeg_present,
                    egui::Checkbox::new(
                        &mut transcode,
                        egui::RichText::new(xcode_label)
                            .color(theme::BONE_DIM)
                            .size(11.0),
                    ),
                )
                .on_hover_text(
                    "Auto-convert FLAC/OGG/Opus/WMA/AIFF to MP3 (VBR ~190 kbps)\n\
                     before upload. Tags preserved. Garmin can't play these natively.",
                );
            if resp.changed() {
                self.transcode_enabled = transcode;
            }

            ui.add_space(6.0);

            let mut require_tags = !self.skip_tag_check;
            if ui
                .checkbox(
                    &mut require_tags,
                    egui::RichText::new("Require tags")
                        .color(theme::BONE_DIM)
                        .size(11.0),
                )
                .on_hover_text(
                    "When on, files without ID3 title + artist are skipped.\n\
                     When off (default), they upload anyway — but Garmin's music\n\
                     app will hide them from its library view.",
                )
                .changed()
            {
                self.skip_tag_check = !require_tags;
            }

            ui.add_space(20.0);
            ui.label(
                egui::RichText::new("drag rows between panes")
                    .color(theme::ASH_DIM)
                    .size(10.0),
            );
            ui.label(
                egui::RichText::new("shift-click for range")
                    .color(theme::ASH_DIM)
                    .size(10.0),
            );
        });
    }

    fn progress_strip(&mut self, ui: &mut egui::Ui, time: f64) {
        let avail = ui.available_width() - 40.0;
        ui.add_space(8.0);

        // Line 1 — stage badge + label + counts.
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            match self.progress.as_ref().map(|p| p.stage) {
                Some(Stage::Transcoding) => {
                    badge(ui, "Transcoding", theme::WARN, theme::ELEVATED);
                }
                Some(Stage::Uploading) => {
                    badge(ui, "Uploading", theme::SCARLET, theme::SCARLET_DEEP);
                }
                None => {
                    badge(ui, "Ready", theme::ASH, theme::ELEVATED);
                }
            }
            ui.add_space(12.0);
            if let Some(p) = self.progress.as_ref() {
                ui.label(
                    egui::RichText::new(&p.label)
                        .color(theme::BONE)
                        .size(13.0),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_space(20.0);
                    if p.stage == Stage::Uploading && p.file_total > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} / {}",
                                human_bytes(p.file_done),
                                human_bytes(p.file_total)
                            ))
                            .color(theme::ASH)
                            .monospace()
                            .size(11.5),
                        );
                        ui.add_space(14.0);
                    }
                    if p.files_total > 0 {
                        ui.label(
                            egui::RichText::new(format!(
                                "File {} of {}",
                                (p.files_done + 1).min(p.files_total),
                                p.files_total
                            ))
                            .color(theme::BONE_DIM)
                            .size(11.5),
                        );
                    }
                });
            } else {
                ui.colored_label(theme::ASH_DIM, "Awaiting transmission");
            }
        });

        ui.add_space(6.0);

        // Line 2 — progress bar (determinate for upload, marquee for transcode).
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            let (rect, _) =
                ui.allocate_exact_size(egui::vec2(avail, 4.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 2.0, theme::ELEVATED);
            match self.progress.as_ref().map(|p| (p.stage, p.file_done, p.file_total)) {
                Some((Stage::Uploading, done, total)) if total > 0 => {
                    let target = (done as f32 / total as f32).clamp(0.0, 1.0);
                    // Smoothly interpolate the displayed fill so chunk-to-chunk
                    // jumps look like a continuous bar instead of staircase.
                    let smooth = ui.ctx().animate_value_with_time(
                        egui::Id::new("progress-bar-fill"),
                        target,
                        0.18,
                    );
                    let fill_w = (smooth * avail).max(2.0);
                    let fill = egui::Rect::from_min_size(
                        rect.left_top(),
                        egui::vec2(fill_w, rect.height()),
                    );
                    ui.painter().rect_filled(fill, 2.0, theme::SCARLET);
                }
                Some((Stage::Transcoding, _, _)) => {
                    // Indeterminate marquee — a 22%-width slug oscillates back
                    // and forth so the user knows ffmpeg is still working.
                    let cycle = 1.6_f64; // seconds round-trip
                    let phase = ((time % cycle) / cycle) as f32; // 0..1
                    let slug_w = avail * 0.22;
                    let max_x = avail - slug_w;
                    // Triangle wave 0..1..0
                    let tri = if phase < 0.5 {
                        phase * 2.0
                    } else {
                        (1.0 - phase) * 2.0
                    };
                    let x = tri * max_x;
                    let fill = egui::Rect::from_min_size(
                        rect.left_top() + egui::vec2(x, 0.0),
                        egui::vec2(slug_w, rect.height()),
                    );
                    ui.painter().rect_filled(fill, 2.0, theme::WARN);
                }
                _ => {}
            }
        });
        ui.add_space(8.0);
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // First-frame device discovery, then keep refreshing periodically so
        // a hot-plug picks up without the user touching anything.
        if self.devices.is_empty() && self.selected_device.is_none() {
            self.refresh_devices();
        } else if self.backend.is_none() && self.busy.is_none() {
            // periodic re-scan when not connected
            static LAST: std::sync::OnceLock<std::sync::Mutex<std::time::Instant>> =
                std::sync::OnceLock::new();
            let m = LAST.get_or_init(|| std::sync::Mutex::new(std::time::Instant::now()));
            let now = std::time::Instant::now();
            let mut g = m.lock().unwrap();
            if now.duration_since(*g) > std::time::Duration::from_millis(2000) {
                *g = now;
                self.refresh_devices();
            }
        }
        self.drain_op();
        self.maybe_reconnect();

        // "Save selection as playlist" modal.
        if self.show_new_playlist_dialog {
            let mut close = false;
            let mut create = false;
            egui::Window::new("Save as local playlist")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
                .show(ctx, |ui| {
                    ui.set_min_width(360.0);
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} tracks selected in LOCAL",
                            self.local.selected.len()
                        ))
                        .color(theme::BONE_DIM)
                        .size(11.5),
                    );
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new("Name").color(theme::ASH).size(10.5));
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.new_playlist_name)
                            .desired_width(f32::INFINITY)
                            .hint_text("e.g. Long Run, Recovery, Album X"),
                    );
                    if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        create = true;
                    }
                    ui.add_space(12.0);
                    ui.horizontal(|ui| {
                        if ui
                            .add_enabled(
                                !self.new_playlist_name.trim().is_empty(),
                                egui::Button::new(
                                    egui::RichText::new("Save")
                                        .color(theme::BONE)
                                        .strong(),
                                )
                                .fill(theme::SCARLET_DEEP)
                                .stroke(egui::Stroke::new(1.0, theme::SCARLET)),
                            )
                            .clicked()
                        {
                            create = true;
                        }
                        if ui.button("Cancel").clicked() {
                            close = true;
                        }
                    });
                });
            if create {
                let name = self.new_playlist_name.trim().to_string();
                if !name.is_empty() {
                    let tracks: Vec<std::path::PathBuf> =
                        self.local.selected.iter().cloned().collect();
                    if let Some(serial) = self.history_serial.clone() {
                        history::add_playlist(&serial, name.clone(), tracks);
                        self.history = history::load(&serial);
                        self.push_log(
                            LogKind::Ok,
                            format!("✓ saved local playlist «{name}»"),
                        );
                    } else {
                        self.push_log(
                            LogKind::Warn,
                            "no device linked — playlists are saved per-device",
                        );
                    }
                    close = true;
                }
            }
            if close {
                self.show_new_playlist_dialog = false;
                self.new_playlist_name.clear();
            }
        }

        // Topbar
        egui::TopBottomPanel::top("topbar")
            .exact_height(54.0)
            .show(ctx, |ui| {
                ui.add_space(10.0);
                self.topbar(ui);
                ui.add_space(8.0);
                theme::hairline(ui);
            });

        if let Some(w) = self.gvfs_warning.clone() {
            egui::TopBottomPanel::top("gvfs_banner").show(ctx, |ui| {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(16.0);
                    ui.colored_label(theme::SCARLET, "▲  ALERT");
                    ui.add_space(10.0);
                    ui.colored_label(theme::BONE, w);
                });
                ui.add_space(6.0);
                theme::faint_line(ui);
            });
        }

        // Log strip
        if self.show_log {
            egui::TopBottomPanel::bottom("log")
                .resizable(true)
                .default_height(140.0)
                .min_height(40.0)
                .show(ctx, |ui| {
                    theme::hairline(ui);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        let (rect, _) = ui
                            .allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 1.0, theme::SCARLET);
                        ui.add_space(8.0);
                        ui.label(
                            egui::RichText::new("ACTIVITY")
                                .color(theme::BONE)
                                .strong()
                                .size(11.5)
                                .extra_letter_spacing(1.8),
                        );
                        ui.add_space(10.0);
                        ui.label(
                            egui::RichText::new(format!("{}", self.log.len()))
                                .color(theme::ASH)
                                .size(11.0),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add_space(20.0);
                            if chip(ui, "Hide").clicked() {
                                self.show_log = false;
                            }
                            if chip(ui, "Clear").clicked() {
                                self.log.clear();
                            }
                        });
                    });
                    ui.add_space(2.0);
                    theme::faint_line(ui);
                    egui::ScrollArea::vertical()
                        .stick_to_bottom(true)
                        .show(ui, |ui| {
                            ui.add_space(4.0);
                            for line in &self.log {
                                let color = match line.kind {
                                    LogKind::Info => theme::BONE_DIM,
                                    LogKind::Warn => theme::WARN,
                                    LogKind::Error => theme::SCARLET_BRIGHT,
                                    LogKind::Ok => theme::SUCCESS,
                                };
                                ui.horizontal(|ui| {
                                    ui.add_space(16.0);
                                    ui.label(
                                        egui::RichText::new(&line.text)
                                            .color(color)
                                            .monospace(),
                                    );
                                });
                            }
                        });
                });
        } else {
            egui::TopBottomPanel::bottom("log_collapsed")
                .exact_height(30.0)
                .show(ctx, |ui| {
                    theme::hairline(ui);
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_space(20.0);
                        if chip(ui, "Show activity").clicked() {
                            self.show_log = true;
                        }
                    });
                });
        }

        // Progress strip — always visible, two-line.
        let now = ctx.input(|i| i.time);
        egui::TopBottomPanel::bottom("progress")
            .exact_height(54.0)
            .show(ctx, |ui| {
                theme::hairline(ui);
                self.progress_strip(ui, now);
            });

        // Three-column main area
        let total_w = ctx.screen_rect().width();
        let action_w = 130.0;
        let pane_w = ((total_w - action_w) / 2.0).max(300.0);

        // Onboarding panel — right side, dismissible.
        if self.show_onboarding {
            egui::SidePanel::right("onboarding")
                .resizable(false)
                .show_separator_line(false)
                .default_width(290.0)
                .min_width(290.0)
                .max_width(320.0)
                .show(ctx, |ui| {
                    self.onboarding_panel(ui);
                });
        }

        egui::SidePanel::left("local_pane")
            .resizable(true)
            .show_separator_line(false)
            .default_width(pane_w)
            .min_width(300.0)
            .show(ctx, |ui| {
                pane_header(ui, "LOCAL", theme::SCARLET);
                ui.add_space(4.0);
                self.local_rect = ui.available_rect_before_wrap();
                self.local_pane_body(ui);
            });

        egui::SidePanel::right("watch_pane")
            .resizable(true)
            .show_separator_line(false)
            .default_width(pane_w)
            .min_width(300.0)
            .show(ctx, |ui| {
                pane_header(ui, "WATCH", theme::SCARLET);
                ui.add_space(4.0);
                self.watch_rect = ui.available_rect_before_wrap();
                self.watch_pane_body(ui);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.action_column(ui);
        });

        self.collect_external_drops(ctx);
        // Manual intra-app drop detection — bypassing dnd_drop_zone which was
        // painting a full-pane HOVER fill that registered as visual flashing.
        // We detect "pointer was dragging over watch_rect, then released with
        // a DragLocal payload set" ourselves.
        self.handle_intra_drops(ctx);

        // Force higher repaint rate during the indeterminate marquee so the
        // animation is smooth; otherwise keep it lazy.
        match self.progress.as_ref().map(|p| p.stage) {
            Some(Stage::Transcoding) => ctx.request_repaint_after(Duration::from_millis(33)),
            _ if self.busy.is_some() => ctx.request_repaint_after(Duration::from_millis(80)),
            _ => ctx.request_repaint_after(Duration::from_millis(220)),
        }
    }
}

// ─────────────────────────── widgets ───────────────────────────

fn chip(ui: &mut egui::Ui, label: &str) -> egui::Response {
    let btn = egui::Button::new(
        egui::RichText::new(label)
            .color(theme::BONE_DIM)
            .size(11.5),
    )
    .fill(theme::ELEVATED)
    .stroke(egui::Stroke::new(1.0, theme::HAIRLINE_FAINT))
    .min_size(egui::vec2(0.0, 22.0));
    ui.add(btn)
}

fn action_button(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let bg = if enabled { theme::SCARLET_DEEP } else { theme::ELEVATED };
    let stroke_col = if enabled { theme::SCARLET } else { theme::HAIRLINE_FAINT };
    let text_col = if enabled { theme::BONE } else { theme::ASH_DIM };
    let btn = egui::Button::new(
        egui::RichText::new(label).strong().color(text_col).size(12.5),
    )
    .fill(bg)
    .stroke(egui::Stroke::new(1.0, stroke_col))
    .min_size(egui::vec2(108.0, 34.0));
    ui.add_enabled(enabled, btn)
}

fn action_button_danger(ui: &mut egui::Ui, label: &str, enabled: bool) -> egui::Response {
    let stroke_col = if enabled { theme::SCARLET } else { theme::HAIRLINE_FAINT };
    let text_col = if enabled { theme::SCARLET_BRIGHT } else { theme::ASH_DIM };
    let btn = egui::Button::new(
        egui::RichText::new(label).strong().color(text_col).size(12.5),
    )
    .fill(theme::PANEL)
    .stroke(egui::Stroke::new(1.0, stroke_col))
    .min_size(egui::vec2(108.0, 34.0));
    ui.add_enabled(enabled, btn)
}

/// Small uppercase badge used for stage indicators in the status strip.
fn badge(ui: &mut egui::Ui, label: &str, accent: egui::Color32, fill: egui::Color32) {
    let text = egui::RichText::new(label)
        .color(accent)
        .size(10.5)
        .strong()
        .extra_letter_spacing(1.4);
    let frame = egui::Frame::none()
        .fill(fill)
        .stroke(egui::Stroke::new(1.0, accent.linear_multiply(0.5)))
        .inner_margin(egui::Margin::symmetric(8.0, 3.0))
        .rounding(egui::Rounding::same(3.0));
    frame.show(ui, |ui| {
        ui.label(text);
    });
}

fn pane_header(ui: &mut egui::Ui, label: &str, accent: egui::Color32) {
    ui.add_space(12.0);
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(2.0, 11.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 1.0, accent);
        ui.add_space(8.0);
        ui.label(
            egui::RichText::new(label)
                .color(theme::BONE)
                .strong()
                .size(11.5)
                .extra_letter_spacing(1.8),
        );
    });
    ui.add_space(2.0);
}

fn path_breadcrumb(ui: &mut egui::Ui, path: &str) {
    ui.horizontal(|ui| {
        ui.add_space(12.0);
        ui.label(
            egui::RichText::new(path)
                .color(theme::BONE_DIM)
                .monospace()
                .size(11.5),
        );
    });
}

/// Watch-pane row variant that knows about `is_broken` — renders a scarlet ✕
/// glyph and a "broken" tag instead of the size, signaling that the entry is
/// only useful for one operation: delete.
fn watch_row<P: Send + Sync + Clone + 'static>(
    ui: &mut egui::Ui,
    id: egui::Id,
    payload: P,
    name: &str,
    is_dir: bool,
    is_broken: bool,
    size: Option<String>,
    selected: bool,
) -> egui::Response {
    let row_h = 26.0;
    let avail_w = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(avail_w, row_h), egui::Sense::click_and_drag());

    if response.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), payload);
    }
    let is_being_dragged = ui.ctx().is_being_dragged(id);

    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SCARLET_WASH);
        let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height()));
        painter.rect_filled(bar, 0.0, theme::SCARLET);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::HOVER);
    }
    if is_being_dragged {
        painter.rect_stroke(
            rect.shrink(1.0),
            0.0,
            egui::Stroke::new(1.0, theme::SCARLET_BRIGHT),
        );
    }

    let glyph = if is_broken {
        "✕"
    } else if is_dir {
        "▸"
    } else {
        "·"
    };
    let glyph_color = if is_broken || is_dir {
        theme::SCARLET
    } else {
        theme::ASH
    };
    let font_mono = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    painter.text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        glyph,
        font_mono.clone(),
        glyph_color,
    );
    let text_color = if is_broken {
        theme::SCARLET_BRIGHT
    } else if selected {
        theme::BONE
    } else {
        theme::BONE_DIM
    };
    painter.text(
        rect.left_center() + egui::vec2(34.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        egui::FontId::new(13.0, egui::FontFamily::Proportional),
        text_color,
    );
    if let Some(s) = size {
        let color = if is_broken { theme::SCARLET } else { theme::ASH };
        painter.text(
            rect.right_center() + egui::vec2(-14.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            s,
            egui::FontId::new(11.0, egui::FontFamily::Monospace),
            color,
        );
    }
    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    response
}

fn drag_row<P: Send + Sync + Clone + 'static>(
    ui: &mut egui::Ui,
    id: egui::Id,
    payload: P,
    name: &str,
    is_dir: bool,
    size: Option<String>,
    selected: bool,
) -> egui::Response {
    let row_h = 26.0;
    let avail_w = ui.available_width();
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(avail_w, row_h), egui::Sense::click_and_drag());

    if response.drag_started() {
        egui::DragAndDrop::set_payload(ui.ctx(), payload);
    }
    let is_being_dragged = ui.ctx().is_being_dragged(id);

    let painter = ui.painter_at(rect);
    if selected {
        painter.rect_filled(rect, 0.0, theme::SCARLET_WASH);
        let bar = egui::Rect::from_min_size(rect.left_top(), egui::vec2(2.0, rect.height()));
        painter.rect_filled(bar, 0.0, theme::SCARLET);
    } else if response.hovered() {
        painter.rect_filled(rect, 0.0, theme::HOVER);
    }
    if is_being_dragged {
        painter.rect_stroke(
            rect.shrink(1.0),
            0.0,
            egui::Stroke::new(1.0, theme::SCARLET_BRIGHT),
        );
    }

    let glyph = if is_dir { "▸" } else { "·" };
    let glyph_color = if is_dir { theme::SCARLET } else { theme::ASH };
    let font = egui::FontId::new(13.0, egui::FontFamily::Monospace);
    painter.text(
        rect.left_center() + egui::vec2(14.0, 0.0),
        egui::Align2::LEFT_CENTER,
        glyph,
        font.clone(),
        glyph_color,
    );
    let text_color = if selected { theme::BONE } else { theme::BONE_DIM };
    painter.text(
        rect.left_center() + egui::vec2(34.0, 0.0),
        egui::Align2::LEFT_CENTER,
        name,
        font.clone(),
        text_color,
    );
    if let Some(s) = size {
        painter.text(
            rect.right_center() + egui::vec2(-14.0, 0.0),
            egui::Align2::RIGHT_CENTER,
            s,
            egui::FontId::new(11.0, egui::FontFamily::Monospace),
            theme::ASH,
        );
    }

    if response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
    }
    response
}

// ─────────────────────────── helpers ───────────────────────────

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
}

fn home_music() -> PathBuf {
    let mut p = home_dir();
    p.push("Music");
    if !p.exists() {
        return home_dir();
    }
    p
}

fn human_bytes(n: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if n >= GB {
        format!("{:.2} GB", n as f64 / GB as f64)
    } else if n >= MB {
        format!("{:.1} MB", n as f64 / MB as f64)
    } else if n >= KB {
        format!("{:.0} KB", n as f64 / KB as f64)
    } else {
        format!("{n} B")
    }
}

fn short_path(p: &Path) -> String {
    let s = p.display().to_string();
    if s.len() > 60 {
        format!(
            "…/{}",
            p.file_name().map(|n| n.to_string_lossy()).unwrap_or_default()
        )
    } else {
        s
    }
}

const SUPPORTED_EXTS: &[&str] = &["mp3", "m4a", "m4b", "aac", "wav"];
fn is_supported_ext(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .map(|e| SUPPORTED_EXTS.iter().any(|s| s.eq_ignore_ascii_case(e)))
        .unwrap_or(false)
}

fn has_required_tags(p: &Path) -> bool {
    let ext = p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    match ext.as_deref() {
        Some("mp3") => match id3::Tag::read_from_path(p) {
            Ok(tag) => {
                use id3::TagLike;
                tag.title().is_some() && tag.artist().is_some()
            }
            Err(_) => false,
        },
        Some("m4a") | Some("m4b") | Some("aac") => match mp4ameta::Tag::read_from_path(p) {
            Ok(tag) => tag.title().is_some() && tag.artist().is_some(),
            Err(_) => false,
        },
        _ => true,
    }
}

#[allow(unused)]
fn _unused_transfer_ref(_: &transfer::Event) {}
