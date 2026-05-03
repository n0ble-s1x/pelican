# Releasing Pelican

Step-by-step for cutting a release. Source of truth is **`main` on GitHub** at
a tagged commit. Every distribution channel is a thin recipe pointing at that
tag.

## Pre-release (every time)

```sh
# Bump version
$EDITOR Cargo.toml          # version = "0.X.Y"
$EDITOR CHANGELOG.md        # add the [0.X.Y] section, move stuff out of [Unreleased]

# Sanity gate — same checks the local QA runs
./scripts/check.sh --full
```

## Cut the release

```sh
# Commit + tag
git add Cargo.toml CHANGELOG.md
git commit -m "release: v0.X.Y"
git tag -s v0.X.Y -m "v0.X.Y"   # signed tag (or unsigned with `git tag -a`)
git push origin main v0.X.Y

# Build artifacts
cargo build --release
cargo install cargo-deb           # one-time
cargo deb --release               # → target/debian/pelican_0.X.Y_amd64.deb

# Stripped + tarball'd binary for direct download
strip target/release/pelican
tar -czf pelican-0.X.Y-linux-x86_64.tar.gz \
  -C target/release pelican \
  -C ../../udev 99-garmin-music.rules \
  -C ../../packaging/desktop pelican.desktop \
  -C ../.. README.md LICENSE-MIT LICENSE-APACHE

# GitHub release page
gh release create v0.X.Y \
  target/debian/pelican_0.X.Y_amd64.deb \
  pelican-0.X.Y-linux-x86_64.tar.gz \
  --title "v0.X.Y" \
  --notes-file <(awk '/^## \[0.X.Y\]/,/^## \[/{if(NR>1 && /^## \[/)exit; print}' CHANGELOG.md)
```

## Update each store

### AUR (Arch / Manjaro)

```sh
# In your AUR clone (separate repo: ssh://aur@aur.archlinux.org/pelican.git)
sed -i "s/^pkgver=.*/pkgver=0.X.Y/" PKGBUILD
updpkgsums              # updates sha256sums for the new tarball
makepkg --printsrcinfo > .SRCINFO
git commit -am "v0.X.Y"
git push
```

Users get the update on next `yay -Syu` (or `pacman -Syu` if installed via repo).

### Flathub (Cosmic Store, GNOME Software, etc.)

Flathub keeps the manifest in its own per-app repo
(`flathub/com.krypteia.Pelican` once we're accepted). To push an update:

```sh
# In the flathub/com.krypteia.Pelican clone
$EDITOR com.krypteia.Pelican.yaml   # bump the `tag:` for the source git ref
                                    # (or `commit:` to a SHA)

# Regenerate vendored cargo sources for offline build
python3 flatpak-builder-tools/cargo/flatpak-cargo-generator.py \
  /path/to/pelican/Cargo.lock -o cargo-sources.json

git commit -am "Update to v0.X.Y"
git push origin master

# Flathub bot picks it up, builds, and publishes within ~hours.
```

For a **brand-new** app (first time submitting), see
[packaging/flatpak/README.md](packaging/flatpak/README.md).

### crates.io (optional)

If we ever publish the library:

```sh
cargo publish --dry-run
cargo publish
```

We have **not** published Pelican to crates.io as of v0.1.0 (it's an
application, not a library — direct download is preferred).

## Coordinating versions

Store-name-to-our-version mapping is **always 1:1** for the same git tag.
We don't ship one feature on AUR and a different one on Flathub. If you bump
the version anywhere, bump it everywhere.

The dependable workflow:

1. Release on GitHub first (tag + artifacts)
2. Open store-update PRs **against the same tag** (AUR, Flathub)
3. Wait for each to land
4. Update README badges if you have version pins anywhere

## Reverting

If something ships broken:

- **GitHub release**: `gh release delete v0.X.Y --yes`; tag stays unless you also
  `git push origin :refs/tags/v0.X.Y`
- **AUR**: revert the PKGBUILD commit and push
- **Flathub**: revert the manifest commit; the bot republishes the prior version

Always preferable to ship a `v0.X.Y+1` patch fix rather than yank.

## What to automate later

- One script that does everything from `cargo deb` through `gh release create`
- Pre-built binaries for `aarch64` (Raspberry Pi / Pinetab)
- Auto-bump dependent manifests via a `release-please`-style bot

Not worth the time at v0.1; manual is fine until releases get frequent.
