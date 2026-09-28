//! Where transcodes wait between ffmpeg and the watch.
//!
//! A run transcodes **everything first**, into a directory of its own, and
//! only then opens the MTP session. That ordering is what makes the
//! capacity check exact (the bytes to send are sitting on disk) and keeps
//! the device idle while ffmpeg works. The cost is that a whole album of
//! MP3s exists on disk for the length of the run, so the directory has to
//! go away however the run ends:
//!
//! - **success, failure, `?`, panic**: [`StagingDir`]'s `Drop` removes it.
//! - **Ctrl-C, SIGKILL, power loss**: no destructor runs. Each staging
//!   directory holds an exclusive lock on its own `.lock` file for as long
//!   as the run lives, and the kernel drops that lock when the process
//!   dies, however it dies. [`sweep`], run at every start, removes any
//!   staging directory whose lock it can take: nobody is holding it, so
//!   nobody is using it. A live run in another terminal keeps its lock and
//!   is left alone. No pid files, no guessing from ages.
//!
//! Sources are never written. Everything here lives under the per-user
//! cache dir ([`crate::paths::cache_dir`]), never `/tmp`, so another local
//! user cannot pre-create the path.

use std::fs::{self, File, OpenOptions};
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use anyhow::{anyhow, Context, Result};

const STAGING: &str = "staging";
const LOCK: &str = ".lock";

/// A directory without a `.lock` is either a run caught in the instant
/// between `mkdir` and taking its lock, or the leftover of a crash in that
/// same instant. Age tells them apart; this is far longer than the instant.
const UNLOCKED_GRACE: Duration = Duration::from_secs(10 * 60);

/// Loose `pelican-*` files in the cache root are the staging scheme before
/// this one (one file per transcode, no directory). Anything that old is a
/// leftover from a crash.
const LEGACY_MAX_AGE: Duration = Duration::from_secs(60 * 60);

/// One run's staging directory: `<cache>/staging/<run-id>/`. Removed, with
/// everything in it, when dropped.
#[derive(Debug)]
pub struct StagingDir {
    path: PathBuf,
    run_id: String,
    // Held, never read: the lock is the "this run is alive" signal that
    // `sweep` tests for. Closed after the directory is gone.
    _lock: File,
}

impl StagingDir {
    /// Create a run directory under `base/staging/`. `base` is the cache
    /// dir in production and a temp dir in tests.
    pub fn create_in(base: &Path) -> Result<Self> {
        let root = base.join(STAGING);
        fs::create_dir_all(&root).with_context(|| format!("creating {}", root.display()))?;
        // A sweep in another process can win the race for a directory we
        // have made but not yet locked. It only does that for a directory it
        // could lock, and it removes it while holding the lock, so if ours
        // is gone once we hold it, try again under a new id.
        for _ in 0..3 {
            let run_id = run_id();
            let path = root.join(&run_id);
            mkdir_private(&path).with_context(|| format!("creating {}", path.display()))?;
            let lock_path = path.join(LOCK);
            let lock = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&lock_path)
                .with_context(|| format!("creating {}", lock_path.display()))?;
            lock.lock()
                .with_context(|| format!("locking {}", lock_path.display()))?;
            if lock_path.exists() {
                return Ok(Self {
                    path,
                    run_id,
                    _lock: lock,
                });
            }
        }
        Err(anyhow!(
            "could not create a staging directory in {}: it kept disappearing",
            root.display()
        ))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn run_id(&self) -> &str {
        &self.run_id
    }

    /// Where the `n`th planned file is transcoded to. Numbered rather than
    /// named after the source, so nothing a source filename contains can
    /// reach ffmpeg's output argument.
    pub fn file(&self, n: usize) -> PathBuf {
        self.path.join(format!("{n:05}.mp3"))
    }
}

impl Drop for StagingDir {
    fn drop(&mut self) {
        if let Err(e) = fs::remove_dir_all(&self.path) {
            if e.kind() != ErrorKind::NotFound {
                // Nothing to propagate to from a destructor. The next start's
                // sweep will find it unlocked and finish the job.
                tracing::warn!(dir = %self.path.display(), error = %e, "could not remove staging dir");
            }
        }
    }
}

