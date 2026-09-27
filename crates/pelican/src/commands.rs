//! What each subcommand does. Output goes to stdout one line per fact;
//! progress and warnings go to stderr, so `pelican ls > list.txt` holds
//! only the listing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Result};

use pelican_core::garmin::{self, strip_control, Device, MUSIC_FOLDER};
use pelican_core::ledger::{Kind, Ledger};
use pelican_core::mtp;
use pelican_core::transcode::encoder;
use pelican_core::transcode::tags::{Overrides, Resolved};
use pelican_core::transfer::{
    self, Env, Mix, Options, Outcome, PlanEntry, Progress, Skip, Stop, Verdict,
};
use pelican_core::watch::{self, Origin, Row};
use pelican_core::{backup, paths, platform, reset, source};

use crate::cli::{BackupArgs, Command, DeviceArgs, PushArgs};

pub fn run(cmd: Command) -> Result<ExitCode> {
    match cmd {
        Command::Push(a) => push(a),
        Command::Status(a) => status(a),
        Command::Ls(a) => ls(a),
        Command::Ledger(a) => ledger(a),
        Command::Backup(a) => backup_cmd(a),
        Command::ResetLedger(a) => reset_ledger(a),
    }
}

fn push(a: PushArgs) -> Result<ExitCode> {
    let ov = Overrides {
        artist: a.artist,
        album: a.album,
        genre: a.genre,
        year: a.year,
    };
    let sources = source::expand(&a.paths)?;
    if sources.is_empty() {
        return Err(anyhow!("no audio files found in the paths given"));
    }
    let mix = a.mix.map(|name| Mix { name });
    let entries = transfer::plan_with(sources, &ov, mix.as_ref())?;

    if a.dry_run {
        // A named watch's ledger can say what would be skipped. Reading it
        // is a file read, not a device touch; without `--serial` there is
        // no way to know which ledger without asking USB, so none is read.
        let ledger = match a.serial.as_deref() {
            Some(s) => match Ledger::read(&data_dir()?, s) {
                Ok(l) => Some(l),
                Err(e) => {
                    eprintln!("warning: not marking skips: {e:#}");
                    None
                }
            },
            None => None,
        };
        let ok = dry_run(
            &entries,
            ledger.as_ref(),
            a.resend,
            &mut std::io::stdout().lock(),
        )?;
        return Ok(exit(ok));
    }

    // Everything that can refuse the run without the watch goes first:
    // no ffmpeg, no serial, a ledger another run holds or cannot parse.
    encoder::require()?;
    warn_if_held();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let serial = serial_of(&device)?;
    let mut ledger = Ledger::open(&data_dir()?, &serial)?;
    let cache = paths::cache_dir()
        .ok_or_else(|| anyhow!("no per-user cache dir (neither XDG_CACHE_HOME nor HOME is set)"))?;
    eprintln!("{} — ledger {}", device.label(), ledger.path().display());

    let report = transfer::push(
        entries,
        &mut ledger,
        || mtp::open(&device),
        Options {
            resend: a.resend,
            retries: a.retries,
            ..Options::default()
        },
        Env {
            staging_base: &cache,
            encode: &encoder::encode,
            progress: &mut print_progress,
            stop: Stop::new(),
        },
    )?;
    let t = report.tally();
    println!(
        "{} verified, {} skipped, {} failed",
        t.verified, t.skipped, t.failed
    );
    Ok(exit(t.failed == 0))
}

/// Print the plan. Returns false if any file would be refused.
///
/// The verdicts are [`transfer::preview`]'s — the same call the run makes
/// before it transcodes — so the plan printed is the run that would happen.
fn dry_run(
    entries: &[PlanEntry],
    ledger: Option<&Ledger>,
    resend: bool,
    out: &mut impl Write,
) -> Result<bool> {
    let (mut refused, mut on_watch, mut repeats) = (0, 0, 0);
    let verdicts = transfer::preview(entries, ledger, resend);
    for (e, verdict) in entries.iter().zip(verdicts) {
        writeln!(out, "{}", show(&e.source.path))?;
        if let Ok(t) = &e.tags {
            writeln!(out, "    {}", tag_line(t))?;
        }
        match verdict {
            Verdict::Send { .. } => writeln!(out, "    → (remote name assigned at run time)")?,
            Verdict::Skip(Skip::AlreadyOnWatch { remote }) => {
                on_watch += 1;
                writeln!(out, "    → already on watch as {remote} (skipped)")?;
            }
            Verdict::Skip(Skip::DuplicateOf(first)) => {
                repeats += 1;
                writeln!(out, "    → same audio as {} (skipped)", show(&first))?;
            }
            // A preview is never stopped; said plainly if one ever is.
            Verdict::Skip(skip @ Skip::Stopped {}) => {
                writeln!(out, "    → {} (skipped)", skip.reason())?;
            }
            Verdict::Refused { reason } => {
                refused += 1;
                writeln!(out, "    refused: {}", strip_control(&reason))?;
            }
        }
    }
    writeln!(
        out,
        "dry run: {} planned, {on_watch} already on watch, {repeats} repeated in this run, \
         {refused} refused. Nothing was transcoded and no device was touched.",
        entries.len() - refused - on_watch - repeats
    )?;
    Ok(refused == 0)
}

