//! MTP backend abstraction.
//!
//! Implementations sit behind a trait so we can swap mtp-rs for libmtp-rs
//! (or a hand-rolled PTP path) on any device that needs it. The trait is
//! deliberately small — copy a local file to a folder on the device — and
//! we resist generalizing further until a second backend lands.

use std::path::Path;

use anyhow::Result;

use crate::garmin::Device;

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_folder: bool,
    /// True if `GetObjectInfo` failed for this handle — we know the handle
    /// exists on the device but can't read its metadata. Almost always a
    /// broken stub from a previous partial / rejected upload. Caller should
    /// render it differently and offer delete-only operations.
    pub is_broken: bool,
}

/// How far an upload got before it failed, attached to the error as typed
/// context so callers can word the failure by phase instead of guessing from
/// a string.
///
/// `sent == len` means the whole file crossed the wire and the failure was in
/// the PTP *response* phase — the object is quite possibly on the watch, and a
/// UI that flatly reports "failed" there is making a claim about the device it
/// cannot support. `sent < len` means the data phase itself was cut short, and
/// whatever is on the device is not the file.
#[derive(Debug, Clone)]
pub struct UploadPhase {
    pub local: String,
    pub sent: u64,
    pub len: u64,
}

impl UploadPhase {
    /// True when every byte was handed to the transport before the error.
    /// A zero-length file counts: there was nothing left to send.
    pub fn drained(&self) -> bool {
        self.sent >= self.len
    }
}

impl std::fmt::Display for UploadPhase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "uploading {} (streamed {} of {} bytes)",
            self.local, self.sent, self.len
        )
    }
}

pub trait Backend: Send {
    /// The device's own model name, from MTP `GetDeviceInfo`.
    ///
    /// The FR165 declares no USB product string descriptor (`iProduct = 0`),
    /// so `nusb::DeviceInfo::product_string()` has nothing to return and the
    /// real name — "Forerunner 165 Music" — exists only here. `GetDeviceInfo`
    /// is fetched and cached during `open`, so this costs no device I/O.
    ///
    /// `None` when the device gives no model, which keeps the caller's
    /// fallback chain honest rather than inventing a name.
    fn model(&self) -> Option<String> {
        None
    }
    fn ensure_folder(&mut self, path: &str) -> Result<()>;
    /// Upload a local file to a remote folder. `on_progress(bytes_transferred, total_bytes)`
    /// is called as data flows; pass `&mut |_, _| {}` if you don't care.
    fn upload(
        &mut self,
        local: &Path,
        remote_dir: &str,
        remote_name: &str,
        on_progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<u64>;
    fn remote_size(&mut self, remote_dir: &str, remote_name: &str) -> Result<Option<u64>>;
    fn list_dir(&mut self, path: &str) -> Result<Vec<RemoteEntry>>;
    fn delete(&mut self, path: &str) -> Result<()>;
    fn free_space(&mut self) -> Result<(u64, u64)>;
    /// Download a small remote file to a Vec. Used for playlists (.m3u8).
    fn download_file(&mut self, path: &str) -> Result<Vec<u8>>;
    /// Write raw bytes as a new file in `remote_dir` — no transcode, no
    /// path manipulation. Caller controls the exact filename. Used for
    /// playlist files.
    fn write_raw(&mut self, remote_dir: &str, remote_name: &str, bytes: &[u8]) -> Result<()>;
}

#[cfg(feature = "mtp-backend")]
pub fn open(device: &Device) -> Result<Box<dyn Backend>> {
    match mtp_rs_impl::MtpRsBackend::open(device) {
        Ok(b) => Ok(Box::new(b)),
        // "could not open interface for exclusive access" tells the user
        // nothing they can act on. Attach the platform's list of likely
        // causes, led by the one that is almost always right: a session we
        // ourselves still have open.
        Err(e) if is_exclusive_access(&e) => {
            Err(e.context(crate::platform::explain_exclusive_access()))
        }
        Err(e) => Err(e),
    }
}

/// True when a failure to open is contention rather than a real fault.
///
/// Matched on the message because the concrete error type differs per
/// platform: macOS says "exclusive access", Linux reports EBUSY, Windows
/// access-denied.
#[cfg(feature = "mtp-backend")]
fn is_exclusive_access(e: &anyhow::Error) -> bool {
    let msg = format!("{e:#}").to_lowercase();
    msg.contains("exclusive access") || msg.contains("busy") || msg.contains("access is denied")
}

#[cfg(not(feature = "mtp-backend"))]
pub fn open(_device: &Device) -> Result<Box<dyn Backend>> {
    anyhow::bail!("built without an MTP backend (enable feature `mtp-backend`)")
}

#[cfg(feature = "mtp-backend")]
mod mtp_rs_impl {
    use std::collections::HashMap;
    use std::path::Path;

