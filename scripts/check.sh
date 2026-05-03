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
#   quick: fmt-check, clippy --all-targets -D warnings, cargo build --release
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
    cargo audit

    step "deny (license + supply-chain)"
    require cargo-deny "cargo install cargo-deny --locked"
    cargo deny check
fi

printf "\n%s✓ all checks passed (%s mode)%s\n" "${GREEN}" "$MODE" "${RESET}"
