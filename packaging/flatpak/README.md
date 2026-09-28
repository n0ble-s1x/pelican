# Flatpak packaging

Pelican is **not on Flathub yet**. The manifest here packages the desktop
app (`pelican-app`, the default command) and the command-line tool
(`pelican`) on the GNOME runtime.

## Files

- `io.github.n0ble_s1x.Pelican.yaml`: the manifest (build recipe).
- `io.github.n0ble_s1x.Pelican.metainfo.xml`: AppStream metadata (the store listing).
  Also installed by the AUR packages.
- `cargo-sources.json`: every crate in `Cargo.lock`, for the offline build.
  Generated; regenerate it whenever `Cargo.lock` changes.
- `../desktop/io.github.n0ble_s1x.Pelican.desktop`: the desktop entry.
- `../icons/io.github.n0ble_s1x.Pelican.svg`: the application icon. Not drawn
  yet; the build fails until it exists.

## Decisions

**Runtime.** `org.gnome.Platform` 51, the current GNOME runtime as of
2026-09 (Freedesktop SDK 26.08 base). It ships webkit2gtk-4.1, which Tauri 2
needs. Check `flatpak remote-ls flathub --runtime | grep org.gnome.Platform`
for a newer branch before each submission.

**ffmpeg.** Nothing to bundle. The Freedesktop SDK that the GNOME runtime
is built on ships `/usr/bin/ffmpeg` with the `libmp3lame` encoder, and
Flatpak installs the `org.freedesktop.Platform.codecs-extra` extension
automatically for decoders the base runtime leaves out. The old
`org.freedesktop.Platform.ffmpeg-full` extension stops at 24.08 and does not
exist for 25.08 or later, so it is not used. Checked on the GNOME 51
runtime: `ffmpeg -encoders` lists `libmp3lame`.

**USB.** Pelican opens the watch itself over MTP (nusb on
`/dev/bus/usb`), so it needs raw USB device nodes, not a portal handle.
The manifest asks for `--device=usb` (Flatpak 1.15.11 and later, which
exposes `/dev/bus/usb` only) with `--device-if=all:!has-usb-device` as the
fallback for older Flatpak versions, and `--usb=vnd:091e` so only Garmin
devices are enumerable through the USB portal. This is the combination
Flathub's linter documents for USB apps (`finish-args-no-required-flatpak`,
`finish-args-conditional-permission-usb-no-restriction`), and
`flatpak-builder-lint` accepts it without a `--require-version`.

**Files.** Read-only access to the Music folder and to `/media`,
`/run/media` and `/mnt`; read-write to Documents, where backups go by
default; read-only to `/run/user/<uid>/gvfs` for the "the file manager is
holding the watch" check. `--filesystem=host:ro` and `home:ro` are linter
errors on Flathub without an exception, so a library anywhere else needs:

```sh
flatpak override --user --filesystem=/path/to/music:ro io.github.n0ble_s1x.Pelican
```

If that proves common, request a `home:ro` exception in the submission PR
(see [the linter docs](https://docs.flathub.org/linter)).

**No network.** No `--share=network`. Pelican makes no connections.

**The udev rule.** A Flatpak cannot install a udev rule on the host. The
app already knows this: inside a Flatpak (`FLATPAK_ID` set or
`/.flatpak-info` present) the rule's state shows as unknown, the install
button is not offered, and the window shows a self-contained one-line
command to run in a host terminal instead
(`crates/pelican-shell/src/udev.rs`, `why_not`). The command pipes the rule
text into `sudo install`, so it needs no checkout. The rule file is also
installed at `/app/share/pelican/70-garmin-mtp.rules` for reference. Until
the rule is installed, the watch cannot be opened, so the store listing
says so in its description.

## Local test build

```sh
flatpak install --user flathub org.flatpak.Builder \
  org.gnome.Platform//51 org.gnome.Sdk//51 \
  org.freedesktop.Sdk.Extension.rust-stable//26.08

# From the repository root:
flatpak run org.flatpak.Builder --user --force-clean --repo=repo-flatpak \
  build-flatpak packaging/flatpak/io.github.n0ble_s1x.Pelican.yaml

# Lint exactly as Flathub does:
flatpak run --command=flatpak-builder-lint org.flatpak.Builder \
  manifest packaging/flatpak/io.github.n0ble_s1x.Pelican.yaml
flatpak run --command=flatpak-builder-lint org.flatpak.Builder repo repo-flatpak

# Install and run it:
flatpak run org.flatpak.Builder --user --install --force-clean \
  build-flatpak packaging/flatpak/io.github.n0ble_s1x.Pelican.yaml
flatpak run io.github.n0ble_s1x.Pelican
flatpak run --command=pelican io.github.n0ble_s1x.Pelican status
```

`build-flatpak/`, `.flatpak-builder/` and `repo-flatpak/` are ignored by
git.

Regenerate the cargo sources after any `Cargo.lock` change:

```sh
git clone https://github.com/flatpak/flatpak-builder-tools
python3 -m venv fbt && fbt/bin/pip install aiohttp tomlkit
fbt/bin/python flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  Cargo.lock -o packaging/flatpak/cargo-sources.json
```

## First Flathub submission

Flathub's process is at
https://docs.flathub.org/docs/for-app-authors/submission. Before starting:

1. **App ID.** `io.github.n0ble_s1x.Pelican` is verified through the GitHub
   account `n0ble-s1x` (Flathub turns the hyphen into an underscore): sign
   in to Flathub with that account. No domain is involved.
2. **Icon.** `packaging/icons/io.github.n0ble_s1x.Pelican.svg`.
3. **Release.** Merge the release PR and push the signed `v0.2.0` tag, so
   the screenshots under
   `https://raw.githubusercontent.com/n0ble-s1x/pelican/v0.2.0/docs/images/`
   and the `<release>` date in the metainfo are real. Set that date in the
   release PR.

Then:

1. Fork https://github.com/flathub/flathub and clone the `new-pr` branch:
   `git clone --branch=new-pr git@github.com:<you>/flathub.git`.
2. Create a branch from `new-pr` (for example `add-pelican`).
3. Copy in `io.github.n0ble_s1x.Pelican.yaml` and `cargo-sources.json`
   (regenerated from the tagged `Cargo.lock`).
4. In the copied manifest, delete the `type: dir` source and uncomment the
   `type: git` source, with `tag: v0.2.0` and `commit:` set to the tag's
   full commit SHA (`git rev-parse v0.2.0^{commit}`).
5. Build and lint it locally with the commands above (the linter's
   `appstream-screenshots-not-mirrored-in-ostree` error is expected on a
   local build; Flathub's build mirrors them).
6. Open a PR against `flathub/flathub`, base branch `new-pr`, titled
   `Add io.github.n0ble_s1x.Pelican`. Fill in the checklist in the PR template.
7. Answer review. A reviewer will ask about `--device=usb` and the
   filesystem permissions; the Decisions section above has the reasons.
   Comment `bot, build` on the PR to trigger a test build.
8. Once merged, Flathub creates `flathub/io.github.n0ble_s1x.Pelican` and invites
   you as a maintainer. Accept the invite and enable two-factor
   authentication on GitHub (required). Updates are PRs to that repository
   (see `RELEASING.md`).
