use clap::Parser;

mod app;
mod cli;
mod garmin;
mod gvfs;
mod history;
mod mtp;
mod playlist;
mod theme;
mod transcode;
mod transfer;

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("pelican=info,warn")),
        )
        .init();

    // Sweep transcode cache leftovers from any previous crashed session.
    transcode::sweep(std::time::Duration::from_secs(60 * 60));

    let args = cli::Cli::parse();

    if args.headless || !args.copy.is_empty() {
        cli::run_headless(args)
    } else {
        app::run()
    }
}
