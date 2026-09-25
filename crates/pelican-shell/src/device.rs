//! The one thread that is allowed to touch the watch.
//!
//! Garmin firmware on the FR165 allows a single MTP session at a time, and
//! `transfer::run` needs a *fresh* session per file. Both of those are easy
//! to violate by accident from a UI. So the invariant is made structural
//! rather than left as a discipline: exactly one thread owns the backend,
//! every device operation is a branch of that thread's loop, and browsing
//! and syncing therefore cannot overlap — the loop is in one of them or the
//! other, never both.
//!
//! Command handlers post [`DeviceOp`]s here and return immediately. Nothing
//! that talks to the device ever runs on the webview's thread.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use pelican_core::garmin::{self, Device, MUSIC_FOLDER};
use pelican_core::mtp::{self, Backend};
use pelican_core::{history, transfer};

use crate::dto::{self, EventSink, RemoteEntryDto, UiEvent};

/// Progress fires per 256 KiB — roughly 4,000 messages per gigabyte. Sending
/// each one across IPC would spend the webview's whole frame budget parsing
/// JSON about a bar that moved a pixel. Cap the rate; the final tick of every
/// file is always let through so the bar lands exactly on full.
const PROGRESS_MIN_INTERVAL: Duration = Duration::from_millis(33);

pub enum DeviceOp {
    Connect {
        serial: Option<String>,
    },
    Disconnect,
    /// Remove objects from the watch. A batch, because the wall selects a
    /// batch: N separate ops would be N separate `snapshot()` round trips and
    /// N chances for the list under the user's cursor to shift mid-gesture.
    Delete {
        paths: Vec<String>,
    },
    /// Drop journal rows. A row the device does not list has nothing to
    /// delete; clearing Pelican's own record is the only honest action left,
    /// and it must not be dressed up as touching the watch.
    Forget {
        names: Vec<String>,
    },
    StartSync {
        paths: Vec<PathBuf>,
        skip_tag_check: bool,
        /// What to do about a name the watch already holds. `Skip` on every
        /// ordinary send; `Rename` only when the user has been shown the
        /// collision and asked for it.
        on_conflict: transfer::OnConflict,
    },
}

/// Shared handles the command layer needs. Cheap to clone, all `Arc`/channel.
pub struct DeviceHandle {
    pub ops: Sender<DeviceOp>,
    /// True while a sync is running. Commands that would queue behind it
    /// refuse instead — the UI has no affordance for them mid-sync, and
    /// silently queueing would make a click appear to do nothing for minutes.
    pub busy: Arc<AtomicBool>,
    /// Set by `stop_sync`. Checked between files, which is the only place a
    /// truthful cancellation point exists.
    pub cancel: Arc<AtomicBool>,
}

impl DeviceHandle {
    pub fn is_busy(&self) -> bool {
        self.busy.load(Ordering::SeqCst)
    }

    pub fn post(&self, op: DeviceOp) -> Result<(), String> {
        self.ops
            .send(op)
            .map_err(|_| "the device worker has stopped; restart Pelican".to_string())
    }
}

/// Spawn the device thread and hand back the handles used to drive it.
pub fn spawn(sink: Arc<EventSink>) -> DeviceHandle {
    let (ops, rx) = crossbeam_channel::unbounded();
    let busy = Arc::new(AtomicBool::new(false));
    let cancel = Arc::new(AtomicBool::new(false));

    let worker = Worker {
        sink,
        busy: busy.clone(),
        cancel: cancel.clone(),
        backend: None,
        device: None,
    };
    std::thread::Builder::new()
        .name("pelican-device".into())
        .spawn(move || worker.run(rx))
        .expect("spawning the device thread");

    DeviceHandle { ops, busy, cancel }
}

struct Worker {
    sink: Arc<EventSink>,
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
    backend: Option<Box<dyn Backend>>,
    device: Option<Device>,
}

impl Worker {
    fn run(mut self, rx: Receiver<DeviceOp>) {
        while let Ok(op) = rx.recv() {
            match op {
                DeviceOp::Connect { serial } => self.connect(serial.as_deref()),
                DeviceOp::Disconnect => {
                    // Dropping the backend closes the session. This is the
                    // escape hatch when a previous session leaked.
                    self.backend = None;
                    self.device = None;
                    self.sink.send(UiEvent::Detached);
                }
                DeviceOp::Delete { paths } => self.delete(&paths),
                DeviceOp::Forget { names } => self.forget(&names),
                DeviceOp::StartSync {
                    paths,
                    skip_tag_check,
                    on_conflict,
                } => self.start_sync(paths, skip_tag_check, on_conflict),
            }
        }
    }

