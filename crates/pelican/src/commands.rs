//! What each subcommand does. Output goes to stdout one line per fact;
//! progress and warnings go to stderr, so `pelican ls > list.txt` holds
//! only the listing.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{anyhow, Context, Result};

use pelican_core::garmin::{self, strip_control, Device, MUSIC_FOLDER};
use pelican_core::ledger::{Kind, Ledger};
use pelican_core::mtp::{self, RemoteEntry};
use pelican_core::transcode::encoder;
use pelican_core::transcode::tags::{Overrides, Resolved};
use pelican_core::transfer::{self, Env, Options, Outcome, PlanEntry, Progress, Skip};
use pelican_core::{hash, paths, platform, source};

use crate::cli::{Command, DeviceArgs, PushArgs};

pub fn run(cmd: Command) -> Result<ExitCode> {
    match cmd {
        Command::Push(a) => push(a),
        Command::Status(a) => status(a),
        Command::Ls(a) => ls(a),
        Command::Ledger(a) => ledger(a),
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
    let entries = transfer::plan(sources, &ov);

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
        let ok = dry_run(&entries, ledger.as_ref(), &mut std::io::stdout().lock())?;
        return Ok(exit(ok));
    }

    // Everything that can refuse the run without the watch goes first:
    // no ffmpeg, no serial, a ledger another run holds or cannot parse.
    encoder::require()?;
    platform::warn_if_holding_garmin();
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
        },
        Env {
            staging_base: &cache,
            encode: &encoder::encode,
            progress: &mut print_progress,
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
fn dry_run(entries: &[PlanEntry], ledger: Option<&Ledger>, out: &mut impl Write) -> Result<bool> {
    let mut refused = 0;
    let mut skipped = 0;
    for e in entries {
        writeln!(out, "{}", show(&e.source.path))?;
        match &e.tags {
            Ok(t) => {
                writeln!(out, "    {}", tag_line(t))?;
                let on_watch = match ledger {
                    Some(l) => hash::file(&e.source.path)
                        .ok()
                        .and_then(|sha| l.verified(&sha).map(|v| v.remote.clone())),
                    None => None,
                };
                match on_watch {
                    Some(remote) => {
                        skipped += 1;
                        writeln!(out, "    → already on watch as {remote} (skipped)")?;
                    }
                    None => writeln!(out, "    → (remote name assigned at run time)")?,
                }
            }
            Err(reason) => {
                refused += 1;
                writeln!(out, "    refused: {}", strip_control(reason))?;
            }
        }
    }
    writeln!(
        out,
        "dry run: {} planned, {skipped} already on watch, {refused} refused. \
         Nothing was transcoded and no device was touched.",
        entries.len() - refused - skipped
    )?;
    Ok(refused == 0)
}

fn tag_line(t: &Resolved) -> String {
    let mut parts = vec![format!("title {:?}", t.title)];
    for (k, v) in [
        ("artist", &t.artist),
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

fn print_progress(p: Progress<'_>) {
    match p {
        Progress::Transcoding { n, of, source } => {
            eprintln!("transcoding {n}/{of}  {}", show(source));
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
                strip_control(reason)
            );
        }
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
                Outcome::Failed { reason, .. } => {
                    println!("failed    {}  ← {src}", strip_control(reason))
                }
            }
        }
    }
}

fn status(a: DeviceArgs) -> Result<ExitCode> {
    platform::warn_if_holding_garmin();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let mut dev = mtp::open(&device)?;
    let model = dev.model().unwrap_or_else(|| device.label());
    let (free, capacity) = dev.free_space().context("reading free space")?;
    let listing = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    drop(dev);

    let stubs = listing.iter().filter(|e| e.is_broken).count();
    let objects = listing.iter().filter(|e| !e.is_folder).count();
    println!("model    {model}");
    println!(
        "serial   {}",
        device
            .serial
            .as_deref()
            .map_or("(none)".into(), strip_control)
    );
    println!("free     {} of {}", size(free), size(capacity));
    println!(
        "/{MUSIC_FOLDER}   {objects} of {} objects ({stubs} unreadable)",
        transfer::MAX_OBJECTS
    );
    match device.serial.as_deref() {
        Some(s) => {
            let l = Ledger::read(&data_dir()?, s)?;
            let t = l.totals();
            println!(
                "ledger   {} names used: {} verified, {} failed, {} unresolved — {}",
                t.reserved,
                t.verified,
                t.failed,
                t.unresolved,
                l.path().display()
            );
        }
        None => println!("ledger   none: the watch reports no serial"),
    }
    Ok(ExitCode::SUCCESS)
}