/// Remove what crashed or interrupted runs left behind. Called once at
/// startup; never touches a directory a live run holds.
pub fn sweep() {
    if let Some(base) = crate::paths::cache_dir() {
        sweep_in(&base);
    }
}

/// [`sweep`] against an explicit cache dir. Returns how many leftovers it
/// removed, for tests and for a log line.
pub fn sweep_in(base: &Path) -> usize {
    let mut removed = 0;
    let now = SystemTime::now();
    let older_than = |meta: &fs::Metadata, age: Duration| {
        meta.modified()
            .ok()
            .and_then(|m| now.duration_since(m).ok())
            .is_some_and(|d| d > age)
    };

    if let Ok(rd) = fs::read_dir(base.join(STAGING)) {
        for ent in rd.flatten() {
            let path = ent.path();
            // `symlink_metadata`: a symlink planted in here is not ours to
            // follow, and `remove_dir_all` on its target would be a delete
            // of someone else's files.
            let Ok(meta) = fs::symlink_metadata(&path) else {
                continue;
            };
            if !meta.is_dir() {
                continue;
            }
            match File::open(path.join(LOCK)) {
                Ok(lock) => match lock.try_lock() {
                    // Nobody holds it, so no run owns it. Keep the lock
                    // while removing, so a racing `create_in` notices.
                    Ok(()) => {
                        if fs::remove_dir_all(&path).is_ok() {
                            removed += 1;
                        }
                    }
                    Err(_) => continue,
                },
                Err(e) if e.kind() == ErrorKind::NotFound => {
                    if older_than(&meta, UNLOCKED_GRACE) && fs::remove_dir_all(&path).is_ok() {
                        removed += 1;
                    }
                }
                Err(_) => continue,
            }
        }
    }

    if let Ok(rd) = fs::read_dir(base) {
        for ent in rd.flatten() {
            let name = ent.file_name();
            if !name.to_string_lossy().starts_with("pelican-") {
                continue;
            }
            let Ok(meta) = fs::symlink_metadata(ent.path()) else {
                continue;
            };
            if meta.is_file()
                && older_than(&meta, LEGACY_MAX_AGE)
                && fs::remove_file(ent.path()).is_ok()
            {
                removed += 1;
            }
        }
    }
    removed
}

/// Unique across processes (pid) and within one (counter), and sortable by
/// start time for anyone reading the directory by hand.
fn run_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let secs = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let seq = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("{secs}-{}-{seq}", std::process::id())
}