    use anyhow::{anyhow, Context, Result};
    use bytes::Bytes;
    use mtp::{MtpDevice, NewObjectInfo, ObjectHandle, Storage};
    use tokio::runtime::Runtime;

    use super::{Backend, RemoteEntry};
    use crate::garmin::Device;

    pub struct MtpRsBackend {
        rt: Runtime,
        device: MtpDevice,
        storage: Storage,
        // path → folder handle (None = root)
        folder_cache: HashMap<String, Option<ObjectHandle>>,
        // path → file handle (for delete & resize)
        file_cache: HashMap<String, ObjectHandle>,
    }

    impl MtpRsBackend {
        pub fn open(device: &Device) -> Result<Self> {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .context("starting tokio runtime")?;

            let mtp = rt
                .block_on(async {
                    match device.serial.as_deref() {
                        Some(s) => MtpDevice::open_by_serial(s).await,
                        None => MtpDevice::open_first().await,
                    }
                })
                .with_context(|| format!("opening MTP session to {}", device.label()))?;

            // Garmin watches require the PTP container header to be sent in a
            // separate USB bulk transfer from the payload. Without this,
            // `send_object_stream` hangs and times out at 30s.
            mtp.session().set_split_header_data(true);

            let storages = rt
                .block_on(mtp.storages())
                .context("listing device storages")?;
            let storage = storages
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("device exposes no storages"))?;

            let mut folder_cache = HashMap::new();
            folder_cache.insert(String::new(), None);
            Ok(Self {
                rt,
                device: mtp,
                storage,
                folder_cache,
                file_cache: HashMap::new(),
            })
        }

        /// Resolve a folder path, creating any missing components.
        fn resolve_folder(&mut self, path: &str) -> Result<Option<ObjectHandle>> {
            Ok(self.resolve_folder_inner(path, true)?.flatten())
        }

        /// Resolve a folder path without creating anything.
        ///
        /// `Ok(None)` means the folder is not on the device. Read-only callers
        /// must use this: routing them through the creating variant made
        /// listing a nonexistent folder silently create it on the watch.
        fn resolve_folder_existing(&mut self, path: &str) -> Result<Option<Option<ObjectHandle>>> {
            self.resolve_folder_inner(path, false)
        }

        fn resolve_folder_inner(
            &mut self,
            path: &str,
            create: bool,
        ) -> Result<Option<Option<ObjectHandle>>> {
            let key = normalize(path);
            if let Some(h) = self.folder_cache.get(&key) {
                return Ok(Some(*h));
            }
            let mut parent: Option<ObjectHandle> = None;
            let mut acc = String::new();
            for component in key.split('/').filter(|s| !s.is_empty()) {
                if !acc.is_empty() {
                    acc.push('/');
                }
                acc.push_str(component);
                if let Some(h) = self.folder_cache.get(&acc) {
                    parent = *h;
                    continue;
                }
                let storage = &self.storage;
                // Stream so a single broken-stub GetObjectInfo doesn't kill
                // our walk before we even reach the folder we want.
                let found = self
                    .rt
                    .block_on(async {
                        let mut stream = storage.list_objects_stream(parent).await?;
                        while let Some(r) = stream.next().await {
                            if let Ok(info) = r {
                                if info.is_folder() && info.filename == component {
                                    return Ok::<_, mtp::Error>(Some(info.handle));
                                }
                            }
                        }
                        Ok(None)
                    })
                    .with_context(|| format!("listing {acc}"))?;
                let handle = match found {
                    Some(h) => h,
                    None => {
                        if !create {
                            return Ok(None);
                        }
                        self.rt
                            .block_on(storage.create_folder(parent, component))
                            .with_context(|| format!("creating folder {acc}"))?
                    }
                };
                self.folder_cache.insert(acc.clone(), Some(handle));
                parent = Some(handle);
            }
            Ok(Some(parent))
        }

