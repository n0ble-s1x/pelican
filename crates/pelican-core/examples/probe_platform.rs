//! Phase-0 instrument: answer the macOS port's open questions in one run.
//!
//!   cargo run -p pelican-core --example probe_platform
//!   cargo run -p pelican-core --example probe_platform -- --write
//!
//! Read-only by default. `--write` additionally uploads a tiny generated
//! tone, verifies its size, and deletes it — a full round-trip through the
//! exact path a real transfer takes.
//!
//! The questions this exists to settle, in order of how much they matter:
//!
//!   1. Does something on this machine hold the watch? On macOS that is
//!      `ptpcamerad`, which is SIP-protected and cannot be signalled — so if
//!      it claims Garmin devices, every macOS user needs a `sudo` command
//!      before Pelican works at all. UNVERIFIED against real hardware.
//!   2. Can we open an MTP session and list `/Music`?
//!   3. What can this machine convert, and to what?
//!   4. Does an upload actually land intact? (`--write`)

use std::path::PathBuf;

use pelican_core::{garmin, mtp, platform, transcode, transfer};

fn rule(title: &str) {
    println!(
        "\n── {title} {}",
        "─".repeat(56usize.saturating_sub(title.len()))
    );
}

fn main() {
    let write = std::env::args().any(|a| a == "--write");

    rule("1 · who holds the device");
    match platform::detect() {
        Some(c) => {
            println!("  HELD BY : {}", c.holder);
            println!("  WHERE   : {}", c.detail);
            println!("  FIX     : {}", c.remedy.replace('\n', "\n            "));
            println!(
                "  SELF-FIX: {}",
                if c.self_fixable {
                    "yes — Pelican could run this itself"
                } else {
                    "NO — the user must run it; we cannot signal the holder"
                }
            );
        }
        None => println!("  nothing detected holding the device"),
    }

    rule("2 · devices");
    let devices = match garmin::list_devices() {
        Ok(d) => d,
        Err(e) => {
            println!("  USB enumeration failed: {e:#}");
            return;
        }
    };
    if devices.is_empty() {
        println!("  no Garmin device on USB (vendor 0x091E).");
        println!("  Plug the watch in, unlock it, and set USB mode to MTP.");
    }
    for d in &devices {
        println!(
            "  {}  [{:04x}:{:04x}]",
            d.label(),
            d.vendor_id,
            d.product_id
        );
    }

    rule("3 · what this machine can convert");
    let encoders = transcode::encoder::available();
    if encoders.is_empty() {
        println!("  no encoder installed.");
        println!("  MP3/M4A/WAV still sync — copied verbatim, tags rebuilt in process.");
        println!("  FLAC/OGG/Opus and friends will be skipped. Install ffmpeg for those.");
    }
    for e in &encoders {
        println!("  {:<10} → .{}", e.name(), e.output_ext());
    }
    for ext in transcode::AUDIO_EXTS {
        let p = PathBuf::from(format!("probe.{ext}"));
        let verdict = match transcode::encoder::plan(&p) {
            Ok(transcode::encoder::Pipeline::Passthrough) => "copy + retag (no tool)".to_string(),
            Ok(transcode::encoder::Pipeline::Encode(e)) => {
                format!("{} → .{}", e.name(), e.output_ext())
            }
            Err(_) => "UNSUPPORTED here".to_string(),
        };
        println!("    .{ext:<5} {verdict}");
    }

    let Some(device) = devices.first() else {
        println!("\nStopped: no device. Re-run with the watch attached for steps 4-5.");
        return;
    };

    rule("4 · MTP session");
    let mut backend = match mtp::open(device) {
        Ok(b) => {
            println!("  session opened");
            // The real model name lives in GetDeviceInfo, not in the USB
            // descriptors — this watch declares no product string at all —
            // so the probe and the app should agree on where it comes from.
            match b.model() {
                Some(m) => println!("  model   : {m} (from MTP GetDeviceInfo)"),
                None => println!("  model   : the device reports none"),
            }
            b
        }
        Err(e) => {
            println!("  FAILED to open: {e:#}");
            println!("\n  This is the answer to question 1. If the error mentions");
            println!("  exclusive access, the holder above is real and blocking.");
            return;
        }
    };
    match backend.free_space() {
        Ok((free, total)) => println!(
            "  storage: {:.1} MB free of {:.1} MB",
            free as f64 / 1e6,
            total as f64 / 1e6
        ),
        Err(e) => println!("  free_space failed: {e:#}"),
    }
    match backend.list_dir(garmin::MUSIC_FOLDER) {
        Ok(entries) => {
            let broken = entries.iter().filter(|e| e.is_broken).count();
            println!(
                "  /Music: {} entries ({broken} unreadable stubs)",
                entries.len()
            );
            for e in entries.iter().take(10) {
                println!("    {:>10}  {}", e.size, e.name);
            }
            if entries.len() > 10 {
                println!("    … {} more", entries.len() - 10);
            }
        }
        Err(e) => println!("  listing /Music failed: {e:#}"),
    }

    // Release before any transfer. The watch allows one session at a time,
    // so holding this one open would make the upload below contend with us
    // and fail as "exclusive access" — which looks exactly like an outside
    // process holding the device. The GUI drops its backend for the same
    // reason before it starts a send.
    drop(backend);

    if !write {
        println!("\nRead-only run. Add --write to test a real upload round-trip.");
        return;
    }

    rule("5 · upload round-trip");
    let dir = std::env::temp_dir().join("pelican-probe");
    let _ = std::fs::create_dir_all(&dir);

    // Two sources, because they exercise different halves of the pipeline
    // and only one of them has ever been confirmed against firmware:
    //   WAV  — already playable, so it is copied and re-tagged in process.
    //   FLAC — has to go through an encoder. On a Mac with no ffmpeg that
    //          is afconvert, which produces M4A/AAC rather than the MP3
    //          profile verified on the FR165. Whether the watch's indexer
    //          is as happy with that is the open question this settles.
    let wav = dir.join("Pelican Probe Tone.wav");
    write_tone(&wav);
    let mut sources = vec![wav.clone()];
    if let Some(flac) = make_flac(&dir, &wav) {
        sources.push(flac);
    } else {
        println!("  (no FLAC source built — nothing here can encode one)");
    }

    for src in &sources {
        println!();
        println!(
            "  ── source: {}",
            src.file_name().unwrap().to_string_lossy()
        );
        match transcode::encoder::plan(src) {
            Ok(transcode::encoder::Pipeline::Passthrough) => {
                println!("     pipeline: copy + retag (no external tool)")
            }
            Ok(transcode::encoder::Pipeline::Encode(e)) => {
                println!("     pipeline: {} → .{}", e.name(), e.output_ext())
            }
            Err(e) => {
                println!("     UNSUPPORTED here: {e}");
                continue;
            }
        }
        round_trip(device, src);
    }

    for src in &sources {
        let _ = std::fs::remove_file(src);
    }
}