    /// Key the upload journal by serial. A watch that reports no serial still
    /// gets a journal, just a shared one — better than losing the record
    /// entirely, which would leave the user with no idea what is aboard.
    fn serial(&self) -> String {
        self.device
            .as_ref()
            .and_then(|d| d.serial.clone())
            .unwrap_or_else(|| "unknown".to_string())
    }

    fn fail(&self, e: &anyhow::Error) {
        let message = dto::err(e);
        // `mtp::open` already folded the platform's explanation into the
        // context chain, so the text is complete. The flag is what tells the
        // UI to treat it as a standing condition rather than a blip.
        let contention = is_contention(&message).then(|| message.clone());
        self.sink.send(UiEvent::Error {
            message,
            contention,
        });
    }

    fn connect(&mut self, serial: Option<&str>) {
        // Release any session we hold before asking for another one. The
        // watch will refuse the second open otherwise, and the error looks
        // like someone else's fault.
        self.backend = None;

        let device = match garmin::pick_device(serial) {
            Ok(d) => d,
            Err(e) => return self.fail(&e),
        };
        match mtp::open(&device) {
            Ok(b) => {
                self.backend = Some(b);
                self.device = Some(device);
                self.snapshot();
            }
            Err(e) => {
                self.device = None;
                self.fail(&e);
            }
        }
    }

    /// Delete a batch, then reconcile the journal, then snapshot once.
    ///
    /// The journal write matters as much as the device write. Without it a
    /// file removed from the watch reappears in the wall the same second, as
    /// "Sent today · the watch is not listing it now" — a true statement
    /// about Pelican's record and a useless one to the person who just
    /// removed it.
    fn delete(&mut self, paths: &[String]) {
        let serial = self.serial();
        let mut removed: Vec<String> = Vec::new();
        let mut failures: Vec<(String, anyhow::Error)> = Vec::new();

        // The braces are still load-bearing: `snapshot` below needs the same
        // `&mut self` this borrow of `backend` would otherwise still hold.
        {
            let Some(b) = self.backend.as_mut() else {
                return self.sink.send_error("no watch is connected");
            };
            for path in paths {
                match b.delete(path) {
                    Ok(()) => removed.push(path.clone()),
                    Err(e) => failures.push((leaf(path), e)),
                }
            }
        }

        for (name, e) in &failures {
            self.sink.send(UiEvent::DeleteFailed {
                name: name.clone(),
                // Verbatim, same contract as `FileFailed`: the engine's text
                // is the only thing that says whether a retry is safe.
                error: dto::err(e),
            });
        }
        if !removed.is_empty() {
            if let Err(e) = history::forget_paths(&serial, &removed) {
                self.sink.send_error(&format!(
                    "Removed from your watch, but Pelican could not update its own record: {e:#}"
                ));
            }
        }
        self.sink.send(UiEvent::Deleted {
            ok: removed.len() as u32,
            failed: failures.len() as u32,
        });
        self.snapshot();
    }

    /// Forget journal rows. Nothing on the watch is touched, and the UI's
    /// copy for this action says so.
    fn forget(&mut self, names: &[String]) {
        let serial = self.serial();
        if let Err(e) = history::forget(&serial, names) {
            return self.fail(&e);
        }
        // The wall reads `uploads` off the snapshot, so this is what makes
        // the forgotten rows leave the screen.
        if self.backend.is_some() {
            self.snapshot();
        } else {
            self.sink.send(UiEvent::Snapshot {
                free: 0,
                total: 0,
                model: None,
                entries: Vec::new(),
                uploads: history::load(&serial).uploads,
            });
        }
    }

    /// Read the whole device state in one pass and emit it.
    ///
    /// Partial failure still produces a `Snapshot`: a `free_space` that fails
    /// should not blank the file list, and a `list_dir` that fails should not
    /// hide the capacity. The error is reported alongside.
    fn snapshot(&mut self) {
        let serial = self.serial();
        let Some(backend) = self.backend.as_mut() else {
            return;
        };

        let mut problem: Option<anyhow::Error> = None;
        let (free, total) = match backend.free_space() {
            Ok(v) => v,
            Err(e) => {
                problem = Some(e);
                (0, 0)
            }
        };
        let entries = match backend.list_dir(MUSIC_FOLDER) {
            Ok(v) => v.iter().map(RemoteEntryDto::from).collect(),
            Err(e) => {
                problem = problem.or(Some(e));
                Vec::new()
            }
        };
        // Cached on the session; no extra device I/O.
        let model = backend.model();

        if let Some(e) = &problem {
            self.fail(e);
        }
        self.sink.send(UiEvent::Snapshot {
            free,
            total,
            model,
            entries,
            uploads: history::load(&serial).uploads,
        });
    }