        fn invalidate_path(&mut self, path: &str) {
            let key = normalize(path);
            self.file_cache.remove(&key);
            // Folder caches below this path are invalid too.
            let prefix = if key.is_empty() {
                String::new()
            } else {
                format!("{key}/")
            };
            self.folder_cache
                .retain(|k, _| !(k == &key || k.starts_with(&prefix)));
            self.file_cache
                .retain(|k, _| !(k == &key || k.starts_with(&prefix)));
            // Re-seed root.
            self.folder_cache.entry(String::new()).or_insert(None);
        }
    }

    impl Backend for MtpRsBackend {
        fn model(&self) -> Option<String> {
            // Device-controlled text on its way to a webview, so it gets the
            // same treatment `Device::label` gives the USB strings.
            let model = crate::playlist::strip_control(&self.device.device_info().model);
            let model = model.trim().to_string();
            (!model.is_empty()).then_some(model)
        }

        fn ensure_folder(&mut self, path: &str) -> Result<()> {
            self.resolve_folder(path).map(|_| ())
        }

        fn upload(
            &mut self,
            local: &Path,
            remote_dir: &str,
            remote_name: &str,
            on_progress: &mut (dyn FnMut(u64, u64) + Send),
        ) -> Result<u64> {
            const CHUNK: usize = 256 * 1024;
            let parent = self.resolve_folder(remote_dir)?;
            let mut file = std::fs::File::open(local)
                .with_context(|| format!("opening {}", local.display()))?;
            let len = file
                .metadata()
                .with_context(|| format!("statting {}", local.display()))?
                .len();
            // Stream chunks lazily from disk — avoids holding the full file +
            // a parallel chunked Vec in memory (was 2× the file size).
            //
            // `sent` counts what the stream actually yielded. mtp-rs writes the
            // PTP data-container header from the declared `len` *before* pulling
            // a chunk, so a stream that ends short leaves the device holding a
            // truncated object. We must not report that as success.
            let sent = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
            let sent_w = std::sync::Arc::clone(&sent);
            let stream = futures::stream::poll_fn(move |_cx| {
                use std::io::Read;
                let mut buf = vec![0u8; CHUNK];
                let n = loop {
                    match file.read(&mut buf) {
                        Ok(n) => break n,
                        // EINTR is not a transfer failure. Abandoning the stream
                        // here would strand the device mid-data-phase.
                        Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                        Err(e) => return std::task::Poll::Ready(Some(Err(e))),
                    }
                };
                if n == 0 {
                    return std::task::Poll::Ready(None);
                }
                buf.truncate(n);
                sent_w.fetch_add(n as u64, std::sync::atomic::Ordering::Relaxed);
                std::task::Poll::Ready(Some(Ok(Bytes::from(buf))))
            });
            let info = NewObjectInfo::file(remote_name, len);
            let storage = &self.storage;
            on_progress(0, len);
            let uploaded = self.rt.block_on(storage.upload_with_progress(
                parent,
                info,
                Box::pin(stream),
                |p| {
                    on_progress(p.bytes_transferred, len);
                    std::ops::ControlFlow::Continue(())
                },
            ));
            // Read the counter *before* `?`, so a failure carries how far the
            // data phase got. An abort during the PTP response phase leaves
            // `sent == len` and an object that is very likely intact; an abort
            // mid-data leaves `sent < len` and an object that is not. Those are
            // different facts and callers have to be able to tell them apart,
            // so the distinction is a typed context rather than prose.
            let streamed = sent.load(std::sync::atomic::Ordering::Relaxed);
            let new_handle = uploaded.with_context(|| crate::mtp::UploadPhase {
                local: local.display().to_string(),
                sent: streamed,
                len,
            })?;
            let sent = streamed;
            if sent != len {
                // The object already exists on the device — mtp-rs returned Ok
                // and the data phase is closed. Leaving it would be exactly the
                // broken stub this codebase exists to avoid, and MTP has no
                // overwrite, so a retry would land a second object beside it.
                // Best-effort removal, and say which way it went so the user
                // knows whether retrying is safe.
                // Delete by the handle this upload just produced, never by
                // name: MTP has no overwrite, so a same-named object from an
                // earlier sync may sit beside it and a name lookup could remove
                // that one instead.
                let storage = &self.storage;
                let cleanup = self.rt.block_on(storage.delete(new_handle));
                self.invalidate_path(&normalize(&format!("{remote_dir}/{remote_name}")));
                let cleanup_note = match cleanup {
                    Ok(()) => "the partial object was removed from the watch",
                    Err(_) => "the partial object could NOT be removed — delete it before retrying",
                };
                anyhow::bail!(
                    "{} changed while uploading: declared {len} bytes to the device \
                     but streamed {sent}. The object on the watch is not intact; \
                     {cleanup_note}.",
                    local.display()
                );
            }
            on_progress(sent, len);
            Ok(sent)
        }

