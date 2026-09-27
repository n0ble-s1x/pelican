# Flatpak packaging

Pelican is **not on Flathub**. The manifest here packages the command-line
tool only, and it has never been built as committed: the application icon
does not exist yet, and the offline cargo sources have not been generated.
The desktop app (`pelican-app`) is not packaged here; it needs a runtime with
webkit2gtk-4.1 (such as `org.gnome.Platform`) and its desktop entry.

## Files

- `com.krypteia.Pelican.yaml`: the manifest (build recipe).
- `com.krypteia.Pelican.metainfo.xml`: AppStream metadata (the store listing).
- `com.krypteia.Pelican.svg`: the application icon. Not created yet.

## Local test build

Use the current `org.freedesktop.Platform` release (26.08 as of 2026-09;
check `flatpak remote-info flathub org.freedesktop.Platform//26.08` or the
Freedesktop SDK release notes for anything newer) and match the manifest's
`runtime-version`.

```sh
flatpak install --user flathub \
  org.freedesktop.Platform//26.08 \
  org.freedesktop.Sdk//26.08 \
  org.freedesktop.Sdk.Extension.rust-stable//26.08

cd <repo root>
flatpak-builder --user --install \
  build-flatpak/ \
  packaging/flatpak/com.krypteia.Pelican.yaml \
  --force-clean

flatpak run com.krypteia.Pelican status
```

Inside a Flatpak the app cannot install the udev rule; the rule has to be
installed on the host (see the README).

## Submitting to Flathub

Once the manifest builds, has an icon, and a release is tagged:

1. Fork https://github.com/flathub/flathub.
2. Create a branch named `new-pr` (Flathub's required name).
3. Add the manifest as `com.krypteia.Pelican.yaml`, with its source changed
   from `type: dir` to a `type: git` source pinned to the release tag and
   commit.
4. Generate the cargo sources for the offline build:
   `python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json`
5. Open a PR against `flathub/flathub:new-pr`.
6. Flathub's bot builds it and reviewers approve it. Once merged, the app is
   published from its own `flathub/com.krypteia.Pelican` repository, and
   appears in GNOME Software, KDE Discover, the COSMIC Store and other
   Flathub-aware software centers.

## Other packages

The AUR `PKGBUILD` is in `packaging/aur/`. The Debian package is configured
in `[package.metadata.deb]` in `crates/pelican/Cargo.toml` (build it with
`cargo deb --release -p pelican`). Both package the command-line tool only.