    fn start_sync(
        &mut self,
        paths: Vec<PathBuf>,
        skip_tag_check: bool,
        on_conflict: transfer::OnConflict,
    ) {
        // `busy` is claimed synchronously by the command, not here — see
        // `commands::start_sync`. So every exit from this function has to
        // release it, and a missed early return would leave Send dead for the
        // life of the process. The guard makes that structural instead of a
        // thing to remember.
        let _busy = BusyGuard(self.busy.clone());

        let Some(device) = self.device.clone() else {
            return self.sink.send_error("no watch is connected");
        };

        // What the watch was listing *before* the run. Reconciliation below
        // only credits a stem that appears afterwards and was not here
        // already: MTP has no overwrite, so a same-named object from an
        // earlier send can sit in /Music, and crediting it would turn a real
        // failure into a false "it landed after all".
        let before = self.music_before();

        // Release the browsing session *before* the first file, structurally.
        // `transfer::run` opens its own session per file and would otherwise
        // be contending with us for the one slot the firmware offers.
        self.backend = None;
        self.cancel.store(false, Ordering::SeqCst);

        let (outcome, ledger) = self.drain_plan(
            &device,
            paths,
            skip_tag_check,
            before.as_ref().map(|b| b.names.as_slice()),
            on_conflict,
        );

        self.sink.send(outcome);

        // Reopen so the wall and the waterline reflect what actually landed.
        let serial = device.serial.clone();
        self.device = Some(device);
        self.connect(serial.as_deref());

        // The observation beats the inference. `connect` has just emitted the
        // authoritative listing; anything the run called failed or uncertain
        // that the watch is now listing, and was not listing before, is on the
        // watch — and the sentence already on screen has to be revised rather
        // than left standing. PRODUCT.md's tie-breaker is explicit about
        // which of the two wins.
        self.reconcile(&ledger, before.map(|b| b.stems));
    }

    /// What `/Music` held before the run, in the two shapes the run needs —
    /// from **one** listing.
    ///
    /// `None` is not an empty picture. Without a *before* picture there is no
    /// evidence a stem is new, so reconciliation declines to run rather than
    /// guessing, and the collision resolver has nothing to compare against.
    /// The whole point of both is that they are observations.
    ///
    /// This is the listing `start_sync` already took before releasing the
    /// browsing session; projecting it twice costs nothing, and the count
    /// does not grow with plan size. The plan targets exactly one directory
    /// today (`expand_inputs` uses `MUSIC_FOLDER` with flatten on), so one
    /// walk covers all of it.
    fn music_before(&mut self) -> Option<MusicBefore> {
        let backend = self.backend.as_mut()?;
        // The one and only listing. Everything the run needs to know about
        // the watch's prior contents is projected out of this vector.
        let entries = backend.list_dir(MUSIC_FOLDER).ok()?;
        Some(project_before(&entries))
    }

    fn reconcile(
        &mut self,
        ledger: &[JobOutcome],
        before: Option<std::collections::HashSet<String>>,
    ) {
        let unresolved: Vec<&JobOutcome> = ledger
            .iter()
            .filter(|j| matches!(j.outcome, Outcome::Failed | Outcome::Uncertain))
            .collect();
        if unresolved.is_empty() {
            return;
        }
        let Some(before) = before else { return };
        let Some(backend) = self.backend.as_mut() else {
            return;
        };
        let Ok(entries) = backend.list_dir(MUSIC_FOLDER) else {
            return;
        };
        // stem → size, from the fresh listing. Broken handles are excluded:
        // a stub is the failure, not a recovery from one.
        let after: std::collections::HashMap<String, u64> = entries
            .iter()
            .filter(|e| !e.is_folder && !e.is_broken)
            .map(|e| (stem_key(&e.name), e.size))
            .collect();

        let serial = self.serial();
        let mut landed = Vec::new();
        let mut journal_problem = false;
        for job in unresolved {
            let key = job.stem.to_lowercase();
            if before.contains(&key) {
                continue;
            }
            let Some(&size) = after.get(&key) else {
                continue;
            };
            // The reconciliation write from the ruling: additive only. A row
            // is never removed here, because a crash between `Done` and this
            // listing must not be able to lose one.
            if history::record_upload(&serial, &job.stem, size, job.tags.as_ref()).is_err() {
                journal_problem = true;
            }
            landed.push(job.stem.clone());
        }
        if journal_problem {
            self.sink.send_error(
                "Pelican sent the file but could not record it. Its list of what it has sent \
                 may be incomplete.",
            );
        }
        if !landed.is_empty() {
            // Re-emit the picture, because the journal rows just written are
            // part of it and the snapshot `connect` sent predates them. Only
            // on a correction: a run with nothing to reconcile has already
            // paid for one listing and does not need a second.
            self.snapshot();
            self.sink.send(UiEvent::SyncReconciled { landed });
        }
    }

