use clap::{CommandFactory, Parser};

use pelican_core::staging;

mod cli;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pelican=info,warn")),
        )
        .init();

    // Before anything else: a run killed by Ctrl-C left its staging dir
    // behind, and this is the one place that can clear it.
    staging::sweep();

    let _args = cli::Cli::parse();
    cli::Cli::command().print_help()?;
    Ok(())
}
