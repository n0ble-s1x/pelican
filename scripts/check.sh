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
# Claims the product makes rather than preferences, and ones that fail
# silently: a network-capable dependency compiles in without a word, and a
# delete path is one careless call away from destroying a track.

step "invariants"

# SECURITY.md promises the binary bundles no HTTP/TLS code. The lockfile
# alone cannot be the evidence — what is actually *compiled* is checked.
for crate in reqwest hyper tower-http; do
    if [[ -n "$(cargo tree -e normal -i "$crate" 2>/dev/null)" ]]; then
        fail "$crate is compiled into the binary — the no-network claim in SECURITY.md is broken"
    fi
done

# There is no delete in Pelican: it cannot take a track out of the watch's
# library and a delete-then-write is how names get reused
# (docs/rebuild-plan.md). The Backend trait has no such method; this keeps
# the mtp-rs call from creeping back in underneath it.
if git grep -nE '(storage|session)\.delete\(|delete_object' -- 'crates/*.rs'; then
    fail "a device delete call is back in the tree — Pelican has no delete"
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