    /// Hand the whole plan to the engine in **one** call, and render what
    /// comes back.
    ///
    /// It used to be a call per file, which read well and cost badly: the
    /// write-time collision guard reads each target directory once per run
    /// and remembers it, so a run of one file re-read `/Music` for every
    /// track and the run's memory of what it had written never survived past
    /// the file that wrote it. One call keeps both. Stop is honoured inside
    /// the engine, between files — the same granularity this loop used to
    /// give it, and the only granularity a PTP data phase allows.
    ///
    /// `existing` is `/Music` as the watch reported it before the run, from
    /// the single listing `music_before` took. `None` means the listing could
    /// not be read, and the plan-time collision check is skipped rather than
    /// guessed at — the write-time guard in `transfer::run` still refuses to
    /// write a name it cannot prove is free.
    fn drain_plan(
        &self,
        device: &Device,
        paths: Vec<PathBuf>,
        skip_tag_check: bool,
        existing: Option<&[(String, String)]>,
        on_conflict: transfer::OnConflict,
    ) -> (UiEvent, Vec<JobOutcome>) {
        // `skip_tag_check` is true by default and the interface depends on it:
        // the copy promises an untagged file *transfers* and merely stays
        // invisible. With it false the engine would silently skip such files
        // and the promise would be a lie.
        let opts = transfer::Options {
            skip_tag_check,
            transcode: true,
        };

        // Recursive read_dir. On the device thread, never inline in a command.
        let mut jobs = match transfer::expand_inputs(&paths, opts.transcode) {
            Ok(j) => j,
            Err(e) => {
                self.fail(&e);
                return (
                    UiEvent::SyncFinished {
                        ok: 0,
                        skipped: 0,
                        failed: 0,
                        stopped: false,
                        delivered_bytes: 0,
                        planned_bytes: 0,
                    },
                    Vec::new(),
                );
            }
        };

        // Compare the plan against what the watch already holds, before a
        // single byte moves. MTP has no overwrite: per docs/garmin-mtp.md
        // section 7, writing a name that is taken turns *both* objects into
        // unreadable stubs, and section 6 records that no DeleteObject
        // against a stub has ever succeeded. So a collision destroys a file
        // the user already had, and the only safe answers are to refuse or to
        // move — never to write.
        //
        // Renaming is confined to here, on purpose. The journal key below is
        // computed from `job.remote_name`, so a rename decided later, inside
        // `transfer::run`, would journal a name that was never written: the
        // wall would show a phantom row and treat the file that did land as
        // unknown-provenance. Plan time is the last moment the shell still
        // knows the final name.
        let conflicts = match existing {
            Some(existing) => {
                transfer::resolve_conflicts(&mut jobs, existing, on_conflict, opts.transcode)
            }
            None => Vec::new(),
        };
        // Under `Skip` a refused job is gone from the plan. It is still part
        // of the run: a collision the user is not told about is a track they
        // believe they sent and did not.
        let refused: Vec<&transfer::Conflict> = conflicts
            .iter()
            .filter(|c| c.renamed_to.is_none())
            .collect();
        // Under `Rename` the job stays, under a name the user did not choose,
        // so the completion line has to say which.
        let renamed: std::collections::HashMap<PathBuf, String> = conflicts
            .iter()
            .filter_map(|c| c.renamed_to.clone().map(|n| (c.src.clone(), n)))
            .collect();

        let total = (jobs.len() + refused.len()) as u32;
        // Source bytes are the only denominator available up front — the
        // converted size of a FLAC is not known until ffmpeg has finished
        // writing it. Weighting the meter by source size still makes it
        // monotone and proportional to the work remaining, which is what the
        // bar is for.
        let sizes: Vec<u64> = jobs.iter().map(|j| file_len(&j.src)).collect();
        // Refused files count toward the denominator they will never fill.
        // Leaving them out would let a run that declined to send half the
        // selection finish with a full bar.
        let job_total: u64 =
            sizes.iter().sum::<u64>() + refused.iter().map(|c| file_len(&c.src)).sum::<u64>();
        self.sink.send(UiEvent::Planned {
            total,
            bytes: job_total,
        });

        let serial = self.serial();
        let mut tally = Tally::default();
        let mut base_bytes = 0u64;
        let mut ledger: Vec<JobOutcome> = Vec::new();
        // One `Error` per run, not one per file. A journal that cannot be
        // written is worth saying once; saying it twenty times would bury the
        // failures the user actually has to act on.
        let mut journal_problem = false;

        // Report the refusals first, and in full. Each carries its own path
        // so the interface can offer to re-send exactly these under new
        // names, and the sentence is the engine's — it names the file on the
        // watch and what sending over it would have done.
        for c in &refused {
            let completed = tally.skip();
            let path = c.src.display().to_string();
            self.sink.send(UiEvent::FileSkipped {
                completed,
                total,
                ok: tally.ok,
                skipped: tally.skipped,
                failed: tally.failed,
                name: c
                    .src
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| path.clone()),
                path,
                kind: dto::SkipKind::NameTaken,
                reason: transfer::name_taken_reason(&c.existing),
            });
        }

