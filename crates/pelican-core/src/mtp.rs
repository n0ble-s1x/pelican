//! MTP backend abstraction.
//!
//! Implementations sit behind a trait so the transfer loop can be tested
//! against [`fake::FakeDevice`] without a watch on the desk. The trait is the
//! whole device API, and what it leaves out is deliberate: there is **no
//! delete and no raw write**. Delete cannot take a track out of the watch's
//! music library (`docs/garmin-library-persistence.md`), a delete-then-write
//! is how a name gets reused, and a playlist write is a second way to put a
//! name on the device that the ledger does not own. A capability that does
//! not exist in the type cannot be called by mistake.

use std::path::Path;

use anyhow::Result;

use crate::garmin::Device;

pub mod fake;

#[derive(Debug, Clone)]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub is_folder: bool,
    /// True if `GetObjectInfo` failed for this handle: we know the handle
    /// exists on the device but can't read its metadata. Almost always a
    /// broken stub from a previous partial / rejected / colliding upload.
    /// Its `name` is synthetic (`‹unreadable #N›`) because the real one is
    /// unknowable, which is why the naming rules treat every stub as
    /// occupying a name we cannot see rather than trying to match it.
    pub is_broken: bool,
    /// The MTP object handle. Stable for the life of the object; the only
    /// identifier a stub has.
    pub handle: u32,
}

/// What a completed upload left on the device.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Uploaded {
    /// Handle of the object this upload created. The backend also remembers
    /// it under the uploaded path, so a `download_file` of that path reads
    /// back *this* object and not some older one the listing might offer.
    pub handle: u32,
    /// Bytes streamed. Equal to the local file's length, or the upload
    /// would have returned an error.
    pub bytes: u64,
}

