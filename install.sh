#!/bin/sh
set -e

REPO="4fuu/open-browser-cli"
BIN_NAME="browser-cli"
INSTALL_DIR="$HOME/.local/bin"

# detect OS and arch
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Linux)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-unknown-linux-musl" ;;
      aarch64) TARGET="aarch64-unknown-linux-musl" ;;
      *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Darwin)
    case "$ARCH" in
      x86_64)  TARGET="x86_64-apple-darwin" ;;
      arm64)   TARGET="aarch64-apple-darwin" ;;
      *) echo "Unsupported arch: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS" >&2
    exit 1
    ;;
esac

# resolve version
if [ -z "$VERSION" ]; then
  VERSION="$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
    | grep '"tag_name"' | sed 's/.*"tag_name": *"\(.*\)".*/\1/')"
fi

DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/${BIN_NAME}-${TARGET}.tar.gz"

echo "Installing $BIN_NAME $VERSION ($TARGET) -> $INSTALL_DIR"

mkdir -p "$INSTALL_DIR"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

curl -fsSL "$DOWNLOAD_URL" | tar -xz -C "$TMP"
chmod +x "$TMP/$BIN_NAME"
mv "$TMP/$BIN_NAME" "$INSTALL_DIR/$BIN_NAME"

echo "Installed: $INSTALL_DIR/$BIN_NAME"

# check PATH
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo ""
    echo "Add the following to your shell profile (~/.bashrc / ~/.zshrc):"
    echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    ;;
esac