        // **One `transfer::run` call for the whole plan**, not one per file.
        //
        // The write-time collision guard reads each target directory once per
        // *run* and maintains what it knows from there, so handing it the
        // plan a file at a time throws that away: every file would pay for
        // its own folder walk. On a full `/Music` that is the difference
        // between one enumeration and one per track.
        //
        // Stop is honoured by the engine, between files. That is the only
        // place it can be honoured safely — a PTP data phase cannot be
        // aborted without leaving an object on the watch that the firmware
        // will not delete — and it is where this loop used to honour it too.
        //
        // Everything the events cannot carry (the journal stem, the source
        // tags, the byte weight, the name a collision moved the file to) is
        // computed up front and looked up by source path.
        let mut staged: Vec<Staged> = Vec::with_capacity(jobs.len());
        let mut index: HashMap<PathBuf, usize> = HashMap::with_capacity(jobs.len());
        for (job, src_len) in jobs.iter().zip(&sizes) {
            let path = job.src.display().to_string();
            let name = job
                .src
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            index.insert(job.src.clone(), staged.len());
            staged.push(Staged {
                // Journal the name that will actually be written to the
                // watch, not the local one. `transfer` maps punctuation to
                // dashes, collapses runs and truncates the stem to 56 chars,
                // so the local stem matches nothing the device will ever
                // report — and the wall, which dedupes the journal against
                // the listing, would show every synced track twice under two
                // different names.
                stem: journal_stem(&job.remote_name, &name),
                name,
                path,
                // Set only when the collision resolver moved this file. The
                // name on the watch is not the name on disk, and the report
                // says so.
                renamed_to: renamed.get(&job.src).cloned(),
                // Same `read_fast` call `scan.rs` already makes per file —
                // one cheap metadata read per upload, on a worker thread and
                // never on the webview's.
                tags: pelican_core::transcode::tags::read_fast(&job.src)
                    .ok()
                    .map(|i| i.tags),
                src_len: *src_len,
                outcome: Outcome::Skipped,
                settled: false,
            });
        }

        let (tx, rx) = transfer::channel();
        let mut last_emit = Instant::now() - PROGRESS_MIN_INTERVAL;
        let cancel = self.cancel.clone();

        // The upload runs on a scoped thread purely so this thread can drain
        // progress live; it is still one thread at a time on the device,
        // because this one does nothing but read `rx` until the worker has
        // finished and dropped its sender.
        std::thread::scope(|s| {
            let opts = &opts;
            s.spawn(move || {
                transfer::run_cancellable(device, jobs, opts, &tx, &|| {
                    cancel.load(Ordering::SeqCst)
                });
                drop(tx);
            });

            for ev in rx.iter() {
                // Our plan is the real one, emitted once above; the engine's
                // own bookkeeping for the same plan is discarded.
                let at = match &ev {
                    transfer::Event::Planned { .. } | transfer::Event::Finished(_) => continue,
                    transfer::Event::Started(at)
                    | transfer::Event::Staging(at)
                    | transfer::Event::Progress { at, .. }
                    | transfer::Event::Skipped { at, .. }
                    | transfer::Event::Done { at, .. }
                    | transfer::Event::Failed { at, .. } => at,
                };
                // Every job in the plan was indexed above, so this cannot
                // miss. If it ever did, dropping the event beats attributing
                // it to the wrong track.
                let Some(&i) = index.get(&at.src) else {
                    continue;
                };

                match ev {
                    transfer::Event::Planned { .. } | transfer::Event::Finished(_) => {}
                    transfer::Event::Started(_) => {
                        self.sink.send(UiEvent::FileStarted {
                            completed: tally.in_flight(),
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                            path: staged[i].path.clone(),
                        });
                    }
                    transfer::Event::Staging(_) => {
                        self.sink.send(UiEvent::FileStaging {
                            completed: tally.in_flight(),
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                        });
                    }
                    transfer::Event::Progress {
                        transferred,
                        total_bytes,
                        ..
                    } => {
                        let final_tick = total_bytes > 0 && transferred >= total_bytes;
                        if !final_tick && last_emit.elapsed() < PROGRESS_MIN_INTERVAL {
                            continue;
                        }
                        last_emit = Instant::now();
                        let frac = if total_bytes > 0 {
                            transferred as f64 / total_bytes as f64
                        } else {
                            0.0
                        };
                        self.sink.send(UiEvent::FileProgress {
                            completed: tally.in_flight(),
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                            file_bytes: transferred,
                            file_total: total_bytes,
                            job_bytes: base_bytes + (staged[i].src_len as f64 * frac) as u64,
                            job_total,
                        });
                    }
                    transfer::Event::Done { bytes, .. } => {
                        let completed = tally.done();
                        staged[i].outcome = Outcome::Ok;
                        staged[i].settled = true;
                        // Byte-weighted progress advances only here. It used
                        // to advance for every job, which walked the meter to
                        // full on a run where nothing landed.
                        base_bytes += staged[i].src_len;
                        // `transfer::run` never writes history. This is our
                        // own record that we sent the file; whether it is
                        // still on the watch is answered by the next listing,
                        // not by this. Kept optimistic on purpose: a crash
                        // between here and the post-run listing must not lose
                        // the only record there is.
                        if history::record_upload(
                            &serial,
                            &staged[i].stem,
                            bytes,
                            staged[i].tags.as_ref(),
                        )
                        .is_err()
                        {
                            journal_problem = true;
                        }
                        self.sink.send(UiEvent::FileDone {
                            completed,
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                            bytes,
                            renamed_to: staged[i].renamed_to.clone(),
                        });
                    }
                    transfer::Event::Skipped { reason, .. } => {
                        let completed = tally.skip();
                        staged[i].outcome = Outcome::Skipped;
                        staged[i].settled = true;
                        self.sink.send(UiEvent::FileSkipped {
                            completed,
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                            path: staged[i].path.clone(),
                            kind: dto::classify_skip(&reason),
                            reason,
                        });
                    }
                    transfer::Event::Failed { error, .. } => {
                        let completed = tally.fail();
                        let kind = dto::classify_fail(&error);
                        // "Uncertain" is the drained-but-unconfirmed case:
                        // every byte crossed the wire and the watch did not
                        // answer. The post-run listing is what resolves it,
                        // which is why it is recorded rather than collapsed
                        // into a plain failure here.
                        staged[i].outcome = match kind {
                            dto::FailKind::Unconfirmed => Outcome::Uncertain,
                            _ => Outcome::Failed,
                        };
                        staged[i].settled = true;
                        // Never terminal: `transfer::run` continues past
                        // every failure, and so does this loop.
                        self.sink.send(UiEvent::FileFailed {
                            completed,
                            total,
                            ok: tally.ok,
                            skipped: tally.skipped,
                            failed: tally.failed,
                            name: staged[i].name.clone(),
                            kind,
                            error,
                        });
                    }
                }
            }
        });