/// How far an upload got before it failed. It ends up in the ledger's
/// `failed` reason, which is the only record of what that attempt left on
/// the watch.
///
/// `sent == len` means the whole file crossed the wire and the failure was in
/// the PTP *response* phase, so the object is quite possibly on the watch.
/// `sent < len` means the data phase itself was cut short, and whatever is on
/// the device is not the file. Either way the name is burned and the file is
/// retried under a new one; the difference is for whoever reads the ledger.
#[derive(Debug, Clone)]
pub struct UploadPhase {
    pub local: String,
    pub sent: u64,
    pub len: u64,
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

/// The one case fold used for every name comparison in Pelican.
///
/// `/Music` is FAT-derived and case-insensitive: `Track.mp3` and `track.mp3`
/// are one file to the firmware and two different strings to Rust. Every
/// comparison (folder resolution, the handle cache, taken-name checks,
/// the ledger) goes through this function, so no two call sites can
/// disagree about what "the same name" means.
///
/// Full `to_lowercase`, not the ASCII fold: our names are ASCII by
/// construction (`transcode::sanitize_filename_stem`) but the device side is
/// whatever Garmin Express or another tool wrote, which is not.
pub fn fold_name(name: &str) -> String {
    name.to_lowercase()
}

/// Do these two names refer to the same object on the watch?
pub fn same_file(a: &str, b: &str) -> bool {
    fold_name(a) == fold_name(b)
}

pub trait Backend: Send {
    /// The device's own model name, from MTP `GetDeviceInfo`.
    ///
    /// The FR165 declares no USB product string descriptor (`iProduct = 0`),
    /// so `nusb::DeviceInfo::product_string()` has nothing to return and the
    /// real name ("Forerunner 165 Music") exists only here. `GetDeviceInfo`
    /// is fetched and cached during `open`, so this costs no device I/O.
    ///
    /// `None` when the device gives no model, which keeps the caller's
    /// fallback chain honest rather than inventing a name.
    fn model(&self) -> Option<String> {
        None
    }
    fn ensure_folder(&mut self, path: &str) -> Result<()>;
    /// Upload a local file into a remote folder as a new object.
    ///
    /// MTP has no overwrite: sending a name that already exists does not
    /// replace anything, it leaves two objects the firmware turns into
    /// stubs (libmtp #307). Choosing a name that has never been used is the
    /// caller's job, and the backend does not second-guess it.
    ///
    /// `on_progress(bytes_transferred, total_bytes)` is called as data
    /// flows; pass `&mut |_, _| {}` if you don't care.
    fn upload(
        &mut self,
        local: &Path,
        remote_dir: &str,
        remote_name: &str,
        on_progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<Uploaded>;
    fn list_dir(&mut self, path: &str) -> Result<Vec<RemoteEntry>>;
    /// `(free, capacity)` in bytes for the storage we write to, re-read
    /// from the device so it reflects writes made in this session.
    fn free_space(&mut self) -> Result<(u64, u64)>;
    /// Read a remote file back in full. This is the proof step: the bytes
    /// that come back are hashed against the local transcode.
    fn download_file(&mut self, path: &str) -> Result<Vec<u8>>;
}

#[cfg(feature = "mtp-backend")]
pub fn open(device: &Device) -> Result<Box<dyn Backend>> {
    match mtp_rs_impl::MtpRsBackend::open(device) {
        Ok(b) => Ok(Box::new(b)),
        // A timeout on open is the watch not answering, typically right
        // after it rebooted. Only a replug clears it, and the error says so.
        Err(e) if crate::error::is_wedged(&e) => Err(e.context(crate::error::Wedged)),
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

    use super::{fold_name, Backend, RemoteEntry, Uploaded};
    use crate::garmin::Device;

    pub struct MtpRsBackend {
        rt: Runtime,
        device: MtpDevice,
        storage: Storage,
        // Both caches are keyed by `key(path)` (trimmed and case-folded), so
        // `Music` and `music` resolve to the one folder the firmware sees.
        // folded path → folder handle (None = root)
        folder_cache: HashMap<String, Option<ObjectHandle>>,
        // folded path → file handle, for `download_file`
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
            let trimmed = trim(path);
            if let Some(h) = self.folder_cache.get(&key(trimmed)) {
                return Ok(Some(*h));
            }
            let mut parent: Option<ObjectHandle> = None;
            let mut acc = String::new();
            for component in trimmed.split('/').filter(|s| !s.is_empty()) {
                if !acc.is_empty() {
                    acc.push('/');
                }
                acc.push_str(component);
                let acc_key = key(&acc);
                if let Some(h) = self.folder_cache.get(&acc_key) {
                    parent = *h;
                    continue;
                }
                let storage = &self.storage;
                let want = fold_name(component);
                // Stream so a single broken-stub GetObjectInfo doesn't kill
                // our walk before we even reach the folder we want.
                //
                // Folded, like every other name comparison: a byte-exact
                // match here missed a `music` folder and created a second
                // `Music` beside it on a filesystem that cannot hold both.
                let found = self
                    .rt
                    .block_on(async {
                        let mut stream = storage.list_objects_stream(parent).await?;
                        while let Some(r) = stream.next().await {
                            if let Ok(info) = r {
                                if info.is_folder() && fold_name(&info.filename) == want {
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
                self.folder_cache.insert(acc_key, Some(handle));
                parent = Some(handle);
            }
            Ok(Some(parent))
        }
    }

    impl Backend for MtpRsBackend {
        fn model(&self) -> Option<String> {
            // Device-controlled text on its way to a terminal, so it gets the
            // same treatment `Device::label` gives the USB strings.
            let model = crate::garmin::strip_control(&self.device.device_info().model);
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
        ) -> Result<Uploaded> {
            const CHUNK: usize = 256 * 1024;
            let parent = self.resolve_folder(remote_dir)?;
            let mut file = std::fs::File::open(local)
                .with_context(|| format!("opening {}", local.display()))?;
            let len = file
                .metadata()
                .with_context(|| format!("statting {}", local.display()))?
                .len();
            // Stream chunks lazily from disk rather than holding the file and a
            // chunked copy of it in memory at once.
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
            // different facts, and the ledger's failure reason records which.
            let streamed = sent.load(std::sync::atomic::Ordering::Relaxed);
            let new_handle = uploaded.with_context(|| crate::mtp::UploadPhase {
                local: local.display().to_string(),
                sent: streamed,
                len,
            })?;
            if streamed != len {
                // The object exists on the device (mtp-rs returned Ok and the
                // data phase is closed), but it is not the file. It stays: the
                // name is burned either way, and the read-back hash is what
                // records the failure. Nothing here tries to remove it.
                anyhow::bail!(
                    "{} changed while uploading: declared {len} bytes to the device \
                     but streamed {streamed}. The object on the watch is not intact, \
                     and its name will not be used again.",
                    local.display()
                );
            }
            // Remember the new object under its path, so the read-back that
            // follows downloads exactly this handle without re-listing a
            // folder that may hold hundreds of objects.
            let path = format!("{}/{remote_name}", trim(remote_dir));
            self.file_cache.insert(key(&path), new_handle);
            on_progress(streamed, len);
            Ok(Uploaded {
                handle: new_handle.0,
                bytes: streamed,
            })
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
            // A stub still occupies a name on the device (just one we cannot
            // read), so it has to be surfaced for the naming rules to account
            // for, and for `ls` to show.
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

            let dir = trim(path);
            let child = |name: &str| {
                if dir.is_empty() {
                    name.to_string()
                } else {
                    format!("{dir}/{name}")
                }
            };
            let mut out = Vec::with_capacity(handles.len());
            let mut broken_count = 0usize;
            for (handle, info_result) in handles.into_iter().zip(infos) {
                match info_result {
                    Ok(info) => {
                        // CRITICAL: `session.get_object_info` does NOT populate
                        // `info.handle`; that's a field only `ObjectListing`
                        // backfills. Use the loop variable `handle` for caching
                        // and downstream operations, not `info.handle`.
                        let is_folder = info.is_folder();
                        let child_path = child(&info.filename);
                        if is_folder {
                            self.folder_cache.insert(key(&child_path), Some(handle));
                        } else {
                            self.file_cache.insert(key(&child_path), handle);
                        }
                        out.push(RemoteEntry {
                            name: info.filename,
                            path: child_path,
                            size: info.size,
                            is_folder,
                            is_broken: false,
                            handle: handle.0,
                        });
                    }
                    Err(_) => {
                        broken_count += 1;
                        let synth_name = format!("‹unreadable #{}›", handle.0);
                        out.push(RemoteEntry {
                            path: child(&synth_name),
                            name: synth_name,
                            size: 0,
                            is_folder: false,
                            is_broken: true,
                            handle: handle.0,
                        });
                    }
                }
            }
            if broken_count > 0 {
                tracing::warn!(
                    path = %path,
                    broken = broken_count,
                    "{} unreadable entries surfaced: orphan stubs from earlier failed uploads",
                    broken_count
                );
            }
            out.sort_by(|a, b| match (a.is_folder, b.is_folder) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => fold_name(&a.name).cmp(&fold_name(&b.name)),
            });
            Ok(out)
        }

        fn download_file(&mut self, path: &str) -> Result<Vec<u8>> {
            let k = key(path);
            // A path this session uploaded or listed is answered from the
            // cache; anything else costs one listing of its parent.
            let handle = match self.file_cache.get(&k).copied() {
                Some(h) => h,
                None => {
                    let parent = trim(path).rsplit_once('/').map_or("", |(p, _)| p);
                    self.list_dir(parent)?;
                    self.file_cache
                        .get(&k)
                        .copied()
                        .ok_or_else(|| anyhow!("not found on watch: {path}"))?
                }
            };
            let storage = &self.storage;
            self.rt
                .block_on(storage.download(handle))
                .with_context(|| format!("downloading {path}"))
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

    fn trim(path: &str) -> &str {
        path.trim_matches('/')
    }

    /// Cache key for a path: trimmed and folded, so one object has one key.
    fn key(path: &str) -> String {
        fold_name(trim(path))
    }
}

#[cfg(test)]
mod tests {
    use super::{fold_name, same_file};

    /// `/Music` is FAT-derived. Two spellings, one file. A byte-equal
    /// comparison would report "not found" for a file that is right there,
    /// and as a pre-write guard would miss exactly the collisions a
    /// case-insensitive filesystem creates.
    #[test]
    fn names_differing_only_in_case_are_the_same_file() {
        assert!(same_file("Track.mp3", "track.mp3"));
        assert!(same_file("TRACK.MP3", "track.mp3"));
        assert!(same_file("track.mp3", "track.mp3"));
    }

    #[test]
    fn different_names_are_not_the_same_file() {
        assert!(!same_file("track.mp3", "track-2.mp3"));
        assert!(!same_file("track.mp3", "track.m4a"));
    }

    /// The device side is whatever Garmin Express wrote, which is not ASCII.
    /// `eq_ignore_ascii_case` would call these two different files.
    #[test]
    fn case_folding_is_not_ascii_only() {
        assert!(same_file("CAFÉ.mp3", "café.mp3"));
    }

    /// Folder names go through the same fold as file names. Byte-exact
    /// folder matching is how a `music` folder got a `Music` sibling.
    #[test]
    fn folders_and_files_share_one_fold() {
        assert_eq!(fold_name("Music"), fold_name("MUSIC"));
        assert_eq!(fold_name("Music"), fold_name("music"));
        assert_eq!(fold_name("ÅLBUM"), "ålbum");
    }
}
