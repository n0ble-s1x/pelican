# Contributing

Thanks for considering a contribution. The project is small and its scope is
narrow, so a focused PR is usually reviewed quickly.

## Quickstart

The workspace has three crates and a frontend:

- `crates/pelican-core`: the library (sources and tags, ffmpeg, the ledger,
  the MTP transfer, the backup).
- `crates/pelican`: the command-line tool, binary `pelican`.
- `crates/pelican-shell`: the desktop app, binary `pelican-app`, a Tauri 2
  window over the core.
- `ui/`: the app's interface, plain HTML, CSS and JavaScript. There is no npm
  and no build step; the files are compiled into `pelican-app` as they are.

You need Rust 1.89 or newer, ffmpeg (every file is transcoded), and
webkit2gtk-4.1 for the app. On Arch:
`pacman -S --needed rustup ffmpeg webkit2gtk-4.1 base-devel`.

```sh
git clone https://github.com/n0ble-s1x/pelican
cd pelican
cargo build --release -p pelican -p pelican-shell

# The full local gate, the same one the maintainer runs before merging:
./scripts/check.sh --full
```

Run the app from source with `cargo run -p pelican-shell`, not
`cargo tauri dev` (a dev server is served without the CSP).

To run `check.sh` on every commit (quick mode) and push (full mode):

```sh
./scripts/install-hooks.sh
```

## The gates

`./scripts/check.sh --full` must pass. It runs:

- `cargo fmt --all -- --check`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo build --release --all-features`
- `cargo test --all-features`
- `cargo audit --deny warnings`
- `cargo deny check`

It also checks invariants a compiler cannot: no HTTP or TLS crate is compiled
in, no device delete call exists, the app's registered, ACL-gated and granted
commands are the same three lists, the capability grants nothing beyond those
commands and event listening, the CSP allows no inline script and no remote
origin, and `deny.toml` and `.cargo/audit.toml` ignore the same advisories.

If your change adds an `invoke()` in `ui/`, add the command to `build.rs`,
`generate_handler!` in `main.rs` and `capabilities/main.json` together.

## What we want

- **New device support.** If you have a Garmin music watch other than the
  Forerunner 165 Music, run the test in the README ("Verifying a new watch")
  and report what works and what does not. Every device claim in this repo
  was measured on one Forerunner 165 Music on firmware 2506.
- **Garmin Express captures.** USB captures of Garmin Express writing a
  playlist to a watch would show whether it uses the vendor MTP operations
  (`0x9000-0x900B`, `0x9810`, `0x9811`). See `docs/vendor-ops.md`. Do not
  probe those operations blind: it wedges the watch until it is replugged.
- **Packaging.** Finish and test the Flatpak manifest in `packaging/flatpak/`
  (it has never been built), and package the app for AUR and Debian. Other
  distributions and a NixOS module are welcome too.
- **A macOS test of the current core.** See "What about macOS?" in the
  README for what was tried before and what failed.
- **Bug fixes and tests.** Always welcome. Reports from real hardware
  especially.

## What we don't want

- **Windows support.** Use Garmin Express. We will not maintain a Windows
  port.
- **Telemetry, "phone home" features or analytics.** Pelican is local-only
  and stays that way.
- **Cloud-tied features** (Spotify sign-in and the like). Garmin Connect IQ
  apps already cover those; Pelican is the local-files alternative.
- **Network-capable dependencies** (HTTP clients, TLS stacks) unless a
  feature needs one and it has been agreed first. PRs that pull in `reqwest`,
  `ureq`, `hyper` or similar will be declined otherwise.
- **A delete command.** Deleting a file does not remove the track from the
  watch's library, and a delete followed by a write is how names get reused.
  See the README.

## Pull request process

There is no hosted CI. **You run the checks, and the maintainer re-runs them
locally before merging.** [SECURITY.md](SECURITY.md) explains why.

1. Fork, and branch from `main` (`feature/xyz`, `fix/abc`).
2. Keep each PR to one feature or one fix.
3. Run `./scripts/check.sh --full` on your branch. It must pass.
4. Open a PR and confirm in the description that the full check passed. If
   you tested on a real watch, give the model and firmware.
5. The maintainer pulls your branch, re-runs `./scripts/check.sh --full`,
   reads the diff (new dependencies, new `unsafe`, new `Command::new`, new
   commands exposed to the webview), and merges if it is sound.

Dependabot PRs open only for releases at least two days old, and are
reviewed and merged by hand. Nothing is auto-merged.

For a larger change, open an issue first so neither of us spends time on
something that would be declined.

## Security issues

Report vulnerabilities privately through the repository's **Security** tab
(**Report a vulnerability**); see [SECURITY.md](SECURITY.md). File other bugs
as public issues.

## Code of conduct

Be decent. Disagreeing about technical direction is fine; personal attacks
are not.

## License

By contributing, you agree that your contributions are dual-licensed under
MIT and Apache 2.0, the project's license. If you contribute work you did
not write, say so and confirm that its license is compatible.