        // "Stopped" is a claim about the plan, not about the button. Pressing
        // Stop on the last file of a run that then finished it is not a
        // stopped run, and the summary must not say it was.
        let stopped = self.cancel.load(Ordering::SeqCst) && staged.iter().any(|j| !j.settled);

        ledger.extend(
            staged
                .into_iter()
                .filter(|j| j.settled)
                .map(|j| JobOutcome {
                    stem: j.stem,
                    outcome: j.outcome,
                    tags: j.tags,
                }),
        );

        if journal_problem {
            self.sink.send_error(
                "Pelican sent the file but could not record it. Its list of what it has sent \
                 may be incomplete.",
            );
        }

        (
            UiEvent::SyncFinished {
                ok: tally.ok,
                skipped: tally.skipped,
                failed: tally.failed,
                stopped,
                delivered_bytes: base_bytes,
                planned_bytes: job_total,
            },
            ledger,
        )
    }
}

/// One planned file, and everything about it the engine's events do not
/// carry.
///
/// The engine reports by source path; the interface reports by name, byte
/// weight and journal stem. This is the join between them, built once before
/// the run so nothing is recomputed per event.
struct Staged {
    /// The local filename, which is what the interface shows.
    name: String,
    /// The full local path, which is what "send this one anyway" needs.
    path: String,
    /// The stem the *watch* will report, which is what the journal keys on.
    stem: String,
    /// Set when the collision resolver moved this file to a name the user
    /// did not choose. The completion line has to say which.
    renamed_to: Option<String>,
    tags: Option<pelican_core::transcode::tags::Tags>,
    /// Source bytes, for the byte-weighted meter.
    src_len: u64,
    outcome: Outcome,
    /// True once this file reached a terminal event. A run the user stopped
    /// leaves the untouched tail false, and neither the ledger nor the
    /// "stopped" claim counts them.
    settled: bool,
}

/// Split one `/Music` listing into the two pictures a run needs.
///
/// A free function so the two filters can be tested without a watch: they
/// differ, and the difference is load-bearing.
fn project_before(entries: &[mtp::RemoteEntry]) -> MusicBefore {
    MusicBefore {
        // Reconciliation credits a stem that appears *after* the run and was
        // not here before, so a broken stub — which is not a file the user
        // can play — must not count as already present.
        stems: entries
            .iter()
            .filter(|e| !e.is_folder && !e.is_broken)
            .map(|e| stem_key(&e.name))
            .collect(),
        // Collisions are a different question, and its answer includes the
        // stubs: a handle occupying a name occupies it whether or not its
        // metadata reads. Their synthesized names can never match a real one,
        // which is the guard's known blind spot — recorded in
        // docs/garmin-mtp.md section 7, not papered over.
        names: entries
            .iter()
            .filter(|e| !e.is_folder)
            .map(|e| (MUSIC_FOLDER.to_string(), e.name.clone()))
            .collect(),
    }
}