/// `mkdir` that fails if the path exists, owner-only on Unix: the files in
/// here are the user's music.
fn mkdir_private(path: &Path) -> std::io::Result<()> {
    let mut b = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        b.mode(0o700);
    }
    b.create(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn staging_entries(base: &Path) -> Vec<PathBuf> {
        fs::read_dir(base.join(STAGING))
            .map(|rd| rd.flatten().map(|e| e.path()).collect())
            .unwrap_or_default()
    }

    #[test]
    fn dropped_after_success_leaves_nothing() {
        let base = tempfile::tempdir().unwrap();
        let path = {
            let s = StagingDir::create_in(base.path()).unwrap();
            fs::write(s.file(1), b"mp3").unwrap();
            assert!(s.file(1).starts_with(s.path()));
            s.path().to_path_buf()
        };
        assert!(!path.exists());
        assert!(staging_entries(base.path()).is_empty());
    }

    #[test]
    fn an_error_return_leaves_nothing() {
        let base = tempfile::tempdir().unwrap();
        let mut seen = None;
        let run = |seen: &mut Option<PathBuf>| -> Result<()> {
            let s = StagingDir::create_in(base.path())?;
            fs::write(s.file(0), b"half an album")?;
            *seen = Some(s.path().to_path_buf());
            Err(anyhow!("ffmpeg failed"))
        };
        assert!(run(&mut seen).is_err());
        assert!(!seen.unwrap().exists());
    }

    #[test]
    fn a_panic_leaves_nothing() {
        let base = tempfile::tempdir().unwrap();
        let b = base.path().to_path_buf();
        let r = std::panic::catch_unwind(move || {
            let s = StagingDir::create_in(&b).unwrap();
            fs::write(s.file(0), b"x").unwrap();
            panic!("mid-run");
        });
        assert!(r.is_err());
        assert!(staging_entries(base.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let base = tempfile::tempdir().unwrap();
        let s = StagingDir::create_in(base.path()).unwrap();
        let mode = fs::metadata(s.path()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn two_runs_get_two_directories() {
        let base = tempfile::tempdir().unwrap();
        let a = StagingDir::create_in(base.path()).unwrap();
        let b = StagingDir::create_in(base.path()).unwrap();
        assert_ne!(a.path(), b.path());
        assert_ne!(a.run_id(), b.run_id());
    }

    /// The Ctrl-C case: the process died, the kernel released its lock,
    /// the directory is still there. Simulated by making one by hand with
    /// a `.lock` nobody holds.
    #[test]
    fn sweep_removes_an_interrupted_run() {
        let base = tempfile::tempdir().unwrap();
        let dead = base.path().join(STAGING).join("1-99999-0");
        fs::create_dir_all(&dead).unwrap();
        fs::write(dead.join(LOCK), b"").unwrap();
        fs::write(dead.join("00001.mp3"), b"orphan").unwrap();
        assert_eq!(sweep_in(base.path()), 1);
        assert!(!dead.exists());
    }

    #[test]
    fn sweep_leaves_a_live_run_alone() {
        let base = tempfile::tempdir().unwrap();
        let live = StagingDir::create_in(base.path()).unwrap();
        fs::write(live.file(0), b"in use").unwrap();
        assert_eq!(sweep_in(base.path()), 0);
        assert!(live.file(0).exists());
    }

    #[test]
    fn sweep_spares_a_fresh_unlocked_dir_and_takes_a_stale_one() {
        let base = tempfile::tempdir().unwrap();
        let fresh = base.path().join(STAGING).join("fresh");
        let stale = base.path().join(STAGING).join("stale");
        fs::create_dir_all(&fresh).unwrap();
        fs::create_dir_all(&stale).unwrap();
        let old = SystemTime::now() - UNLOCKED_GRACE - Duration::from_secs(60);
        File::open(&stale).unwrap().set_modified(old).unwrap();
        assert_eq!(sweep_in(base.path()), 1);
        assert!(fresh.exists());
        assert!(!stale.exists());
    }

    #[cfg(unix)]
    #[test]
    fn sweep_never_follows_a_symlink() {
        let base = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        fs::write(elsewhere.path().join(LOCK), b"").unwrap();
        fs::write(elsewhere.path().join("precious.flac"), b"keep").unwrap();
        fs::create_dir_all(base.path().join(STAGING)).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), base.path().join(STAGING).join("link"))
            .unwrap();
        sweep_in(base.path());
        assert!(elsewhere.path().join("precious.flac").exists());
    }

    #[test]
    fn sweep_clears_legacy_loose_files_only_when_old() {
        let base = tempfile::tempdir().unwrap();
        let old = base.path().join("pelican-1-2-3.mp3");
        let new = base.path().join("pelican-4-5-6.mp3");
        let other = base.path().join("not-ours.txt");
        for p in [&old, &new, &other] {
            fs::write(p, b"x").unwrap();
        }
        let t = SystemTime::now() - LEGACY_MAX_AGE - Duration::from_secs(60);
        for p in [&old, &other] {
            File::options()
                .write(true)
                .open(p)
                .unwrap()
                .set_modified(t)
                .unwrap();
        }
        assert_eq!(sweep_in(base.path()), 1);
        assert!(!old.exists());
        assert!(new.exists());
        assert!(other.exists(), "only pelican-* files are ours");
    }
}
