#!/bin/sh
# Install vllm-doctor from GitHub Releases.
# Usage: curl -fsSL https://vllm.doctor/install.sh | sh

set -e

REPO="vllm-doctor/vllm-doctor"
BINARY="vllm-doctor"

# Detect OS
OS="$(uname -s)"
case "$OS" in
    Linux) OS="linux" ;;
    Darwin) OS="macos" ;;
    *) echo "Unsupported OS: $OS. Use cargo install vllm-doctor instead."; exit 1 ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    arm64|aarch64) ARCH="aarch64" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Get latest release tag
TAG=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
if [ -z "$TAG" ]; then
    echo "Could not determine latest release."
    exit 1
fi

ARCHIVE="${BINARY}-${ARCH}-${OS}.tar.gz"
URL="https://github.com/$REPO/releases/download/$TAG/$ARCHIVE"

echo "Downloading $BINARY $TAG for $OS/$ARCH..."
curl -fsSL "$URL" | tar -xz -C /tmp

# Install to /usr/local/bin if writable, otherwise ~/.local/bin
INSTALL_DIR="/usr/local/bin"
if [ ! -w "$INSTALL_DIR" ]; then
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

mv /tmp/$BINARY "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"

echo "Installed $BINARY to $INSTALL_DIR/$BINARY"
echo "Run: $BINARY diagnose http://localhost:8000/metrics"
