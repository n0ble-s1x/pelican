//! The command line. Pelican is CLI-only until the UI is rebuilt against
//! this core.
//!
//! There is no delete, playlist or tag-on-device command, and there will
//! not be one: the `Backend` trait has no such capability to call.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Transcode music to one proven profile and push it to a Garmin watch over MTP",
    long_about = "Transcode music to one proven profile and push it to a Garmin watch over MTP.\n\n\
                  Every file is sent under a name never used on that watch before, then read \
                  back and hashed to prove it landed intact. Pelican never removes anything \
                  from the watch."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Transcode files and send them to the watch, proving each one.
    Push(PushArgs),
    /// Model, serial, free space, /Music object count and ledger totals.
    Status(DeviceArgs),
    /// List /Music, marking each entry ledger, foreign or stub. Read-only.
    Ls(DeviceArgs),
    /// Print this machine's ledger of every name sent to the watch.
    Ledger(DeviceArgs),
}

#[derive(Args, Debug)]
pub struct DeviceArgs {
    /// Which watch, by USB serial, when more than one is plugged in. For
    /// `ledger`, naming it skips USB entirely.
    #[arg(long, value_name = "S")]
    pub serial: Option<String>,
}

#[derive(Args, Debug)]
pub struct PushArgs {
    /// Print the plan — each source and its resolved tags — and stop.
    /// Transcodes nothing and touches no device.
    #[arg(long)]
    pub dry_run: bool,
    /// Send files the ledger already has as verified, under new names.
    #[arg(long)]
    pub resend: bool,
    /// Further attempts after a failed one, each under a fresh name.
    #[arg(long, value_name = "N", default_value_t = 1)]
    pub retries: u32,
    /// Artist for every file in this run.
    #[arg(long, value_name = "A")]
    pub artist: Option<String>,
    /// Album for every file in this run.
    #[arg(long, value_name = "B")]
    pub album: Option<String>,
    /// Genre for every file in this run.
    #[arg(long, value_name = "G")]
    pub genre: Option<String>,
    /// Year for every file in this run.
    #[arg(long, value_name = "Y")]
    pub year: Option<String>,
    /// Send the files as one album named NAME, in the order given: track
    /// numbers follow that order, the album artist is "Various Artists",
    /// and each song keeps its own title and artist. The watch has no
    /// playlists; this is how a mix plays in order. Year and genre come
    /// only from --year and --genre.
    #[arg(long, value_name = "NAME", conflicts_with_all = ["artist", "album"])]
    pub mix: Option<String>,
    /// Which watch, by USB serial, when more than one is plugged in. With
    /// `--dry-run`, also marks what that watch's ledger already has.
    #[arg(long, value_name = "S")]
    pub serial: Option<String>,
    /// Audio files, or directories to walk (hidden and non-audio files are
    /// skipped).
    #[arg(required = true, value_name = "PATHS")]
    pub paths: Vec<PathBuf>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn push_takes_the_spec_flags() {
        let cli = Cli::try_parse_from([
            "pelican",
            "push",
            "--dry-run",
            "--resend",
            "--retries",
            "3",
            "--artist",
            "A",
            "--album",
            "B",
            "--genre",
            "G",
            "--year",
            "2020",
            "--serial",
            "123",
            "x.flac",
            "dir",
        ])
        .unwrap();
        let Command::Push(p) = cli.command else {
            panic!("not push")
        };
        assert!(p.dry_run && p.resend);
        assert_eq!(p.retries, 3);
        assert_eq!(p.artist.as_deref(), Some("A"));
        assert_eq!(p.album.as_deref(), Some("B"));
        assert_eq!(p.genre.as_deref(), Some("G"));
        assert_eq!(p.year.as_deref(), Some("2020"));
        assert_eq!(p.serial.as_deref(), Some("123"));
        assert_eq!(p.paths, [PathBuf::from("x.flac"), PathBuf::from("dir")]);
    }

    #[test]
    fn retries_default_to_one() {
        let cli = Cli::try_parse_from(["pelican", "push", "x.flac"]).unwrap();
        let Command::Push(p) = cli.command else {
            panic!("not push")
        };
        assert_eq!(p.retries, 1);
        assert!(!p.dry_run && !p.resend);
    }

    #[test]
    fn mix_takes_a_name_and_keeps_the_path_order() {
        let cli = Cli::try_parse_from(["pelican", "push", "--mix", "Long Run", "b.flac", "a.flac"])
            .unwrap();
        let Command::Push(p) = cli.command else {
            panic!("not push")
        };
        assert_eq!(p.mix.as_deref(), Some("Long Run"));
        assert_eq!(p.paths, [PathBuf::from("b.flac"), PathBuf::from("a.flac")]);
        // A mix's album is its name and its artists are the songs' own.
        for clash in ["--album", "--artist"] {
            let r = Cli::try_parse_from(["pelican", "push", "--mix", "M", clash, "X", "a.flac"]);
            assert!(r.is_err(), "{clash}");
        }
    }

    #[test]
    fn push_needs_a_path() {
        assert!(Cli::try_parse_from(["pelican", "push"]).is_err());
    }

    /// The capability does not exist in the type, and not on the command
    /// line either.
    #[test]
    fn there_is_no_delete_command() {
        let names: Vec<String> = Cli::command()
            .get_subcommands()
            .map(|c| c.get_name().to_string())
            .collect();
        assert_eq!(names, ["push", "status", "ls", "ledger"]);
        for bad in ["delete", "rm", "remove", "wipe", "playlist"] {
            assert!(Cli::try_parse_from(["pelican", bad]).is_err(), "{bad}");
        }
    }
}