        fn remote_size(&mut self, remote_dir: &str, remote_name: &str) -> Result<Option<u64>> {
            let Some(parent) = self.resolve_folder_existing(remote_dir)? else {
                return Ok(None);
            };
            let storage = &self.storage;
            self.rt
                .block_on(async {
                    let mut stream = storage.list_objects_stream(parent).await?;
                    while let Some(r) = stream.next().await {
                        if let Ok(info) = r {
                            if !info.is_folder() && info.filename == remote_name {
                                return Ok::<_, mtp::Error>(Some(info.size));
                            }
                        }
                    }
                    Ok(None)
                })
                .context("listing for size check")
        }

        fn list_dir(&mut self, path: &str) -> Result<Vec<RemoteEntry>> {
            let Some(parent) = self.resolve_folder_existing(path)? else {
                return Ok(Vec::new());
            };
            let storage_id = self.storage.id();
            let session = self.device.session();

            // Two-phase listing so broken stubs stay visible:
            // 1. GetObjectHandles → every handle, including ones whose info errors
            // 2. GetObjectInfo per handle → real entry on success, synthetic
            //    "unreadable" entry on failure.
            //
            // The synthetic entry still carries the handle so delete() can be
            // *attempted* against it — not because that attempt works. Every
            // DeleteObject we have issued against a broken-stub handle on
            // FR165 FW 2506 has come back Protocol GeneralError; see
            // docs/garmin-mtp.md §6 and examples/wipe_stubs.rs, which counts
            // those refusals because refusal is what it expects. Surfacing
            // the handle is what lets the UI name the file and report the
            // firmware's answer verbatim; it is not a claim that the file can
            // be removed. Cleanup is watch-side and asynchronous.
            let (handles, infos): (Vec<_>, Vec<_>) = self
                .rt
                .block_on(async {
                    let handles = session.get_object_handles(storage_id, None, parent).await?;
                    let mut infos = Vec::with_capacity(handles.len());
                    for h in &handles {
                        infos.push(session.get_object_info(*h).await);
                    }
                    Ok::<_, mtp::Error>((handles, infos))
                })
                .with_context(|| format!("listing {path}"))?;

            let key = normalize(path);
            let mut out = Vec::with_capacity(handles.len());
            let mut broken_count = 0usize;
            for (handle, info_result) in handles.into_iter().zip(infos) {
                match info_result {
                    Ok(info) => {
                        // CRITICAL: `session.get_object_info` does NOT populate
                        // `info.handle` — that's a field only `ObjectListing`
                        // backfills. Use the loop variable `handle` for caching
                        // and downstream operations, not `info.handle`.
                        let is_folder = info.is_folder();
                        let child_path = if key.is_empty() {
                            info.filename.clone()
                        } else {
                            format!("{key}/{}", info.filename)
                        };
                        if is_folder {
                            self.folder_cache.insert(child_path.clone(), Some(handle));
                        } else {
                            self.file_cache.insert(child_path.clone(), handle);
                        }
                        out.push(RemoteEntry {
                            name: info.filename,
                            path: child_path,
                            size: info.size,
                            is_folder,
                            is_broken: false,
                        });
                    }
                    Err(_) => {
                        broken_count += 1;
                        let synth_name = format!("‹unreadable #{}›", handle.0);
                        let child_path = if key.is_empty() {
                            synth_name.clone()
                        } else {
                            format!("{key}/{synth_name}")
                        };
                        self.file_cache.insert(child_path.clone(), handle);
                        out.push(RemoteEntry {
                            name: synth_name,
                            path: child_path,
                            size: 0,
                            is_folder: false,
                            is_broken: true,
                        });
                    }
                }
            }
            if broken_count > 0 {
                tracing::warn!(
                    path = %path,
                    broken = broken_count,
                    "{} unreadable entries surfaced — orphan stubs from prior failed uploads",
                    broken_count
                );
            }
            out.sort_by(|a, b| match (a.is_folder, b.is_folder) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            });
            Ok(out)
        }

