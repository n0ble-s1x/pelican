//! The command line. Pelican is CLI-only until the UI is rebuilt against
//! this core; the subcommands (`push`, `status`, `ls`, `ledger`) land with
//! the transfer loop.
//!
//! There is no delete, playlist or tag-on-device command, and there will
//! not be one: the `Backend` trait has no such capability to call.

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    version,
    about = "Transcode music to one proven profile and push it to a Garmin watch over MTP",
    long_about = None
)]
pub struct Cli {}