fn tag_line(t: &Resolved) -> String {
    let mut parts = vec![format!("title {:?}", t.title)];
    for (k, v) in [
        ("artist", &t.artist),
        ("album artist", &t.album_artist),
        ("album", &t.album),
        ("track", &t.track),
        ("date", &t.date),
        ("genre", &t.genre),
    ] {
        if let Some(v) = v {
            parts.push(format!("{k} {v:?}"));
        }
    }
    parts.join(" · ")
}

fn print_progress(p: Progress) {
    match p {
        Progress::Transcoding { n, of, source, .. } => {
            eprintln!("transcoding {n}/{of}  {}", show(&source));
        }
        Progress::Connecting { files, bytes } => {
            eprintln!(
                "sending {files} file(s), {} — opening the watch",
                size(bytes)
            );
        }
        Progress::AttemptFailed {
            remote,
            reason,
            retrying,
            ..
        } => {
            let next = if retrying {
                "; retrying under a new name"
            } else {
                ""
            };
            println!(
                "  attempt failed  {remote}: {}{next}",
                strip_control(&reason)
            );
        }
        // One line per file is the CLI's contract; these are for a UI. The
        // tally is printed from the report.
        Progress::Sending { .. } | Progress::Uploading { .. } | Progress::Finished(_) => {}
        Progress::Done(r) => {
            let src = show(&r.source);
            match &r.outcome {
                Outcome::Verified { remote, .. } => println!("verified  {remote}  ← {src}"),
                Outcome::Skipped(Skip::AlreadyOnWatch { remote }) => {
                    println!("skipped   already on watch as {remote}  ← {src}")
                }
                Outcome::Skipped(Skip::DuplicateOf(first)) => {
                    println!("skipped   same audio as {}  ← {src}", show(first))
                }
                Outcome::Skipped(Skip::Stopped {}) => {
                    println!("skipped   run stopped before it was sent  ← {src}")
                }
                Outcome::Failed { reason, .. } => {
                    println!("failed    {}  ← {src}", strip_control(reason))
                }
            }
        }
    }
}

fn status(a: DeviceArgs) -> Result<ExitCode> {
    warn_if_held();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let snap = watch::read(mtp::open(&device)?.as_mut(), &device)?;
    let c = snap.counts();
    println!("model    {}", snap.model);
    println!("serial   {}", snap.serial.as_deref().unwrap_or("(none)"));
    println!("free     {} of {}", size(snap.free), size(snap.capacity));
    println!(
        "/{MUSIC_FOLDER}   {} of {} objects ({} unreadable)",
        c.audio_objects, c.max_objects, c.stubs
    );
    match device.serial.as_deref() {
        Some(s) => {
            let l = Ledger::read(&data_dir()?, s)?;
            let t = l.totals();
            println!(
                "ledger   {} names used: {} verified, {} failed, {} unresolved{} — {}",
                t.reserved,
                t.verified,
                t.failed,
                t.unresolved,
                since_reset(t.resets),
                l.path().display()
            );
        }
        None => println!("ledger   none: the watch reports no serial"),
    }
    Ok(ExitCode::SUCCESS)
}

fn ls(a: DeviceArgs) -> Result<ExitCode> {
    warn_if_held();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let ledger = match device.serial.as_deref() {
        Some(s) => Some(Ledger::read(&data_dir()?, s)?),
        None => None,
    };
    let snap = watch::read(mtp::open(&device)?.as_mut(), &device)?;
    let mut out = std::io::stdout().lock();
    for row in snap.rows(ledger.as_ref()) {
        writeln!(out, "{}", ls_line(&row))?;
    }
    Ok(ExitCode::SUCCESS)
}

/// `ledger` (Pelican wrote it, per this machine's ledger), `foreign`
/// (something else did), or `stub` (unreadable), then size and name.
fn ls_line(r: &Row) -> String {
    let mark = match r.origin {
        Origin::Ledger => "ledger",
        Origin::Foreign => "foreign",
        Origin::Stub => "stub",
    };
    let size = r.size.map_or("-".to_string(), size);
    let slash = if r.is_folder { "/" } else { "" };
    format!("{mark:<8} {size:>10}  {}{slash}", r.name)
}

