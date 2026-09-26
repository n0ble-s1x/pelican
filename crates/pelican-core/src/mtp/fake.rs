//! An in-memory watch, for tests.
//!
//! Agents and CI never touch the real device, so everything above the
//! [`Backend`] trait is proven against this instead. It models the parts of
//! the FR165's behaviour that the transfer loop's correctness depends on,
//! not just the happy path:
//!
//! - **Names are case-insensitive**, through the same [`fold_name`] as the
//!   real backend.
//! - **A second write to a taken name turns both objects into stubs** —
//!   libmtp #307 and `docs/garmin-mtp.md` §7. A test that reuses a name
//!   sees exactly the wreckage the hardware would leave.
//! - **Stubs are listed with a synthetic name and cannot be read.**
//! - **Faults on demand**: an upload that errors, one that lands corrupted
//!   (so the read-back hash disagrees), a download that errors, a listing
//!   that errors.
//!
//! There is no delete here either, for the same reason there is none on
//! the trait.
//!
//! [`FakeDevice`] is the handle a test keeps; [`FakeDevice::backend`] hands
//! out a `Box<dyn Backend>` sharing the same state, so a test can drive the
//! code under test and then inspect what the "watch" was asked to do.

use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};

use anyhow::{anyhow, bail, Context, Result};

use super::{fold_name, Backend, RemoteEntry, Uploaded};

/// One call the code under test made, in order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Call {
    EnsureFolder(String),
    ListDir(String),
    /// `(remote_dir, remote_name)` — recorded before any fault applies, so
    /// a failed upload still shows up as attempted.
    Upload(String, String),
    Download(String),
    FreeSpace,
}

/// An object on the fake watch.
#[derive(Debug, Clone)]
pub struct FakeObject {
    pub handle: u32,
    /// Folder path as it was created, e.g. `Music`.
    pub dir: String,
    pub name: String,
    pub data: Vec<u8>,
    pub is_folder: bool,
    /// A stub: listed, unreadable, and holding a name we cannot see.
    pub broken: bool,
}

/// Faults to inject. Each counter is consumed one call at a time, so
/// "fail the first upload, succeed on the retry" is `fail_uploads = 1`.
#[derive(Debug, Default, Clone)]
pub struct Faults {
    /// Uploads that return an error without creating an object.
    pub fail_uploads: usize,
    /// Uploads that return `Ok` but store altered bytes — the silent
    /// corruption the read-back hash exists to catch.
    pub corrupt_uploads: usize,
    /// Downloads that return an error.
    pub fail_downloads: usize,
    /// Every `list_dir` errors while this is set.
    pub fail_listing: bool,
}

type UploadHook = Box<dyn FnMut(&str, &str) + Send>;

struct State {
    objects: Vec<FakeObject>,
    next_handle: u32,
    free: u64,
    capacity: u64,
    model: Option<String>,
    calls: Vec<Call>,
    faults: Faults,
    on_upload: Option<UploadHook>,
}

/// The test's handle on a fake watch. Cheap to clone; every clone and every
/// backend it hands out share one state.
#[derive(Clone)]
pub struct FakeDevice {
    state: Arc<Mutex<State>>,
}

impl Default for FakeDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeDevice {
    /// An empty watch with a `Music` folder and 4 GiB free.
    pub fn new() -> Self {
        let dev = Self {
            state: Arc::new(Mutex::new(State {
                objects: Vec::new(),
                next_handle: 1,
                free: 4 << 30,
                capacity: 4 << 30,
                model: Some("Fake Forerunner".into()),
                calls: Vec::new(),
                faults: Faults::default(),
                on_upload: None,
            })),
        };
        dev.lock().push_folder("", crate::garmin::MUSIC_FOLDER);
        dev
    }

    /// A backend over this device's state, as `mtp::open` would return.
    pub fn backend(&self) -> Box<dyn Backend> {
        Box::new(FakeBackend { dev: self.clone() })
    }

