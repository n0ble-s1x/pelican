#!/usr/bin/env bash
# Pelican local QA gate.
#
# Replaces a CI runner. We do not use GitHub Actions: every executable thing
# runs on YOUR machine, where you can audit it. Contributors are expected to
# run this before opening a PR; the maintainer runs it before merge.
#
# Quick mode (default — fast, suitable for pre-commit):
#   ./scripts/check.sh
#
# Full mode (slow — suitable for pre-push / pre-release):
#   ./scripts/check.sh --full
#
# What runs:
#   quick: invariants, fmt-check, clippy --all-targets -D warnings,
#          cargo build --release
#   full : quick + cargo test + cargo audit + cargo deny

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="quick"
[[ "${1:-}" == "--full" ]] && MODE="full"

GREEN=$(tput setaf 2 2>/dev/null || true)
RED=$(tput setaf 1 2>/dev/null || true)
DIM=$(tput dim 2>/dev/null || true)
RESET=$(tput sgr0 2>/dev/null || true)

step() { printf "\n%s▸ %s%s\n" "${GREEN}" "$1" "${RESET}"; }
fail() { printf "%s✕ %s%s\n" "${RED}" "$1" "${RESET}"; exit 1; }

require() {
    if ! command -v "$1" >/dev/null 2>&1; then
        fail "missing tool: $1 — install with: $2"
    fi
}

require cargo "rustup default stable"

# ── invariants that a type checker cannot express ────────────────────────
#
# Both of these are claims the product makes rather than preferences, and
# both fail silently: an `async fn` command panics only when a real watch is
# attached, and a network-capable dependency compiles in without a word.

step "invariants"

# A Tauri command declared `async fn` runs on a tokio worker inside an active
# runtime context. `MtpRsBackend` drives its transport with `rt.block_on`,
# which panics when a runtime is already entered — so an async command is a
# crash that only shows up with hardware attached. Measured, not assumed:
# see the module comment in crates/pelican-shell/src/commands.rs.
# Anchored to the start of a line so the module comment, which explains the
# rule and therefore has to say "async fn", does not trip its own check.
if grep -nE '^[[:space:]]*(pub[^ ]* )?async fn' crates/pelican-shell/src/commands.rs; then
    fail "commands.rs must contain no 'async fn' — see the note at the top of that file"
fi

# SECURITY.md promises the binary bundles no HTTP/TLS code. Tauri drags
# reqwest, hyper and tower-http into the lockfile for features this build
# does not enable, so the lockfile alone cannot be the evidence — what is
# actually *compiled* has to be checked.
for crate in reqwest hyper tower-http; do
    if [[ -n "$(cargo tree -e normal -i "$crate" 2>/dev/null)" ]]; then
        fail "$crate is compiled into the binary — the no-network claim in SECURITY.md is broken"
    fi
done

# SECURITY.md and the module comment in main.rs both claim there is no npm
# and no node_modules. A `cargo tauri init` scaffold writes a `$schema` line
# into tauri.conf.json pointing at a node_modules path; Tauri ignores the
# key, so the build stays green and the only symptom is that the one file
# meant to prove the claim is where an auditor's grep lands.
if git grep -n --cached -I 'node_modules' -- . ':!SECURITY.md' ':!crates/pelican-shell/src/main.rs' ':!scripts/check.sh'; then
    fail "node_modules is referenced in the tree — SECURITY.md claims it never appears"
fi

# The capability file is the security boundary and its description says
# "exactly the commands ui/app.js calls". `removeUnusedCommands` cannot strip
# a command that a capability grants, so a granted-but-uncalled command ships
# as live IPC surface — the exact thing a hand-audited grant list exists to
# prevent. Both directions matter: an ungranted call fails at runtime only
# when the user reaches that button.
granted=$(grep -oE '"allow-[a-z-]+"' crates/pelican-shell/capabilities/main.json \
    | tr -d '"' | sed 's/^allow-//' | tr '-' '_' | sort -u)
called=$(grep -oE "invoke\('[a-z_]+'" ui/app.js | sed "s/invoke('//; s/'//" | sort -u)
if [[ "$granted" != "$called" ]]; then
    diff <(echo "$granted") <(echo "$called") || true
    fail "capabilities/main.json and ui/app.js disagree (< granted, > called)"
fi

# cargo-audit and cargo-deny each keep their own advisory ignore list, and
# only .cargo/audit.toml carries the argument for why each entry is there.
# A drift means one of the two tools has been silenced without a reason.
deny_ids=$(grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' deny.toml | sort -u)
audit_ids=$(grep -oE 'RUSTSEC-[0-9]{4}-[0-9]{4}' .cargo/audit.toml | sort -u)
if [[ "$deny_ids" != "$audit_ids" ]]; then
    diff <(echo "$deny_ids") <(echo "$audit_ids") || true
    fail "deny.toml and .cargo/audit.toml ignore different advisories (< deny, > audit)"
fi

step "rustfmt"
cargo fmt --all -- --check || fail "rustfmt: run 'cargo fmt' to fix"

step "clippy (deny warnings)"
cargo clippy --all-targets --all-features -- -D warnings || fail "clippy: fix warnings above"

step "build (release)"
cargo build --release --all-features

if [[ "$MODE" == "full" ]]; then
    step "test"
    cargo test --all-features

    step "audit (RUSTSEC advisories)"
    require cargo-audit "cargo install cargo-audit --locked"
    # --deny warnings so unmaintained/unsound/yanked advisories fail the gate
    # too, which is what SECURITY.md already promises.
    cargo audit --deny warnings

    step "deny (license + supply-chain)"
    require cargo-deny "cargo install cargo-deny --locked"
    cargo deny check
fi

printf "\n%s✓ all checks passed (%s mode)%s\n" "${GREEN}" "$MODE" "${RESET}"
