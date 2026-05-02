# Contributing

Thanks for considering a contribution. The project is small and the surface
area is well-defined; getting a PR through review is usually quick.

## Quickstart

```sh
git clone https://github.com/n0ble-s1x/pelican
cd pelican
cargo build --release
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

CI runs all of the above plus `cargo audit` and `cargo deny`. Anything that
breaks CI won't merge.

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

## Pull request process

1. Fork & branch from `main` (`feature/xyz`, `fix/abc`)
2. Keep PRs focused — one feature / one fix per PR
3. Include a brief test plan in the PR description; if you tested on real
   hardware, say which model + firmware
4. We review for **security, correctness, and maintainability** (in that order).
   We're not pedantic about style — just run `cargo fmt`
5. Merge requires one approving review from a maintainer + green CI

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