    /// Put a readable file on the watch, as some earlier tool would have.
    pub fn add_file(&self, dir: &str, name: &str, data: &[u8]) -> u32 {
        let mut s = self.lock();
        s.push(FakeObject {
            handle: 0,
            dir: dir.to_string(),
            name: name.to_string(),
            data: data.to_vec(),
            is_folder: false,
            broken: false,
        })
    }

    /// Put a stub on the watch: a handle whose name cannot be read.
    pub fn add_stub(&self, dir: &str) -> u32 {
        let mut s = self.lock();
        s.push(FakeObject {
            handle: 0,
            dir: dir.to_string(),
            name: String::new(),
            data: Vec::new(),
            is_folder: false,
            broken: true,
        })
    }

    pub fn set_space(&self, free: u64, capacity: u64) {
        let mut s = self.lock();
        s.free = free;
        s.capacity = capacity;
    }

    pub fn set_faults(&self, faults: Faults) {
        self.lock().faults = faults;
    }

    /// Run `hook(remote_dir, remote_name)` at the start of every upload,
    /// before any byte "moves". This is how a test observes what was true
    /// on disk at the moment of the write — the ledger's reserve line, for
    /// one.
    pub fn on_upload(&self, hook: impl FnMut(&str, &str) + Send + 'static) {
        self.lock().on_upload = Some(Box::new(hook));
    }

    pub fn calls(&self) -> Vec<Call> {
        self.lock().calls.clone()
    }

    /// Every object currently on the watch, folders included.
    pub fn objects(&self) -> Vec<FakeObject> {
        self.lock().objects.clone()
    }

