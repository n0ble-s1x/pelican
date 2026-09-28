# Releasing Pelican

How to cut a release. The source of truth is a signed tag on `main`. Every
distribution channel is a recipe that points at that tag.

`main` is protected: changes land through pull requests with squash merges,
so the version bump goes through a PR like any other change, and only the
tag is pushed directly.

Users install a package; nobody should have to build from source. Each
release ships:

| Artifact | Contents | Built by |
| --- | --- | --- |
| `Pelican_0.X.Y_amd64.deb` | app, CLI, udev rule, desktop entry, icon | `cargo tauri build` |
| `Pelican_0.X.Y_amd64.AppImage` | app only | `cargo tauri build` |
| `pelican-0.X.Y-linux-x86_64.tar.gz` | app, CLI, udev rule, desktop entry, icon, metainfo | `tar` |
| AUR `pelican-bin` | the tarball, installed | `packaging/aur/pelican-bin/PKGBUILD` |
| AUR `pelican` | built from the tag | `packaging/aur/PKGBUILD` |
| Flathub `com.krypteia.Pelican` | app and CLI | `packaging/flatpak/` |

## Prepare the release PR

```sh
git switch -c release/v0.X.Y main

$EDITOR Cargo.toml                                # [workspace.package] version = "0.X.Y"
$EDITOR crates/pelican-shell/tauri.conf.json      # "version": "0.X.Y"
$EDITOR CHANGELOG.md                              # move [Unreleased] into [0.X.Y] - YYYY-MM-DD
$EDITOR packaging/flatpak/com.krypteia.Pelican.metainfo.xml
                                                  # a <release version="0.X.Y" date="YYYY-MM-DD">
                                                  # entry at the top of <releases>, and the
                                                  # screenshot URLs pointing at the v0.X.Y tag
$EDITOR packaging/aur/PKGBUILD packaging/aur/pelican-bin/PKGBUILD
                                                  # pkgver=0.X.Y, pkgrel=1

# The same gate the maintainer runs before every merge:
./scripts/check.sh --full

git add Cargo.toml Cargo.lock crates/pelican-shell/tauri.conf.json CHANGELOG.md \
  packaging/flatpak/com.krypteia.Pelican.metainfo.xml packaging/aur
git commit -m "release: v0.X.Y"
git push -u origin release/v0.X.Y
gh pr create --title "release: v0.X.Y" --body "Version bump and changelog for v0.X.Y."
```

All three crates inherit the version through `version.workspace`. If
`Cargo.lock` changed since the last release, regenerate
`packaging/flatpak/cargo-sources.json` in the same PR (see
`packaging/flatpak/README.md`).

## Tag the merged commit

After the PR is squash-merged:

```sh
git switch main
git pull --ff-only
git log -1                      # confirm this is the "release: v0.X.Y" squash commit
git tag -s v0.X.Y -m "v0.X.Y"   # signed tag
git push origin v0.X.Y
```

## Build the artifacts

Build from a clean checkout of the tag. The binaries link against the
build machine's glibc, and a binary runs only where glibc is at least as
new, so build on the oldest distribution you want the `.deb`, AppImage and
tarball to support (Ubuntu 22.04 has glibc 2.35; a build on current Arch
needs 2.39 and will not start on Ubuntu 22.04 or Debian 12). The AUR and
Flathub builds are unaffected: they build on their own base.

The icon (`packaging/icons/com.krypteia.Pelican.svg`) must exist: the
`.deb`, the AppImage, the tarball, both PKGBUILDs and the Flatpak all
install it, and each build fails without it.

```sh
git switch --detach v0.X.Y

# One-time: the Tauri CLI. Debian and Ubuntu also need the Tauri
# prerequisites: libwebkit2gtk-4.1-dev build-essential curl wget file
# libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev
cargo install tauri-cli --version "^2" --locked

# The .deb and the AppImage. `beforeBuildCommand` in tauri.conf.json builds
# the CLI first, so the .deb can ship it too.
(cd crates/pelican-shell && cargo tauri build)
#   target/release/bundle/deb/Pelican_0.X.Y_amd64.deb
#   target/release/bundle/appimage/Pelican_0.X.Y_amd64.AppImage

# The tarball, from the same binaries. One top-level directory; the
# pelican-bin PKGBUILD expects exactly this layout.
d=pelican-0.X.Y-linux-x86_64
rm -rf dist && mkdir -p dist/$d
install -m 755 target/release/pelican target/release/pelican-app dist/$d/
install -m 644 udev/70-garmin-mtp.rules \
  packaging/desktop/com.krypteia.Pelican.desktop \
  packaging/icons/com.krypteia.Pelican.svg \
  packaging/flatpak/com.krypteia.Pelican.metainfo.xml \
  README.md LICENSE-MIT LICENSE-APACHE dist/$d/
tar -C dist -czf dist/$d.tar.gz $d

cp target/release/bundle/deb/Pelican_0.X.Y_amd64.deb \
   target/release/bundle/appimage/Pelican_0.X.Y_amd64.AppImage dist/
(cd dist && sha256sum *.deb *.AppImage *.tar.gz > SHA256SUMS)
```

