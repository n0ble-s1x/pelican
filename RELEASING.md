# Releasing Pelican

How to cut a release. The source of truth is a signed tag on `main`. Every
distribution channel is a recipe that points at that tag.

`main` is protected: changes land through pull requests with squash merges,
so the version bump goes through a PR like any other change, and only the
tag is pushed directly.

Users install a package; nobody should have to build from source. There is
no CI: every channel either builds on its own infrastructure or on the
user's machine, so a release is a tag and a few pull requests, and nothing
is built or uploaded by hand.

| Channel | Contents | Built by |
| --- | --- | --- |
| Flathub `io.github.n0ble_s1x.Pelican` | app and CLI | Flathub, from `packaging/flatpak/` |
| AUR `pelican` | app, CLI, udev rule | the user's machine, from the tag (`packaging/aur/PKGBUILD`) |
| GitHub release | the signed tag, source and release notes | `gh release create` |

Why not a `.deb`, AppImage or prebuilt tarball: a binary runs only where
glibc is at least as new as the build machine's, so those have to be built
on an old base (Ubuntu 22.04), which means a container or CI job to keep up.
The AppImage target is still configured in `tauri.conf.json` for when there
is one (see Possible later work).

## Prepare the release PR

```sh
git switch -c release/v0.X.Y main

$EDITOR Cargo.toml                                # [workspace.package] version = "0.X.Y"
$EDITOR crates/pelican/Cargo.toml crates/pelican-shell/Cargo.toml
                                                  # the pelican-core dependency's version = "0.X.Y"
$EDITOR crates/pelican-shell/tauri.conf.json      # "version": "0.X.Y"
$EDITOR CHANGELOG.md                              # move [Unreleased] into [0.X.Y] - YYYY-MM-DD
$EDITOR packaging/flatpak/io.github.n0ble_s1x.Pelican.metainfo.xml
                                                  # a <release version="0.X.Y" date="YYYY-MM-DD">
                                                  # entry at the top of <releases>, and the
                                                  # screenshot URLs pointing at the v0.X.Y tag
$EDITOR packaging/aur/PKGBUILD
                                                  # pkgver=0.X.Y, pkgrel=1

# The same gate the maintainer runs before every merge:
./scripts/check.sh --full

git add Cargo.toml Cargo.lock crates/pelican-shell/tauri.conf.json CHANGELOG.md \
  packaging/flatpak/io.github.n0ble_s1x.Pelican.metainfo.xml packaging/aur
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

## Check the Flatpak from the tag

Before the store PRs, build the Flatpak locally from the tag and try it with
the watch (`packaging/flatpak/README.md` has the one-time setup):

```sh
git switch --detach v0.X.Y
flatpak run org.flatpak.Builder --user --force-clean --install \
  build-flatpak packaging/flatpak/io.github.n0ble_s1x.Pelican.yaml
flatpak run io.github.n0ble_s1x.Pelican
```

On the USB rule: a Flatpak cannot install it. The first time the watch
cannot be opened, the window shows the one command to run on the host.
The AUR package installs the rule itself.

## Publish the GitHub release

No files are attached; the release is the signed tag and its notes.

```sh
gh release create v0.X.Y --verify-tag --title "v0.X.Y" \
  --notes-file <(awk '/^## \[0.X.Y\]/{f=1; print; next} f && /^## /{exit} f' CHANGELOG.md)
```

## Update each store

Do the stores after the GitHub release: both build from the tag.

### AUR

One package, `pelican`, built from the tag on the user's machine, in its own
repository `ssh://aur@aur.archlinux.org/pelican.git`:

```sh
# In the AUR clone
cp /path/to/pelican/packaging/aur/PKGBUILD .
updpkgsums                  # replaces the placeholder sha256 with the real one
makepkg -f                  # builds and runs the tests
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
acceptance, an update is a PR to `flathub/io.github.n0ble_s1x.Pelican`:

```sh
# In the flathub/io.github.n0ble_s1x.Pelican clone
git switch -c update-0.X.Y
$EDITOR io.github.n0ble_s1x.Pelican.yaml   # the git source's tag: v0.X.Y and
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
- **AUR:** revert the `PKGBUILD` commit and push.
- **Flathub:** revert the manifest commit; the bot republishes the previous
  version.

Prefer shipping a `v0.X.Y+1` fix to yanking.

## Possible later work

- An AppImage built in an Ubuntu 22.04 container (`podman`), attached to
  the release: `cd crates/pelican-shell && cargo tauri build` inside it.
  Launch-check it against sockets that do not exist: with
  `WAYLAND_DISPLAY` merely unset, GTK falls back to `wayland-0` and opens a
  real window, which looks for a watch.
- Prebuilt `aarch64` binaries.