        fn delete(&mut self, path: &str) -> Result<()> {
            let key = normalize(path);
            let handle = if let Some(h) = self.file_cache.get(&key).copied() {
                h
            } else if let Some(Some(h)) = self.folder_cache.get(&key).copied() {
                h
            } else {
                // Force a parent listing to populate caches.
                let (parent_path, _) = key
                    .rsplit_once('/')
                    .map(|(p, n)| (p.to_string(), n.to_string()))
                    .unwrap_or_else(|| (String::new(), key.clone()));
                self.list_dir(&parent_path)?;
                if let Some(h) = self.file_cache.get(&key).copied() {
                    h
                } else if let Some(Some(h)) = self.folder_cache.get(&key).copied() {
                    h
                } else {
                    anyhow::bail!("not found on watch: {path}");
                }
            };
            let storage = &self.storage;
            self.rt
                .block_on(storage.delete(handle))
                .with_context(|| format!("deleting {path}"))?;
            self.invalidate_path(&key);
            Ok(())
        }

        fn download_file(&mut self, path: &str) -> Result<Vec<u8>> {
            let key = normalize(path);
            // Reuse cached handle if we listed the parent already.
            let handle = if let Some(h) = self.file_cache.get(&key).copied() {
                h
            } else {
                let (parent_path, _) = key
                    .rsplit_once('/')
                    .map(|(p, n)| (p.to_string(), n.to_string()))
                    .unwrap_or_else(|| (String::new(), key.clone()));
                self.list_dir(&parent_path)?;
                self.file_cache
                    .get(&key)
                    .copied()
                    .ok_or_else(|| anyhow!("not found on watch: {path}"))?
            };
            let storage = &self.storage;
            self.rt
                .block_on(storage.download(handle))
                .with_context(|| format!("downloading {path}"))
        }

        fn write_raw(&mut self, remote_dir: &str, remote_name: &str, bytes: &[u8]) -> Result<()> {
            use mtp::ObjectFormatCode;
            let parent = self.resolve_folder(remote_dir)?;
            let len = bytes.len() as u64;
            // M3U/M3U8 → MTP_FORMAT_ABSTRACT_AV_PLAYLIST (0xBA05). Verified
            // working path per `better-sync` (Schachte) on FR family + Venu;
            // Garmin firmware silently rejects playlist writes with any
            // other format code. See docs/playlists.md.
            let lower = remote_name.to_ascii_lowercase();
            let format = if lower.ends_with(".m3u8") || lower.ends_with(".m3u") {
                ObjectFormatCode::Unknown(0xBA05)
            } else {
                ObjectFormatCode::Undefined
            };
            let info = mtp::NewObjectInfo::with_format(remote_name, len, format);
            let chunks: Vec<_> = bytes
                .chunks(256 * 1024)
                .map(|c| Ok::<_, std::io::Error>(Bytes::copy_from_slice(c)))
                .collect();
            let stream = futures::stream::iter(chunks);
            let storage = &self.storage;
            self.rt
                .block_on(storage.upload(parent, info, Box::pin(stream)))
                .with_context(|| format!("writing {remote_dir}/{remote_name}"))?;
            self.invalidate_path(remote_dir);
            self.folder_cache.entry(String::new()).or_insert(None);
            Ok(())
        }

        fn free_space(&mut self) -> Result<(u64, u64)> {
            // mtp-rs StorageInfo is fetched at storage-discovery time; re-fetch
            // by re-querying storages so the "free" number reflects writes.
            let storages = self
                .rt
                .block_on(self.device.storages())
                .context("re-fetching storage info")?;
            let s = storages
                .into_iter()
                .next()
                .ok_or_else(|| anyhow!("no storages"))?;
            let info = s.info();
            Ok((info.free_space_bytes, info.max_capacity))
        }
    }

    fn normalize(path: &str) -> String {
        path.trim_matches('/').to_string()
    }

    #[allow(dead_code)]
    fn _device_marker(_d: &Device) {}
}