/// What `/Music` held before a run, projected two ways from one listing.
#[derive(Debug)]
struct MusicBefore {
    /// Lowercased stems of the readable files, for reconciliation.
    stems: std::collections::HashSet<String>,
    /// `(remote_dir, remote_name)` exactly as the watch reports them, for
    /// `transfer::resolve_conflicts`. Includes broken stubs, whose names the
    /// firmware will not read back — the guard's known blind spot.
    names: Vec<(String, String)>,
}

/// One job's fate, kept so the post-run listing can revise it.
struct JobOutcome {
    /// The sanitized remote stem — the name the watch will report, and the
    /// key both the journal and the wall use.
    stem: String,
    outcome: Outcome,
    /// Read from the source at send time, so a reconciled row is journalled
    /// with the same tags an immediately-successful one would have been.
    tags: Option<pelican_core::transcode::tags::Tags>,
}

enum Outcome {
    Ok,
    Skipped,
    Failed,
    /// The bytes drained but the watch never confirmed the write. Not a
    /// success and not yet a failure — only the listing can say.
    Uncertain,
}

/// Releases `busy` however `start_sync` returns.
///
/// The flag is claimed on the webview thread, in the command, to close the
/// window where two fast clicks both pass an `is_busy()` check and both queue
/// a run. That makes releasing it this thread's job on every path, including
/// the panicking one.
struct BusyGuard(Arc<AtomicBool>);

impl Drop for BusyGuard {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

/// The numerator in "3 of 12", and the reason it is not `Progress::done`.
///
/// `transfer::Progress::done` counts outcomes resolved *before* the current
/// file, so it is 0 for every event of the first file — including that
/// file's `Done`. Forwarded verbatim, the UI reads "0 of 12" after the first
/// track finishes and only reaches 11 at the end. In-flight events carry the
/// count so far; a terminal event carries the count *including itself*.
///
/// A skip or a failure advances it too. The user asked how many files are
/// behind them, not how many succeeded — that is what `SyncFinished` is for.
///
/// `in_flight` is position in the batch, not a success count, and that is why
/// it is **never rendered on its own**. `ok`/`skipped`/`failed` ride alongside
/// it on every `file*` event so the interface can say which of the three a
/// given position was: "3 of 5" is a true statement about where the run is,
/// and reading it as "3 sent" is the defect. Fixed in the rendering, not here.
#[derive(Default)]
struct Tally {
    ok: u32,
    skipped: u32,
    failed: u32,
}

impl Tally {
    fn in_flight(&self) -> u32 {
        self.ok + self.skipped + self.failed
    }
    fn done(&mut self) -> u32 {
        self.ok += 1;
        self.in_flight()
    }
    fn skip(&mut self) -> u32 {
        self.skipped += 1;
        self.in_flight()
    }
    fn fail(&mut self) -> u32 {
        self.failed += 1;
        self.in_flight()
    }
}

impl EventSink {
    /// A message that is ours, not the engine's — no context chain to keep.
    pub(crate) fn send_error(&self, message: &str) {
        self.send(UiEvent::Error {
            message: message.to_string(),
            contention: None,
        });
    }
}

fn is_contention(msg: &str) -> bool {
    let m = msg.to_ascii_lowercase();
    m.contains("exclusive access") || m.contains("busy") || m.contains("access is denied")
}

/// The key the upload journal is written under: the sanitized stem of the
/// name the file lands on the device as.
///
/// The wall in the UI merges two lists — what `list_dir("Music")` reports and
/// what this journal holds — and dedupes them by stem. That only works if
/// both sides spell the name the same way, so the journal has to record the
/// *remote* name. `sanitize_filename_stem` is applied again here rather than
/// trusted from the planner: it is idempotent, and this way the invariant
/// holds even if a future path hands over an unsanitized `remote_name`.
fn journal_stem(remote_name: &str, fallback: &str) -> String {
    let stem = std::path::Path::new(remote_name)
        .file_stem()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| fallback.to_string());
    pelican_core::transcode::sanitize_filename_stem(&stem)
}

/// The dedupe key both sides of the wall use: last path segment, extension
/// removed, lowercased. Matches `history::forget`'s key and `stemOf` in
/// `ui/app.js`. A leading dot is not an extension separator.
fn stem_key(name: &str) -> String {
    let leaf = name.rsplit('/').next().unwrap_or(name);
    match leaf.rfind('.') {
        Some(i) if i > 0 => leaf[..i].to_lowercase(),
        _ => leaf.to_lowercase(),
    }
}

