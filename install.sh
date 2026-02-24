#!/bin/bash
set -e

OWNER="woochaq"
REPO="tuneli-tui"
BIN_NAME="tuneli-tui"

echo "Installing $BIN_NAME..."

# Detect OS
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
case "$OS" in
    linux*)     OS_EXT="linux";;
    darwin*)    OS_EXT="macos";;
    *)          echo "Unsupported OS: $OS"; exit 1;;
esac

# Detect Architecture
ARCH="$(uname -m)"
case "$ARCH" in
    x86_64)     ARCH_EXT="amd64";;
    aarch64|arm64) ARCH_EXT="aarch64";;
    *)          echo "Unsupported architecture: $ARCH"; exit 1;;
esac

# Fetch latest release URL
LATEST_RELEASE_API="https://api.github.com/repos/$OWNER/$REPO/releases/latest"
echo "Fetching latest release information from GitHub..."

# Extract the browser_download_url for the matching asset.
# This assumes asset naming convention: tuneli-tui-linux-amd64.tar.gz or similar
DOWNLOAD_URL=$(curl -s $LATEST_RELEASE_API | grep "browser_download_url" | grep "$OS_EXT" | grep "$ARCH_EXT" | cut -d '"' -f 4 | head -n 1)

if [ -z "$DOWNLOAD_URL" ]; then
    echo "Error: Could not find a suitable release payload for $OS_EXT ($ARCH_EXT) on GitHub."
    echo "Please build from source using 'cargo install --path .'"
    exit 1
fi

echo "Downloading $DOWNLOAD_URL..."
TMP_DIR=$(mktemp -d)
curl -L -o "$TMP_DIR/release.tar.gz" "$DOWNLOAD_URL"

echo "Extracting payload..."
tar -xzf "$TMP_DIR/release.tar.gz" -C "$TMP_DIR"

# Find the binary
EXTRACTED_BIN=$(find "$TMP_DIR" -type f -name "$BIN_NAME" | head -n 1)

if [ -z "$EXTRACTED_BIN" ]; then
    echo "Error: Could not find executable inside the downloaded archive."
    rm -rf "$TMP_DIR"
    exit 1
fi

chmod +x "$EXTRACTED_BIN"

INSTALL_PATH="/usr/local/bin/$BIN_NAME"
echo "Installing to $INSTALL_PATH..."
if [ -w "/usr/local/bin" ]; then
    mv "$EXTRACTED_BIN" "$INSTALL_PATH"
else
    echo "Sudo privileges required to install to /usr/local/bin."
    sudo mv "$EXTRACTED_BIN" "$INSTALL_PATH"
fi

rm -rf "$TMP_DIR"
echo "✅ Successfully installed $BIN_NAME to $INSTALL_PATH"
echo "Run '$BIN_NAME' to get started!"