    /// Readable files in `dir`, as `(name, data)`.
    pub fn files(&self, dir: &str) -> Vec<(String, Vec<u8>)> {
        let want = fold_name(dir.trim_matches('/'));
        self.lock()
            .objects
            .iter()
            .filter(|o| !o.is_folder && !o.broken && fold_name(&o.dir) == want)
            .map(|o| (o.name.clone(), o.data.clone()))
            .collect()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        // A test that panicked while holding the lock has already failed;
        // the state is still worth reading for the next assertion.
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl State {
    fn push(&mut self, mut o: FakeObject) -> u32 {
        o.handle = self.next_handle;
        self.next_handle += 1;
        let h = o.handle;
        self.objects.push(o);
        h
    }

    fn push_folder(&mut self, parent: &str, name: &str) -> u32 {
        self.push(FakeObject {
            handle: 0,
            dir: parent.to_string(),
            name: name.to_string(),
            data: Vec::new(),
            is_folder: true,
            broken: false,
        })
    }

    fn folder_exists(&self, path: &str) -> bool {
        if path.is_empty() {
            return true;
        }
        let (parent, name) = split(path);
        let (parent, name) = (fold_name(parent), fold_name(name));
        self.objects
            .iter()
            .any(|o| o.is_folder && fold_name(&o.dir) == parent && fold_name(&o.name) == name)
    }
}

struct FakeBackend {
    dev: FakeDevice,
}

impl Backend for FakeBackend {
    fn model(&self) -> Option<String> {
        self.dev.lock().model.clone()
    }

    fn ensure_folder(&mut self, path: &str) -> Result<()> {
        let path = path.trim_matches('/');
        let mut s = self.dev.lock();
        s.calls.push(Call::EnsureFolder(path.to_string()));
        let mut acc = String::new();
        for component in path.split('/').filter(|c| !c.is_empty()) {
            let parent = acc.clone();
            if !acc.is_empty() {
                acc.push('/');
            }
            acc.push_str(component);
            if !s.folder_exists(&acc) {
                s.push_folder(&parent, component);
            }
        }
        Ok(())
    }

    fn upload(
        &mut self,
        local: &Path,
        remote_dir: &str,
        remote_name: &str,
        on_progress: &mut (dyn FnMut(u64, u64) + Send),
    ) -> Result<Uploaded> {
        let dir = remote_dir.trim_matches('/').to_string();
        // Take the hook out so it can run without the lock held — it will
        // usually want to read files, and may call back into the device.
        let hook = {
            let mut s = self.dev.lock();
            s.calls
                .push(Call::Upload(dir.clone(), remote_name.to_string()));
            s.on_upload.take()
        };
        if let Some(mut hook) = hook {
            hook(&dir, remote_name);
            self.dev.lock().on_upload = Some(hook);
        }

        let mut data =
            std::fs::read(local).with_context(|| format!("opening {}", local.display()))?;
        let len = data.len() as u64;
        let mut s = self.dev.lock();
        if !s.folder_exists(&dir) {
            bail!("no folder {dir} on the fake watch");
        }
        if s.faults.fail_uploads > 0 {
            s.faults.fail_uploads -= 1;
            bail!("injected upload failure for {dir}/{remote_name}");
        }
        if len > s.free {
            bail!("storage full: {len} bytes wanted, {} free", s.free);
        }
        if s.faults.corrupt_uploads > 0 {
            s.faults.corrupt_uploads -= 1;
            match data.first_mut() {
                Some(b) => *b ^= 0xFF,
                None => data.push(0),
            }
        }
        on_progress(0, len);

        // The hardware's answer to a reused name: the object already there
        // and the one being sent both become stubs. Modelled so a test that
        // reuses a name fails the way the watch would, not silently.
        let (want_dir, want_name) = (fold_name(&dir), fold_name(remote_name));
        let mut collided = false;
        for o in s.objects.iter_mut() {
            if !o.is_folder && fold_name(&o.dir) == want_dir && fold_name(&o.name) == want_name {
                o.broken = true;
                collided = true;
            }
        }
        s.free -= len;
        let handle = s.push(FakeObject {
            handle: 0,
            dir,
            name: remote_name.to_string(),
            data,
            is_folder: false,
            broken: collided,
        });
        drop(s);
        on_progress(len, len);
        Ok(Uploaded { handle, bytes: len })
    }

    fn list_dir(&mut self, path: &str) -> Result<Vec<RemoteEntry>> {
        let path = path.trim_matches('/').to_string();
        let mut s = self.dev.lock();
        s.calls.push(Call::ListDir(path.clone()));
        if s.faults.fail_listing {
            bail!("injected listing failure for {path}");
        }
        if !s.folder_exists(&path) {
            return Ok(Vec::new());
        }
        let want = fold_name(&path);
        let mut out: Vec<RemoteEntry> = s
            .objects
            .iter()
            .filter(|o| fold_name(&o.dir) == want)
            .map(|o| {
                let name = if o.broken {
                    format!("‹unreadable #{}›", o.handle)
                } else {
                    o.name.clone()
                };
                RemoteEntry {
                    path: if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}/{name}")
                    },
                    name,
                    size: if o.broken { 0 } else { o.data.len() as u64 },
                    is_folder: o.is_folder,
                    is_broken: o.broken,
                    handle: o.handle,
                }
            })
            .collect();
        out.sort_by(|a, b| {
            b.is_folder
                .cmp(&a.is_folder)
                .then_with(|| fold_name(&a.name).cmp(&fold_name(&b.name)))
        });
        Ok(out)
    }

    fn free_space(&mut self) -> Result<(u64, u64)> {
        let mut s = self.dev.lock();
        s.calls.push(Call::FreeSpace);
        Ok((s.free, s.capacity))
    }

    fn download_file(&mut self, path: &str) -> Result<Vec<u8>> {
        let path = path.trim_matches('/');
        let mut s = self.dev.lock();
        s.calls.push(Call::Download(path.to_string()));
        if s.faults.fail_downloads > 0 {
            s.faults.fail_downloads -= 1;
            bail!("injected download failure for {path}");
        }
        let (dir, name) = split(path);
        let (dir, name) = (fold_name(dir), fold_name(name));
        // Newest first, like the real backend's handle cache after an upload.
        let o = s
            .objects
            .iter()
            .rev()
            .find(|o| !o.is_folder && fold_name(&o.dir) == dir && fold_name(&o.name) == name)
            .ok_or_else(|| anyhow!("not found on watch: {path}"))?;
        if o.broken {
            bail!("GetObjectInfo failed for {path}: the object is a stub");
        }
        Ok(o.data.clone())
    }
}

