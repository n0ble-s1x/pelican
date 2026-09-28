//! Experiment 1 from `docs/garmin-library-persistence.md`: did the bytes we
//! uploaded actually arrive?
//!
//! Community reports say MTP writes from Linux and macOS can land on Garmin
//! watches empty or truncated while every call still returns success. That
//! would explain a track the watch lists but will not play, and nothing in
//! this repo has ever checked it.
//!
//! Read-only. It lists `/Music`, downloads each object, and reports the size
//! the listing claimed against the bytes actually received. Give it a local
//! file as a second argument to compare content directly.
//!
//!     cargo run -p pelican-core --example verify_roundtrip
//!     cargo run -p pelican-core --example verify_roundtrip Music/A001.mp3 ~/src/A001.mp3
//!
//! It never deletes anything.

use pelican_core::{garmin, mtp};

fn main() {
    let mut args = std::env::args().skip(1);
    let one = args.next();
    let local = args.next();

    let d = garmin::pick_device(None).expect("no Garmin device found on USB");
    let mut b = mtp::open(&d).expect("opening an MTP session");
    println!("device: {}\n", d.label());

    // A single named object, compared against its source on disk.
    if let (Some(remote), Some(local)) = (one.as_deref(), local.as_deref()) {
        let got = b.download_file(remote).expect("downloading");
        let want = std::fs::read(local).expect("reading the local file");
        println!(
            "{remote}\n  on watch: {} bytes\n  on disk:  {} bytes",
            got.len(),
            want.len()
        );
        if got == want {
            println!("  IDENTICAL: the transport is not the problem for this file");
        } else if got.is_empty() {
            println!("  EMPTY on the watch: the write never delivered data");
        } else if want.starts_with(&got) {
            println!("  TRUNCATED at {} of {} bytes", got.len(), want.len());
        } else {
            let n = got.iter().zip(&want).take_while(|(a, b)| a == b).count();
            println!("  DIFFERS: first {n} bytes match, then diverges");
        }
        return;
    }

    // Otherwise: sweep /Music and flag anything whose payload is short.
    let entries = b.list_dir("Music").expect("listing Music");
    println!("{} entries in /Music\n", entries.len());
    let (mut ok, mut bad) = (0, 0);
    for e in &entries {
        if e.is_folder || e.is_broken {
            continue;
        }
        let path = format!("Music/{}", e.name);
        match b.download_file(&path) {
            Ok(bytes) => {
                let listed = e.size;
                let got = bytes.len() as u64;
                // A player needs a parseable header. Zero bytes, or far fewer
                // than the listing promised, is the failure the reports describe.
                let verdict = if got == 0 {
                    bad += 1;
                    "EMPTY"
                } else if got < listed {
                    bad += 1;
                    "SHORT"
                } else {
                    ok += 1;
                    "ok"
                };
                println!(
                    "  {:<58} listed={listed:>9} got={got:>9}  {verdict}",
                    trim(&e.name, 58)
                );
            }
            Err(err) => {
                bad += 1;
                println!("  {:<58} DOWNLOAD FAILED: {err}", trim(&e.name, 58));
            }
        }
    }
    println!("\n{ok} whole, {bad} suspect");
    if bad > 0 {
        println!("Suspect files mean the write path is at fault, which is ours to fix.");
    }
}

fn trim(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n - 1).chain(std::iter::once('…')).collect()
    }
}
