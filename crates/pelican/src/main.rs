use std::process::ExitCode;

use clap::Parser;

use pelican_core::staging;

mod cli;
mod commands;

fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pelican=info,warn")),
        )
        .init();

    // Before anything else: a run killed by Ctrl-C left its staging dir
    // behind, and this is the one place that can clear it.
    staging::sweep();

    let cli = cli::Cli::parse();
    match commands::run(cli.command) {
        Ok(code) => code,
        Err(e) => {
            // A watch that stopped answering gets the one instruction that
            // helps, on a line of its own, before the detail.
            if pelican_core::error::is_wedged(&e) {
                eprintln!("{}", pelican_core::error::REPLUG);
            }
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}