fn ledger(a: DeviceArgs) -> Result<ExitCode> {
    let serial = match a.serial {
        Some(s) => s,
        None => serial_of(&garmin::pick_device(None)?)?,
    };
    let l = Ledger::read(&data_dir()?, &serial)?;
    let mut out = std::io::stdout().lock();
    if l.events().is_empty() {
        writeln!(
            out,
            "nothing sent to watch {serial} from this machine yet ({})",
            l.path().display()
        )?;
        return Ok(ExitCode::SUCCESS);
    }
    for e in l.events() {
        let kind = match e.event {
            Kind::Reserve => "reserve",
            Kind::Verified => "verified",
            Kind::Failed => "failed",
            Kind::Reset => "reset",
        };
        let mut line = format!(
            "{}  {kind:<8} {:>6}  {}  {}",
            e.at,
            e.counter,
            e.remote,
            strip_control(&e.title)
        );
        if let Some(r) = &e.reason {
            line.push_str(&format!("  ({})", strip_control(r)));
        }
        writeln!(out, "{line}")?;
    }
    let t = l.totals();
    writeln!(
        out,
        "{} names used: {} verified, {} failed, {} unresolved{} — {}",
        t.reserved,
        t.verified,
        t.failed,
        t.unresolved,
        since_reset(t.resets),
        l.path().display()
    )?;
    Ok(ExitCode::SUCCESS)
}

fn backup_cmd(a: BackupArgs) -> Result<ExitCode> {
    warn_if_held();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let mut dev = mtp::open(&device)?;
    let model = dev.model().unwrap_or_else(|| device.label());
    let dest = match a.dest {
        Some(d) => d,
        None => backup::default_dest(&model)
            .ok_or_else(|| anyhow!("no home directory to put the backup in; name a DEST"))?,
    };
    eprintln!(
        "backing up {model}'s GARMIN folder to {} — read-only on the watch",
        dest.display()
    );
    let s = backup::backup(dev.as_mut(), &dest, &Stop::new(), &mut |p| match p {
        backup::Progress::Listing => eprintln!("listing the watch's files…"),
        backup::Progress::File {
            index, total, path, ..
        } => eprintln!("{:>5}/{total}  {}", index + 1, strip_control(&path)),
        backup::Progress::Finished(_) => {}
    })?;
    for f in &s.failed {
        println!(
            "not copied  {}: {}",
            strip_control(&f.path),
            strip_control(&f.reason)
        );
    }
    if s.unreadable > 0 {
        println!("{} unreadable object(s) had nothing to copy", s.unreadable);
    }
    println!(
        "{} files, {} copied to {}{}",
        s.files,
        size(s.bytes),
        s.dest.display(),
        if s.failed.is_empty() {
            String::new()
        } else {
            format!(" — {} could not be copied", s.failed.len())
        }
    );
    Ok(exit(s.failed.is_empty() && !s.stopped))
}

fn reset_ledger(a: DeviceArgs) -> Result<ExitCode> {
    warn_if_held();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let serial = serial_of(&device)?;
    let mut ledger = Ledger::open(&data_dir()?, &serial)?;
    let mut dev = mtp::open(&device)?;
    let out = reset::reset_ledger(dev.as_mut(), &mut ledger)?;
    println!(
        "checked  /{MUSIC_FOLDER} on {}: {} audio object(s)",
        device.label(),
        out.audio_objects
    );
    println!("{}", out.message);
    if out.reset {
        println!(
            "ledger   reset line appended to {}",
            ledger.path().display()
        );
    }
    Ok(exit(out.clean))
}

/// A warning, not a refusal: gvfs may let go by the time the session
/// opens, and if it does not, the open fails with its own explanation.
fn warn_if_held() {
    if let Some(c) = platform::detect() {
        eprintln!("warning: {}", c.message());
    }
}

fn serial_of(device: &Device) -> Result<String> {
    device.serial.clone().ok_or_else(|| {
        anyhow!(
            "{} reports no USB serial number, so Pelican cannot keep its ledger of the \
             names used on it. Nothing was sent.",
            device.label()
        )
    })
}

fn data_dir() -> Result<PathBuf> {
    paths::data_dir()
        .ok_or_else(|| anyhow!("no per-user data dir (neither XDG_DATA_HOME nor HOME is set)"))
}

/// Totals count only the events since the last factory reset.
fn since_reset(resets: usize) -> String {
    if resets == 0 {
        String::new()
    } else {
        format!(" since the last factory reset ({resets} recorded)")
    }
}

