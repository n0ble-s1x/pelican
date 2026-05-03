#!/usr/bin/env bash
# Install git hooks that run scripts/check.sh on commit/push. Opt-in: this is
# NOT auto-installed when you clone — you choose to run this script.
#
#   ./scripts/install-hooks.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HOOKS="$ROOT/.git/hooks"

if [[ ! -d "$HOOKS" ]]; then
    echo "no .git/hooks directory — are you in a git working tree?"
    exit 1
fi

# pre-commit: fast checks (fmt, clippy)
cat > "$HOOKS/pre-commit" <<'EOF'
#!/usr/bin/env bash
exec "$(git rev-parse --show-toplevel)/scripts/check.sh"
EOF
chmod +x "$HOOKS/pre-commit"

# pre-push: full checks (+ tests, audit, deny)
cat > "$HOOKS/pre-push" <<'EOF'
#!/usr/bin/env bash
exec "$(git rev-parse --show-toplevel)/scripts/check.sh" --full
EOF
chmod +x "$HOOKS/pre-push"

echo "✓ pre-commit and pre-push hooks installed"
echo "  pre-commit runs scripts/check.sh"
echo "  pre-push   runs scripts/check.sh --full"