Check them before publishing:

```sh
# The .deb holds both programs, the rule and the desktop entry:
bsdtar -xOf dist/Pelican_0.X.Y_amd64.deb data.tar.gz | tar tz    # or: dpkg -c
# The AppImage starts (with no display it stops at "Failed to initialize
# GTK", which proves the bundled libraries load). Point it at sockets that
# do not exist: with WAYLAND_DISPLAY merely unset, GTK falls back to
# wayland-0 and opens a real window, which looks for a watch.
env WAYLAND_DISPLAY=/nonexistent DISPLAY=:987 GDK_BACKEND=wayland \
  XDG_RUNTIME_DIR=/nonexistent dist/Pelican_0.X.Y_amd64.AppImage
```

Notes on what each one does about the USB rule:

- The `.deb` installs `/usr/lib/udev/rules.d/70-garmin-mtp.rules` and its
  `postinst` (`packaging/deb/postinst`) reloads udev, so the watch works
  without a replug.
- The AppImage and the tarball install nothing system-wide. The first time
  the watch cannot be opened, the window offers **Install the USB rule**,
  which runs one fixed command through polkit (`crates/pelican-shell/src/udev.rs`).
  This works from an AppImage, because the AppImage runs on the host.
- The Flatpak cannot install it; the window shows the command to run on the
  host instead.

The Debian package config in `crates/pelican/Cargo.toml`
(`cargo deb -p pelican`) builds a CLI-only `.deb` under the same package
name, `pelican`. The Tauri `.deb` replaces it. Do not attach both.

## Publish the GitHub release

```sh
gh release create v0.X.Y \
  dist/Pelican_0.X.Y_amd64.deb \
  dist/Pelican_0.X.Y_amd64.AppImage \
  dist/pelican-0.X.Y-linux-x86_64.tar.gz \
  dist/SHA256SUMS \
  --title "v0.X.Y" \
  --notes-file <(awk '/^## \[0.X.Y\]/{f=1; print; next} f && /^## /{exit} f' CHANGELOG.md)
```

## Update each store

Do the stores after the GitHub release: `pelican-bin` downloads the
release tarball, and the others build from the tag.

### AUR

Two packages, each its own AUR repository:
`ssh://aur@aur.archlinux.org/pelican.git` (from source) and
`ssh://aur@aur.archlinux.org/pelican-bin.git` (prebuilt). For each:

```sh
# In the AUR clone
cp /path/to/pelican/packaging/aur/PKGBUILD .              # pelican
cp /path/to/pelican/packaging/aur/pelican-bin/PKGBUILD .  # pelican-bin
updpkgsums                  # replaces the placeholder sha256 with the real one
makepkg -f                  # builds and, for pelican, runs the tests
namcap PKGBUILD *.pkg.tar.zst
makepkg --printsrcinfo > .SRCINFO
git add PKGBUILD .SRCINFO
git commit -m "v0.X.Y"
git push
```

`namcap` warns that `ffmpeg` "may not be needed": Pelican runs the
`ffmpeg` program rather than linking it, so the warning is expected.

### Flathub

For the first submission, follow `packaging/flatpak/README.md`. After
acceptance, an update is a PR to `flathub/com.krypteia.Pelican`:

```sh
# In the flathub/com.krypteia.Pelican clone
git switch -c update-0.X.Y
$EDITOR com.krypteia.Pelican.yaml   # the git source's tag: v0.X.Y and
                                    # commit: $(git rev-parse v0.X.Y^{commit})
cp /path/to/pelican/packaging/flatpak/cargo-sources.json .

git commit -am "Update to v0.X.Y"
git push -u origin update-0.X.Y
gh pr create --title "Update to v0.X.Y" --body "https://github.com/n0ble-s1x/pelican/releases/tag/v0.X.Y"
```

Flathub's bot builds the PR. Merge it once the test build passes; the
update is published a few hours later. The store listing (description,
screenshots, release notes) comes from the metainfo file in the tag, which
the release PR already updated.

### crates.io

Pelican is not published to crates.io. It is an application, and the
packages above are the supported install.

## Keep versions in step

Every channel ships the same tag. If you bump the version anywhere, bump it
everywhere:

1. Release on GitHub first (tag and artifacts).
2. Update each store against the same tag.
3. Wait for each to land.

## Reverting

If something ships broken:

- **GitHub release:** `gh release delete v0.X.Y --yes`. The tag stays
  unless you also run `git push origin :refs/tags/v0.X.Y`.
- **AUR:** revert the `PKGBUILD` commit and push, in each of the two
  repositories.
- **Flathub:** revert the manifest commit; the bot republishes the previous
  version.

Prefer shipping a `v0.X.Y+1` fix to yanking.

## Possible later automation

- A CI job that builds the `.deb`, AppImage and tarball in an Ubuntu 22.04
  container and attaches them to the release.
- Prebuilt `aarch64` binaries.
