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

pub trait Backend: Send {
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
    Ok(Box::new(mtp_rs_impl::MtpRsBackend::open(device)?))
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

            let mtp = rt.block_on(async {
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

        fn resolve_folder(&mut self, path: &str) -> Result<Option<ObjectHandle>> {
            let key = normalize(path);
            if let Some(h) = self.folder_cache.get(&key) {
                return Ok(*h);
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
                    None => self
                        .rt
                        .block_on(storage.create_folder(parent, component))
                        .with_context(|| format!("creating folder {acc}"))?,
                };
                self.folder_cache.insert(acc.clone(), Some(handle));
                parent = Some(handle);
            }
            Ok(parent)
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
            let bytes = std::fs::read(local)
                .with_context(|| format!("reading {}", local.display()))?;
            let len = bytes.len() as u64;
            let info = NewObjectInfo::file(remote_name, len);
            let chunks: Vec<_> = bytes
                .chunks(CHUNK)
                .map(|c| Ok::<_, std::io::Error>(Bytes::copy_from_slice(c)))
                .collect();
            let stream = futures::stream::iter(chunks);
            let storage = &self.storage;
            on_progress(0, len);
            self.rt
                .block_on(storage.upload_with_progress(
                    parent,
                    info,
                    Box::pin(stream),
                    |p| {
                        on_progress(p.bytes_transferred, len);
                        std::ops::ControlFlow::Continue(())
                    },
                ))
                .with_context(|| format!("uploading {}", local.display()))?;
            on_progress(len, len);
            Ok(len)
        }

        fn remote_size(&mut self, remote_dir: &str, remote_name: &str) -> Result<Option<u64>> {
            let parent = self.resolve_folder(remote_dir)?;
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
            let parent = self.resolve_folder(path)?;
            let storage_id = self.storage.id();
            let session = self.device.session();

            // Two-phase listing so broken stubs stay visible (and deletable):
            // 1. GetObjectHandles → every handle, including ones whose info errors
            // 2. GetObjectInfo per handle → real entry on success, synthetic
            //    "unreadable" entry on failure (still carries the handle so
            //    delete() works against it later).
            let (handles, infos): (Vec<_>, Vec<_>) = self
                .rt
                .block_on(async {
                    let handles = session
                        .get_object_handles(storage_id, None, parent)
                        .await?;
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
            for (handle, info_result) in handles.into_iter().zip(infos.into_iter()) {
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
            // Pick the right MTP format code by extension. M3U/M3U8 → Abstract
            // Audio Playlist (0xBA10), which Garmin firmware actually expects
            // for playlist files in /Music. Without the right format code,
            // the watch silently drops the write.
            let lower = remote_name.to_ascii_lowercase();
            let format = if lower.ends_with(".m3u8") || lower.ends_with(".m3u") {
                // M3U files are plain text. Try standard MTP `Text` format
                // (0x3004) — Garmin firmware seems to silently drop writes
                // with playlist-specific format codes (0xBA11 etc).
                ObjectFormatCode::Text
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
