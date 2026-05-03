use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(version, about = "Sync music to Garmin watches over MTP", long_about = None)]
pub struct Cli {
    /// Run without GUI. Implied when --copy is given.
    #[arg(long)]
    pub headless: bool,

    /// Files or folders to copy to the watch's Music/ folder.
    #[arg(long, value_name = "PATH")]
    pub copy: Vec<PathBuf>,

    /// Require valid ID3 title + artist tags before upload. Without this,
    /// untagged files are uploaded but Garmin's music app won't show them
    /// (the file is still on the watch and can be deleted via this tool).
    #[arg(long)]
    pub require_tags: bool,

    /// Disable auto-transcoding of FLAC/OGG/Opus/WMA/AIFF to MP3.
    /// By default, ffmpeg is used to convert these formats since the
    /// watch can't play them natively.
    #[arg(long)]
    pub no_transcode: bool,

    /// Pick a specific watch by USB serial. If omitted and multiple Garmin
    /// devices are attached, garmin-music will list them and exit.
    #[arg(long, value_name = "SERIAL")]
    pub serial: Option<String>,

    /// Delete a file from the watch by remote path (e.g. `Music/foo.mp3`).
    /// Repeatable. Listed entries that look like `‹unreadable #N›` can also
    /// be deleted by passing that exact synthetic name.
    #[arg(long, value_name = "REMOTE_PATH")]
    pub delete: Vec<String>,

    /// List existing .m3u/.m3u8 playlists in /Music with their track contents.
    #[arg(long)]
    pub list_playlists: bool,

    /// Create a playlist named NAME (without extension). Combine with
    /// repeated `--track <FILENAME>` to populate it. Each track is a
    /// filename in /Music (e.g. `01-track.mp3`). Overwrites if already exists.
    #[arg(long, value_name = "NAME")]
    pub create_playlist: Option<String>,

    /// Track filename (relative to /Music) for `--create-playlist`. Repeatable.
    #[arg(long, value_name = "FILENAME")]
    pub track: Vec<String>,
}

pub fn run_headless(args: Cli) -> anyhow::Result<()> {
    use crate::{garmin, gvfs, mtp, transfer};

    gvfs::warn_if_holding_garmin()?;

    let device = garmin::pick_device(args.serial.as_deref())?;

    // Process deletes first so they can free space before any uploads.
    let mut delete_failed = 0usize;
    if !args.delete.is_empty() {
        let mut backend = mtp::open(&device)?;
        for path in &args.delete {
            match backend.delete(path) {
                Ok(()) => println!("✓ deleted {path}"),
                Err(e) => {
                    eprintln!("✕ delete {path}: {e:#}");
                    delete_failed += 1;
                }
            }
        }
    }

    if args.list_playlists {
        let mut backend = mtp::open(&device)?;
        let entries = backend.list_dir("Music")?;
        let playlists: Vec<_> = entries
            .iter()
            .filter(|e| !e.is_folder && !e.is_broken && crate::playlist::is_playlist(&e.name))
            .collect();
        if playlists.is_empty() {
            println!("(no playlists in /Music)");
        }
        for p in playlists {
            println!("▸ {} ({} bytes)", p.name, p.size);
            match backend.download_file(&p.path) {
                Ok(bytes) => {
                    for t in crate::playlist::parse(&bytes) {
                        println!("    {t}");
                    }
                }
                Err(e) => eprintln!("    (read error: {e:#})"),
            }
        }
    }

    if let Some(name) = &args.create_playlist {
        let mut backend = mtp::open(&device)?;
        let safe_name = name.trim().replace(
            |c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '_' && c != ' ',
            "-",
        );
        let filename = format!("{safe_name}.{}", crate::playlist::EXT);
        let bytes = crate::playlist::serialize_for_device(
            &args.track,
            crate::playlist::PathStyle::BareCasePreserved,
        );
        backend.write_raw("Music", &filename, &bytes)?;
        println!(
            "✓ wrote /Music/{filename} ({} tracks, {} bytes)",
            args.track.len(),
            bytes.len()
        );
    }

    // Plan and run uploads with a fresh MTP session per file. Empirically,
    // Garmin firmware on the FR165 absorbs only the first 1-2 files of a
    // multi-file session — everything after lands as a broken metadata stub.
    // Closing and reopening the session between files restores reliability.
    let opts = transfer::Options {
        skip_tag_check: !args.require_tags,
        transcode: !args.no_transcode,
    };
    let report = transfer::run_jobs_per_file(&device, &args.copy, &opts)?;

    println!(
        "{} ok, {} skipped, {} failed, {} delete-failed",
        report.ok, report.skipped, report.failed, delete_failed
    );
    if report.failed > 0 || delete_failed > 0 {
        std::process::exit(1);
    }
    Ok(())
}