fn split(path: &str) -> (&str, &str) {
    path.rsplit_once('/').unwrap_or(("", path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local(dir: &tempfile::TempDir, bytes: &[u8]) -> std::path::PathBuf {
        let p = dir.path().join("f.mp3");
        std::fs::write(&p, bytes).unwrap();
        p
    }

    #[test]
    fn upload_then_download_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        let mut b = dev.backend();
        let up = b
            .upload(
                &local(&tmp, b"abc"),
                "Music",
                "pl00001-x.mp3",
                &mut |_, _| {},
            )
            .unwrap();
        assert_eq!(up.bytes, 3);
        assert_eq!(b.download_file("Music/PL00001-X.mp3").unwrap(), b"abc");
    }

    #[test]
    fn a_reused_name_leaves_two_stubs_like_the_hardware() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.add_file("Music", "Track.mp3", b"old");
        let mut b = dev.backend();
        b.upload(&local(&tmp, b"new"), "Music", "track.mp3", &mut |_, _| {})
            .unwrap();
        let listed = b.list_dir("Music").unwrap();
        assert_eq!(listed.len(), 2);
        assert!(listed.iter().all(|e| e.is_broken), "{listed:?}");
        assert!(dev.files("Music").is_empty());
    }

    #[test]
    fn corrupt_upload_reads_back_different() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.set_faults(Faults {
            corrupt_uploads: 1,
            ..Default::default()
        });
        let mut b = dev.backend();
        b.upload(&local(&tmp, b"abc"), "Music", "a.mp3", &mut |_, _| {})
            .unwrap();
        assert_ne!(b.download_file("Music/a.mp3").unwrap(), b"abc");
    }

    #[test]
    fn failed_upload_creates_nothing_but_is_recorded() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.set_faults(Faults {
            fail_uploads: 1,
            ..Default::default()
        });
        let mut b = dev.backend();
        assert!(b
            .upload(&local(&tmp, b"abc"), "Music", "a.mp3", &mut |_, _| {})
            .is_err());
        assert!(dev.files("Music").is_empty());
        assert_eq!(
            dev.calls(),
            vec![Call::Upload("Music".into(), "a.mp3".into())]
        );
    }

    #[test]
    fn stubs_are_listed_and_unreadable() {
        let dev = FakeDevice::new();
        let h = dev.add_stub("Music");
        let mut b = dev.backend();
        let listed = b.list_dir("Music").unwrap();
        assert_eq!(listed.len(), 1);
        assert!(listed[0].is_broken);
        assert_eq!(listed[0].handle, h);
        assert_eq!(listed[0].name, format!("‹unreadable #{h}›"));
    }

    #[test]
    fn upload_hook_runs_before_the_object_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        let seen = Arc::new(Mutex::new(None));
        let (probe, seen_w) = (dev.clone(), Arc::clone(&seen));
        dev.on_upload(move |_, _| {
            *seen_w.lock().unwrap() = Some(probe.files("Music").len());
        });
        let mut b = dev.backend();
        b.upload(&local(&tmp, b"abc"), "Music", "a.mp3", &mut |_, _| {})
            .unwrap();
        assert_eq!(*seen.lock().unwrap(), Some(0));
        assert_eq!(dev.files("Music").len(), 1);
    }

    #[test]
    fn folder_lookup_is_case_insensitive() {
        let dev = FakeDevice::new();
        let mut b = dev.backend();
        b.ensure_folder("MUSIC").unwrap();
        let folders = dev.objects().iter().filter(|o| o.is_folder).count();
        assert_eq!(folders, 1, "ensure_folder created a second Music");
    }
}
