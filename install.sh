#!/usr/bin/env bash
set -euo pipefail

# AgentOS one-line install
# Usage: curl -fsSL https://raw.githubusercontent.com/WAHIB-EL-KHADIRI/agentOS/main/install.sh | bash

REPO="WAHIB-EL-KHADIRI/agentOS"
BIN_DIR="${AGENTOS_BIN:-$HOME/.agentos/bin}"
VERSION="${AGENTOS_VERSION:-latest}"

say() { printf "\033[1;32m%s\033[0m\n" "$*"; }
err() { printf "\033[1;31m%s\033[0m\n" "$*" >&2; exit 1; }

# Sanitize version tag to prevent path injection
sanitize_tag() {
  printf '%s' "$1" | tr -d '/\0'
}

ARCH="$(uname -m)"
OS="$(uname -s)"
case "$OS" in
  Linux) TARGET="x86_64-unknown-linux-gnu" ;;
  Darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      *) TARGET="x86_64-apple-darwin" ;;
    esac
    ;;
  *) err "Unsupported OS: $OS. Please install from source: cargo install --path crates/cli" ;;
esac

if command -v agentOS >/dev/null 2>&1; then
  say "AgentOS is already installed at $(command -v agentOS)"
  say "Re-run this script with AGENTOS_VERSION=<tag> to install a specific release."
fi

if [ "$VERSION" = "latest" ]; then
  API_URL="https://api.github.com/repos/$REPO/releases/latest"
  TAG="$(curl -fsSL "$API_URL" | grep '"tag_name"' | cut -d'"' -f4)"
  [ -z "$TAG" ] && err "Could not resolve latest release from GitHub API. Build from source with: cargo install --path crates/cli"
else
  TAG="$(sanitize_tag "$VERSION")"
fi

ARCHIVE="agentOS-${TAG}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/$REPO/releases/download/$TAG/$ARCHIVE"

say "Downloading AgentOS $TAG for $TARGET"
mkdir -p "$BIN_DIR"

TMPFILE="$(mktemp -t agentos-XXXXXXXX.XXXX.tar.gz)"
trap 'rm -f "$TMPFILE"' EXIT

curl -fsSL --connect-timeout 30 --max-time 120 "$DOWNLOAD_URL" -o "$TMPFILE"
tar xzf "$TMPFILE" -C "$BIN_DIR"
chmod +x "$BIN_DIR/agentOS"

for tool in agentOS agentos; do
  ln -sf "$BIN_DIR/agentOS" "$BIN_DIR/$tool" 2>/dev/null || true
done

case "${SHELL:-}" in
  *zsh*) RC="$HOME/.zshrc" ;;
  *bash*) RC="$HOME/.bashrc" ;;
  *fish*) RC="$HOME/.config/fish/config.fish" ;;
  *) RC="" ;;
esac

if [ -n "$RC" ]; then
  LINE="export PATH=\"\$PATH:$BIN_DIR\""
  grep -qF "$BIN_DIR" "$RC" 2>/dev/null || echo "$LINE" >> "$RC"
fi

say "AgentOS $TAG installed to $BIN_DIR"
say ""
say "Make sure $BIN_DIR is in your PATH, or run:"
say "  export PATH=\"\$PATH:$BIN_DIR\""
say ""
say "Quick start:"
say "  agentOS quickstart"
say "  agentOS run --help"