fn exit(ok: bool) -> ExitCode {
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// A local path for the terminal. Filenames are whatever an album's
/// ripper chose, escape sequences included.
fn show(p: &Path) -> String {
    strip_control(&p.display().to_string())
}

fn size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut v = bytes as f64;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    if u == 0 {
        format!("{bytes} B")
    } else {
        format!("{v:.1} {}", UNITS[u])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pelican_core::hash;
    use pelican_core::ledger::Event;

    #[test]
    fn dry_run_prints_the_plan_and_writes_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let album = tmp.path().join("Sea of Thieves");
        std::fs::create_dir(&album).unwrap();
        std::fs::write(album.join("02 - Maiden Voyage.wav"), b"not really audio").unwrap();
        std::fs::write(album.join("\u{2117}.wav"), b"x").unwrap();
        let before = snapshot(tmp.path());

        let sources = source::expand(std::slice::from_ref(&album)).unwrap();
        let entries = transfer::plan(sources, &Overrides::default());
        let mut out = Vec::new();
        let ok = dry_run(&entries, None, false, &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();

        assert!(!ok, "a refused file makes the dry run fail");
        assert!(text.contains(r#"title "Maiden Voyage""#), "{text}");
        assert!(text.contains(r#"artist "Sea of Thieves""#), "{text}");
        assert!(text.contains(r#"track "2""#), "{text}");
        assert!(
            text.contains("(remote name assigned at run time)"),
            "{text}"
        );
        assert!(text.contains("refused: "), "{text}");
        assert!(
            text.contains("1 planned, 0 already on watch, 0 repeated in this run, 1 refused"),
            "{text}"
        );
        assert_eq!(snapshot(tmp.path()), before, "a dry run wrote something");
    }

    #[test]
    fn dry_run_marks_what_the_ledger_has() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("01 - One.wav");
        std::fs::write(&f, b"one").unwrap();
        let data = tmp.path().join("data");
        {
            let mut l = Ledger::open(&data, "42").unwrap();
            let mut e = Event::new(Kind::Verified, 3, "pl00003-One.mp3");
            e.source_sha256 = hash::file(&f).unwrap();
            e.album = Resolved::for_file(&f, None, &Overrides::default())
                .unwrap()
                .album;
            l.append(e).unwrap();
        }
        let l = Ledger::read(&data, "42").unwrap();
        let entries = transfer::plan(source::expand(&[f]).unwrap(), &Overrides::default());
        let mut out = Vec::new();
        assert!(dry_run(&entries, Some(&l), false, &mut out).unwrap());
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("already on watch as pl00003-One.mp3"),
            "{text}"
        );

        // `--resend` sends it again, so the plan must say it will be sent.
        let mut out = Vec::new();
        assert!(dry_run(&entries, Some(&l), true, &mut out).unwrap());
        let text = String::from_utf8(out).unwrap();
        assert!(!text.contains("already on watch as"), "{text}");
        assert!(text.contains("dry run: 1 planned, 0 already"), "{text}");
    }

    /// The run skips a second copy of the same audio; the plan says so
    /// rather than counting it as planned.
    #[test]
    fn dry_run_marks_repeats_within_the_run() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("01 - One.wav");
        let b = tmp.path().join("02 - Again.wav");
        std::fs::write(&a, b"same bytes").unwrap();
        std::fs::write(&b, b"same bytes").unwrap();
        let entries = transfer::plan(source::expand(&[a, b]).unwrap(), &Overrides::default());
        let mut out = Vec::new();
        assert!(dry_run(&entries, None, false, &mut out).unwrap());
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("→ same audio as "), "{text}");
        assert!(
            text.contains("1 planned, 0 already on watch, 1 repeated in this run, 0 refused"),
            "{text}"
        );
    }

    #[test]
    fn ls_lines_read_like_the_listing() {
        let row = |name: &str, size, is_folder, origin| Row {
            name: name.into(),
            size,
            is_folder,
            origin,
            handle: 1,
        };
        assert_eq!(
            ls_line(&row("PL00001-One.mp3", Some(2048), false, Origin::Ledger)),
            "ledger      2.0 KiB  PL00001-One.mp3"
        );
        assert_eq!(
            ls_line(&row("‹unreadable #7›", None, false, Origin::Stub)),
            "stub              -  ‹unreadable #7›"
        );
        assert_eq!(
            ls_line(&row("Podcasts", None, true, Origin::Foreign)),
            "foreign           -  Podcasts/"
        );
    }

    #[test]
    fn sizes_read_like_sizes() {
        assert_eq!(size(512), "512 B");
        assert_eq!(size(2048), "2.0 KiB");
        assert_eq!(size(3 << 30), "3.0 GiB");
    }

    fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
        let mut out = Vec::new();
        for e in std::fs::read_dir(dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                out.extend(snapshot(&p));
            } else {
                out.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
        out.sort();
        out
    }
}
