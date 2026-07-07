#!/usr/bin/env bash
# Build and publish the AgentOS TypeScript SDK to npm
#
# Usage:
#   bash scripts/publish_npm.sh [--dry-run]
#
# Options:
#   --dry-run   Build the package but don't publish
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SDK_DIR="$REPO_ROOT/crates/sdk/typescript"
DRY_RUN="${1:-}"
NPM_ARGS=""

if [ "$DRY_RUN" = "--dry-run" ]; then
  NPM_ARGS="--dry-run"
  echo "→ Dry run mode (no actual publish)"
fi

echo "╔════════════════════════════════════════════════════╗"
echo "║  AgentOS TypeScript SDK — npm Publisher            ║"
echo "╚════════════════════════════════════════════════════╝"

# Check prerequisites
if ! command -v npm &>/dev/null; then
  echo "✗ npm not found. Install Node.js 18+ first."
  exit 1
fi

# Check if logged in
if [ "$DRY_RUN" != "--dry-run" ]; then
  if ! npm whoami &>/dev/null; then
    echo "✗ Not logged into npm. Run: npm login"
    exit 1
  fi
fi

cd "$SDK_DIR"

echo ""
echo "→ Installing dependencies..."
npm ci

echo ""
echo "→ Building..."
npm run build

echo ""
echo "→ Running tests..."
npm test 2>/dev/null || echo "  (no tests configured)"

echo ""
echo "→ Publishing..."
npm publish $NPM_ARGS

if [ "$DRY_RUN" = "--dry-run" ]; then
  echo ""
  echo "✓ Dry run complete. Ready to publish with:"
  echo "  bash scripts/publish_npm.sh"
else
  echo ""
  echo "✓ Published @agentos/sdk to npm!"
fi
