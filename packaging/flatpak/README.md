# Flatpak / Cosmic Store packaging

Pelican is distributed via [Flathub](https://flathub.org/), which means it
appears automatically in:

- **Cosmic Store** (Pop!_OS 24.04+)
- GNOME Software
- KDE Discover
- Elementary AppCenter
- Any other Flathub-aware software center

## Files

- `com.krypteia.Pelican.yaml` — Flathub manifest (build recipe)
- `com.krypteia.Pelican.metainfo.xml` — AppStream metadata (the listing page)
- `com.krypteia.Pelican.svg` — application icon (TODO: design + add)

## Local test build

```sh
flatpak install --user flathub \
  org.freedesktop.Platform//23.08 \
  org.freedesktop.Sdk//23.08 \
  org.freedesktop.Sdk.Extension.rust-stable//23.08

cd <repo root>
flatpak-builder --user --install \
  build-flatpak/ \
  packaging/flatpak/com.krypteia.Pelican.yaml \
  --force-clean

flatpak run com.krypteia.Pelican
```

## Submitting to Flathub

When ready (post v0.1.0 release):

1. Fork https://github.com/flathub/flathub
2. Create branch `new-pr` (Flathub's required name)
3. Add this manifest as `com.krypteia.Pelican.yaml`
4. Generate cargo sources for offline build:
   `python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py Cargo.lock -o cargo-sources.json`
5. Open PR against `flathub/flathub:new-pr`
6. Flathub bot validates; reviewers approve; auto-merges to its own repo
7. Once merged, `com.krypteia.Pelican` is live on Flathub and downstream stores

## Why Flatpak (and not native packages first)

- One manifest covers every Linux software store
- Sandboxing limits what a compromised dep can do (USB-only, no network)
- Updates happen via Flathub regardless of distro release cycle
- Cosmic Store specifically prefers Flathub apps
- Native `.deb`/AUR are also offered for users who avoid Flatpak — see `packaging/debian/` and `packaging/aur/`
