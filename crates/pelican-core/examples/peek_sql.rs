//! Read-only: what is inside /GARMIN/SQL and friends?
use pelican_core::{garmin, mtp};
fn main() {
    let d = garmin::pick_device(None).expect("device");
    let mut b = mtp::open(&d).expect("open");
    for p in [
        "GARMIN/Audio",
        "GARMIN/EXPRESS",
        "GARMIN/Backup",
        "GARMIN/Audio/Music",
        "GARMIN/Audio/Playlists",
    ] {
        match b.list_dir(p) {
            Ok(es) if es.is_empty() => println!("{p}: (empty)"),
            Ok(es) => {
                println!("{p}: {} entries", es.len());
                for e in es.iter().take(14) {
                    println!(
                        "   {} {:>9}  {}",
                        if e.is_folder { "DIR " } else { "FILE" },
                        e.size,
                        e.name
                    );
                }
                if es.len() > 14 {
                    println!("   … {} more", es.len() - 14);
                }
            }
            Err(e) => println!("{p}: error {e:#}"),
        }
        println!();
    }
}