/// Last path segment, for a message that has no room for a device path.
fn leaf(path: &str) -> String {
    path.rsplit('/').next().unwrap_or(path).to_string()
}

/// Size on disk, or 0 for anything we cannot stat. A zero just makes that
/// file weightless in the progress bar; it never blocks the transfer.
fn file_len(p: &std::path::Path) -> u64 {
    std::fs::metadata(p).map(|m| m.len()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wall dedupes the journal against the device listing by stem. If
    /// these two ever drift apart again, every synced track with punctuation
    /// or a long name is listed twice and counted twice.
    #[test]
    fn the_journal_records_the_name_the_watch_will_report() {
        let local_stem =
            "01 - Iva Davies, Christopher Gordon, Richard Tognetti - Ghost of Time - Into the Fog";
        // What `transfer` plans, and what `transcode::normalize` then writes.
        let remote_name = format!(
            "{}.mp3",
            pelican_core::transcode::sanitize_filename_stem(local_stem)
        );
        let reported_by_the_watch = remote_name.trim_end_matches(".mp3");

        assert_eq!(
            journal_stem(&remote_name, "fallback"),
            reported_by_the_watch
        );
        assert_ne!(
            journal_stem(&remote_name, "fallback"),
            local_stem,
            "the local stem is exactly the thing that must not be journalled"
        );
        // Idempotent, so re-sanitizing a planned name cannot shift the key.
        assert_eq!(
            journal_stem(&remote_name, "fallback"),
            journal_stem(&format!("{reported_by_the_watch}.m4a"), "fallback"),
        );
    }

    /// A rename decided at *plan* time keeps the journal honest, because the
    /// shell computes the key from `job.remote_name` after the resolver has
    /// moved it. A rename decided later, inside `transfer::run`, would
    /// journal a name that was never written — a phantom row on the wall, and
    /// the file that did land treated as unknown-provenance. This is the test
    /// that pins renaming to plan time.
    #[test]
    fn a_renamed_job_is_journalled_under_the_name_actually_written() {
        let mut jobs = vec![transfer::Job {
            src: std::path::PathBuf::from("/a/track.mp3"),
            remote_dir: MUSIC_FOLDER.into(),
            remote_name: "track.mp3".into(),
            name_checked: false,
        }];
        let conflicts = transfer::resolve_conflicts(
            &mut jobs,
            &[(MUSIC_FOLDER.to_string(), "track.mp3".to_string())],
            transfer::OnConflict::Rename,
            true,
        );
        assert_eq!(conflicts[0].renamed_to.as_deref(), Some("track-2.mp3"));
        assert_eq!(
            journal_stem(&jobs[0].remote_name, "track.mp3"),
            "track-2",
            "the journal must key on the name the watch will report"
        );
    }

    /// One listing, two pictures, and they are not the same picture. A broken
    /// stub is not something the user can play — so it must not count as
    /// "already there" for reconciliation — but it *is* a handle occupying a
    /// name, so it counts for collisions.
    #[test]
    fn the_two_before_pictures_treat_broken_stubs_differently() {
        let entry = |name: &str, is_folder: bool, is_broken: bool| mtp::RemoteEntry {
            name: name.into(),
            path: format!("Music/{name}"),
            size: 1,
            is_folder,
            is_broken,
        };
        let before = project_before(&[
            entry("Song.mp3", false, false),
            entry("\u{2039}unreadable #42\u{203a}", false, true),
            entry("Subfolder", true, false),
        ]);

        assert_eq!(before.stems, ["song".to_string()].into_iter().collect());
        assert_eq!(before.names.len(), 2, "{:?}", before.names);
        assert!(before
            .names
            .iter()
            .any(|(_, n)| n.contains("unreadable #42")));
        assert!(
            !before.names.iter().any(|(_, n)| n == "Subfolder"),
            "a folder is not a name a file can collide with"
        );
    }

    #[test]
    fn the_first_file_reads_one_of_five_when_it_finishes() {
        let mut t = Tally::default();
        assert_eq!(t.in_flight(), 0, "before anything resolves: 0 of 5");
        assert_eq!(t.done(), 1, "after the first file: 1 of 5, not 0");
        assert_eq!(t.in_flight(), 1, "the second file is in flight at 1 of 5");
    }

    #[test]
    fn skips_and_failures_advance_the_counter_too() {
        // A folder of cover art would stall the counter and look hung.
        let mut t = Tally::default();
        assert_eq!(t.skip(), 1);
        assert_eq!(t.fail(), 2);
        assert_eq!(t.done(), 3);
        assert_eq!((t.ok, t.skipped, t.failed), (1, 1, 1));
    }
}
