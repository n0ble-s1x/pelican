//! Two hardware questions in one run, using a real untagged WAV.
//!
//!  1. Does an untagged WAV appear in the watch's music app? (left on device)
//!  2. Does the collision guard refuse a second upload of the same name?
use pelican_core::{garmin, mtp, transfer};

fn main() {
    let src = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: <file>"));
    let device = garmin::pick_device(None).expect("device");
    let opts = transfer::Options {
        skip_tag_check: true,
        transcode: true,
    };

    for pass in 1..=2 {
        println!("\n── pass {pass} ──");
        let jobs = match transfer::expand_inputs_into(
            std::slice::from_ref(&src),
            garmin::MUSIC_FOLDER,
            opts.transcode,
        ) {
            Ok(j) => j,
            Err(e) => {
                println!("  planning refused: {e:#}");
                continue;
            }
        };
        for j in &jobs {
            println!("  plan -> {}", j.remote_name);
        }
        let (tx, rx) = transfer::channel();
        std::thread::scope(|s| {
            s.spawn(|| {
                transfer::run(&device, jobs, &opts, &tx);
                drop(tx);
            });
            for e in rx {
                match e {
                    transfer::Event::Done { at, bytes } => {
                        println!("  UPLOADED {} ({bytes} b)", at.label())
                    }
                    transfer::Event::Failed { at, error } => {
                        println!("  REFUSED  {} :: {error}", at.label())
                    }
                    transfer::Event::Skipped { at, reason } => {
                        println!("  SKIPPED  {} :: {reason}", at.label())
                    }
                    transfer::Event::Finished(r) => println!(
                        "  result: {} ok, {} skipped, {} failed",
                        r.ok, r.skipped, r.failed
                    ),
                    _ => {}
                }
            }
        });
    }

    println!("\n── what is on the watch now ──");
    match mtp::open(&device) {
        Ok(mut b) => match b.list_dir(garmin::MUSIC_FOLDER) {
            Ok(es) => {
                for e in es {
                    println!("  {:>10}  {}", e.size, e.name);
                }
            }
            Err(e) => println!("  listing failed: {e:#}"),
        },
        Err(e) => println!("  open failed: {e:#}"),
    }
}
