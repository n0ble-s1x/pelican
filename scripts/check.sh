#!/usr/bin/env bash
# Pelican local QA gate.
#
# Replaces a CI runner. We do not use GitHub Actions: every executable thing
# runs on YOUR machine, where you can audit it. Contributors are expected to
# run this before opening a PR; the maintainer runs it before merge.
#
# Quick mode (the default; fast, suitable for pre-commit):
#   ./scripts/check.sh
#
# Full mode (slow; suitable for pre-push and pre-release):
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
        fail "missing tool: $1 (install with: $2)"
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
# alone cannot be the evidence, so what is actually *compiled* is checked.
for crate in reqwest hyper tower-http; do
    if [[ -n "$(cargo tree -e normal -i "$crate" 2>/dev/null)" ]]; then
        fail "$crate is compiled into the binary: the no-network claim in SECURITY.md is broken"
    fi
done

# There is no delete in Pelican: it cannot take a track out of the watch's
# library and a delete-then-write is how names get reused
# (docs/rebuild-plan.md). The Backend trait has no such method; this keeps
# the mtp-rs call from creeping back in underneath it.
if git grep -nE '(storage|session)\.delete\(|delete_object' -- 'crates/*.rs'; then
    fail "a device delete call is back in the tree: Pelican has no delete"
fi

# The window's IPC surface is four lists that must agree: the commands the
# build script ACL-gates, the ones `generate_handler!` registers, the ones
# the capability grants, and the ones `ui/app.js` invokes. A command registered but not gated is callable
# by anything in the webview; one granted but not registered is a grant
# nobody reviewed. The capability may add only event listening (never a
# `*:default` set), and the CSP may name no remote origin and no
# 'unsafe-inline'.
shell=crates/pelican-shell
gated=$(sed -n '/commands(&\[/,/\])/p' "$shell/build.rs" | grep -oE '"[a-z_]+"' | tr -d '"' | sort)
handled=$(sed -n '/generate_handler!\[/,/\]/p' "$shell/src/main.rs" | grep -oE 'commands::[a-z_]+' | sed 's/commands:://' | sort)
granted=$(grep -oE '"allow-[a-z-]+"' "$shell/capabilities/main.json" | tr -d '"' | sed 's/^allow-//; s/-/_/g' | sort)
if [[ "$gated" != "$handled" || "$gated" != "$granted" ]]; then
    diff <(echo "$gated") <(echo "$handled") || true
    diff <(echo "$gated") <(echo "$granted") || true
    fail "pelican-shell: build.rs, generate_handler! and capabilities/main.json list different commands"
fi
invoked=$(grep -oE 'api\.invoke\("[a-z_]+"' ui/app.js | grep -oE '"[a-z_]+"' | tr -d '"' | sort -u)
if [[ "$gated" != "$invoked" ]]; then
    diff <(echo "$gated") <(echo "$invoked") || true
    fail "pelican-shell: the commands ui/app.js invokes differ from the ones build.rs gates"
fi
extra=$(grep -oE '"[a-z-]+:[a-z:-]+"' "$shell/capabilities/main.json" | tr -d '"' \
    | grep -vxE 'core:event:allow-(listen|unlisten)' || true)
if [[ -n "$extra" ]]; then
    fail "pelican-shell grants more than its own commands and event listening: $extra"
fi
csp=$(grep -oE '"csp": *"[^"]*"' "$shell/tauri.conf.json")
if [[ "$csp" == *unsafe-inline* || "$csp" == *unsafe-eval* ]] \
    || grep -oE 'https?://[^ ;"]+' <<<"$csp" | grep -vqx 'http://ipc.localhost'; then
    fail "pelican-shell CSP allows inline/eval or a remote origin: $csp"
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