/// Upload one file, confirm it landed at the right size, then delete it.
/// Leaves the device exactly as it was found.
fn round_trip(device: &garmin::Device, src: &std::path::Path) {
    let opts = transfer::Options {
        skip_tag_check: true,
        transcode: true,
    };
    let jobs = match transfer::expand_inputs_into(
        std::slice::from_ref(&src.to_path_buf()),
        garmin::MUSIC_FOLDER,
        opts.transcode,
    ) {
        Ok(j) => j,
        Err(e) => {
            println!("     planning failed: {e:#}");
            return;
        }
    };
    // The planner names the job from the source extension; the encoder may
    // change the container. Ask the pipeline what it will actually write.
    let remote_name = match transcode::normalize(src, None) {
        Ok(t) => t.remote_name.clone(),
        Err(e) => {
            println!("     normalize failed: {e:#}");
            return;
        }
    };

    let (tx, rx) = transfer::channel();
    std::thread::scope(|s| {
        s.spawn(|| {
            transfer::run(device, jobs, &opts, &tx);
            drop(tx);
        });
        for evt in rx {
            match evt {
                transfer::Event::Done { at, bytes } => {
                    println!("     UPLOADED {} ({bytes} bytes)", at.label())
                }
                transfer::Event::Failed { at, error } => {
                    println!("     FAILED {} · {error}", at.label())
                }
                transfer::Event::Skipped { at, reason } => {
                    println!("     SKIPPED {} · {reason}", at.label())
                }
                transfer::Event::Finished(r) => println!(
                    "     result: {} ok, {} skipped, {} failed",
                    r.ok, r.skipped, r.failed
                ),
                _ => {}
            }
        }
    });

    // The transfer closed its session; a fresh one confirms the object is
    // really there rather than trusting our own write path.
    let remote_path = format!("{}/{remote_name}", garmin::MUSIC_FOLDER);
    match mtp::open(device) {
        Ok(mut b) => {
            match b.remote_size(garmin::MUSIC_FOLDER, &remote_name) {
                Ok(Some(n)) => println!("     verified on watch: {remote_name} — {n} bytes"),
                Ok(None) => println!("     NOT FOUND on watch after upload: {remote_name}"),
                Err(e) => println!("     size check failed: {e:#}"),
            }
            match b.delete(&remote_path) {
                Ok(()) => println!("     cleaned up {remote_path}"),
                Err(e) => println!("     cleanup FAILED for {remote_path}: {e:#}"),
            }
        }
        Err(e) => println!("     reopen for verify failed: {e:#}"),
    }
}

/// Build a FLAC beside the tone, using whatever encoder exists.
fn make_flac(dir: &std::path::Path, wav: &std::path::Path) -> Option<PathBuf> {
    let out = dir.join("Pelican Probe Tone.flac");
    let ok = if cfg!(target_os = "macos") {
        std::process::Command::new("afconvert")
            .arg(wav)
            .args(["-f", "flac", "-d", "flac"])
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    } else {
        std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error", "-i"])
            .arg(wav)
            .arg(&out)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    };
    ok.then_some(out)
}

/// One second of 440 Hz stereo, 16-bit PCM. Small, silent-ish, harmless.
fn write_tone(path: &std::path::Path) {
    const RATE: u32 = 44_100;
    let frames = RATE;
    let data_len = frames * 4;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 4).to_le_bytes());
    b.extend_from_slice(&4u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for i in 0..frames {
        let t = i as f64 / RATE as f64;
        let v = (12_000.0 * (2.0 * std::f64::consts::PI * 440.0 * t).sin()) as i16;
        b.extend_from_slice(&v.to_le_bytes());
        b.extend_from_slice(&v.to_le_bytes());
    }
    let _ = std::fs::write(path, b);
}
