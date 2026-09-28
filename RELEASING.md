# Releasing Pelican

How to cut a release. The source of truth is a signed tag on `main`. Every
distribution channel is a recipe that points at that tag.

`main` is protected: changes land through pull requests with squash merges,
so the version bump goes through a PR like any other change, and only the
tag is pushed directly.

## Prepare the release PR

```sh
git switch -c release/v0.X.Y main

$EDITOR Cargo.toml                                # [workspace.package] version = "0.X.Y"
$EDITOR crates/pelican-shell/tauri.conf.json      # "version": "0.X.Y"
$EDITOR CHANGELOG.md                              # move [Unreleased] into [0.X.Y] - YYYY-MM-DD

# The same gate the maintainer runs before every merge:
./scripts/check.sh --full

git add Cargo.toml Cargo.lock crates/pelican-shell/tauri.conf.json CHANGELOG.md
git commit -m "release: v0.X.Y"
git push -u origin release/v0.X.Y
gh pr create --title "release: v0.X.Y" --body "Version bump and changelog for v0.X.Y."
```

All three crates inherit the version through `version.workspace`.

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

```sh
cargo build --release -p pelican -p pelican-shell

# Debian package of the CLI (config in crates/pelican/Cargo.toml)
cargo install cargo-deb                 # one-time
cargo deb --release -p pelican          # target/debian/pelican_0.X.Y_amd64.deb

# Tarball with both binaries, the udev rule and the desktop entry.
# Each -C is relative to the one before it.
tar -czf pelican-0.X.Y-linux-x86_64.tar.gz \
  -C target/release pelican pelican-app \
  -C ../../udev 70-garmin-mtp.rules \
  -C ../packaging/desktop pelican.desktop \
  -C ../.. README.md LICENSE-MIT LICENSE-APACHE
```

The app can also be bundled by Tauri as a `.deb` and an AppImage
(`bundle.targets` in `crates/pelican-shell/tauri.conf.json`) with
`cargo tauri build`. That path has not been used for a release yet; check
its output before attaching it.

## Publish the GitHub release

```sh
gh release create v0.X.Y \
  target/debian/pelican_0.X.Y_amd64.deb \
  pelican-0.X.Y-linux-x86_64.tar.gz \
  --title "v0.X.Y" \
  --notes-file <(awk '/^## \[0.X.Y\]/{f=1; print; next} f && /^## /{exit} f' CHANGELOG.md)
```

## Update each store

### AUR

```sh
# In the AUR clone (a separate repo: ssh://aur@aur.archlinux.org/pelican.git)
sed -i "s/^pkgver=.*/pkgver=0.X.Y/" PKGBUILD
updpkgsums              # updates sha256sums for the new tarball
makepkg --printsrcinfo > .SRCINFO
git commit -am "v0.X.Y"
git push
```

The `PKGBUILD` in `packaging/aur/` packages the CLI only.

### Flathub

Pelican is not on Flathub yet; `packaging/flatpak/README.md` has the steps
for a first submission. The manifest in this repo uses a `type: dir` source,
which only works for local builds. For Flathub it must be a `type: git`
source pinned to the release tag and commit. After acceptance, an update is:

```sh
# In the flathub/com.krypteia.Pelican clone
$EDITOR com.krypteia.Pelican.yaml   # set the git source's tag: and commit:

python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  /path/to/pelican/Cargo.lock -o cargo-sources.json

git commit -am "Update to v0.X.Y"
git push
```

### crates.io

Pelican is not published to crates.io. It is an application, and direct
download is the supported install.

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

## Possible later automation

- One script from `cargo build` through `gh release create`.
- Prebuilt `aarch64` binaries.
