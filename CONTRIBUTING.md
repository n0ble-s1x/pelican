# Contributing

Thanks for considering a contribution. The project is small and the surface
area is well-defined; getting a PR through review is usually quick.

## Quickstart

```sh
git clone https://github.com/n0ble-s1x/pelican
cd pelican
cargo build --release

# Run the full local QA gate (same checks the maintainer runs before merge):
./scripts/check.sh --full
```

Optionally, install the git hooks so `check.sh` runs on every commit/push:

```sh
./scripts/install-hooks.sh
```

## What we want

- **New device support.** If you have a Garmin music watch we haven't tested,
  run the tool against it and report what works / what doesn't. Add the model
  to the compatibility matrix in `README.md`. If it needs new firmware
  workarounds, code them up — we'll review and merge.
- **Reverse-engineering Garmin's vendor MTP ops** (`0x9000–0x900B`,
  `0x9810`, `0x9811`). The watch advertises support but Garmin Express
  uses a vendor-specific path for playlist sync we haven't cracked.
  USB packet captures of Garmin Express writing a playlist would be
  invaluable.
- **Packaging** for distros that don't have us yet — Flatpak manifest,
  Debian packaging, NixOS module, etc.
- **macOS port.** Mostly a build-system / udev-equivalent problem.
- **Bug fixes & tests.** Always welcome. Real-hardware test reports doubly so.

## What we don't want

- **Windows support.** Use Garmin Express. We are not going to maintain a
  Windows port. (Or — sincerely — try Linux. It's good now.)
- **Telemetry, "phone home" features, analytics.** This binary is local-only
  and stays that way.
- **Cloud-tied features** (Spotify auth, etc.). Garmin Connect IQ apps already
  exist for those; we are explicitly building the local-files alternative.
- **Network-capable dependencies** (HTTP clients, TLS stacks) unless there's
  a feature requirement we've explicitly agreed to. The dep tree is audited
  to remain network-free; PRs that pull in `reqwest`/`ureq`/`hyper`/etc. will
  be rejected unless the feature has been discussed first.

## Pull request process

We do **not** run third-party CI. **You run the checks; the maintainer
re-runs them locally before merge.** This is intentional — see
[SECURITY.md](SECURITY.md) for the rationale.

1. Fork & branch from `main` (`feature/xyz`, `fix/abc`)
2. Keep PRs focused — one feature / one fix per PR
3. Run `./scripts/check.sh --full` on your branch — it must pass clean
   (rustfmt, clippy with `-D warnings`, build, tests, `cargo audit`,
   `cargo deny`)
4. Open a PR. In the description, confirm `scripts/check.sh --full` passed.
   If you tested on real hardware, list the model + firmware
5. The maintainer pulls your branch, re-runs `./scripts/check.sh --full`,
   reads the diff (especially: any new `Cargo.toml` deps, any new `unsafe`
   block, any new `Command::new` / network code), and merges if happy

For Dependabot PRs: a 48h cooldown is enforced before the PR even opens.
Maintainer reviews + merges manually. **Nothing is ever auto-merged.**

If you're sending a bigger change, open a discussion issue first so we don't
waste your time on something we'd reject.

## Security issues

Don't file public issues for security problems. See [SECURITY.md](SECURITY.md).

## Code of conduct

Be decent. We're all here to make a small thing better. Disagreements
about technical direction are fine and healthy; personal attacks aren't.

## License

By contributing, you agree your contributions will be dual-licensed under
MIT and Apache 2.0 (the project's existing license). If you contribute
work you don't own (e.g., copy-pasted from elsewhere), say so explicitly
and confirm the source license is compatible.