fn ls(a: DeviceArgs) -> Result<ExitCode> {
    platform::warn_if_holding_garmin();
    let device = garmin::pick_device(a.serial.as_deref())?;
    let ledger = match device.serial.as_deref() {
        Some(s) => Some(Ledger::read(&data_dir()?, s)?),
        None => None,
    };
    let mut dev = mtp::open(&device)?;
    let listing = dev
        .list_dir(MUSIC_FOLDER)
        .with_context(|| format!("listing /{MUSIC_FOLDER}"))?;
    drop(dev);
    let mut out = std::io::stdout().lock();
    for row in ls_rows(&listing, ledger.as_ref()) {
        writeln!(out, "{row}")?;
    }
    Ok(ExitCode::SUCCESS)
}

/// One row per entry: `ledger` (Pelican wrote it, per this machine's
/// ledger), `foreign` (something else did), or `stub` (unreadable).
fn ls_rows(listing: &[RemoteEntry], ledger: Option<&Ledger>) -> Vec<String> {
    listing
        .iter()
        .map(|e| {
            let mark = if e.is_broken {
                "stub"
            } else if ledger.is_some_and(|l| l.has_name(&e.name)) {
                "ledger"
            } else {
                "foreign"
            };
            let size = if e.is_broken || e.is_folder {
                "-".to_string()
            } else {
                size(e.size)
            };
            let slash = if e.is_folder { "/" } else { "" };
            format!("{mark:<8} {size:>10}  {}{slash}", strip_control(&e.name))
        })
        .collect()
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
        "{} names used: {} verified, {} failed, {} unresolved — {}",
        t.reserved,
        t.verified,
        t.failed,
        t.unresolved,
        l.path().display()
    )?;
    Ok(ExitCode::SUCCESS)
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
    use pelican_core::ledger::Event;
    use pelican_core::mtp::fake::FakeDevice;

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
        let ok = dry_run(&entries, None, &mut out).unwrap();
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
            text.contains("1 planned, 0 already on watch, 1 refused"),
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
            l.append(e).unwrap();
        }
        let l = Ledger::read(&data, "42").unwrap();
        let entries = transfer::plan(source::expand(&[f]).unwrap(), &Overrides::default());
        let mut out = Vec::new();
        assert!(dry_run(&entries, Some(&l), &mut out).unwrap());
        let text = String::from_utf8(out).unwrap();
        assert!(
            text.contains("already on watch as pl00003-One.mp3"),
            "{text}"
        );
    }

    #[test]
    fn ls_marks_ledger_foreign_and_stub() {
        let tmp = tempfile::tempdir().unwrap();
        let dev = FakeDevice::new();
        dev.add_file("Music", "PL00001-One.mp3", &[0; 2048]);
        dev.add_file("Music", "Garmin Express.m4a", b"x");
        dev.add_file("Music", "evil\u{1b}[2J.mp3", b"x");
        dev.add_stub("Music");
        let mut l = Ledger::open(tmp.path(), "1").unwrap();
        l.append(Event::new(Kind::Reserve, 1, "pl00001-one.mp3"))
            .unwrap();
        let listing = dev.backend().list_dir("Music").unwrap();
        let rows = ls_rows(&listing, Some(&l));
        let find = |needle: &str| rows.iter().find(|r| r.contains(needle)).unwrap().clone();
        assert!(find("PL00001-One.mp3").starts_with("ledger"));
        assert!(find("2.0 KiB").contains("PL00001"));
        assert!(find("Garmin Express.m4a").starts_with("foreign"));
        assert!(find("‹unreadable").starts_with("stub"));
        assert!(rows.iter().all(|r| !r.contains('\u{1b}')), "{rows:?}");
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
