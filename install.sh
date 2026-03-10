#!/bin/bash
set -e

echo "Building ClipKeeper (release)..."
cargo build --release

BINARY="target/release/clipkeeper"
INSTALL_DIR="${HOME}/.local/bin"

mkdir -p "$INSTALL_DIR"
cp "$BINARY" "$INSTALL_DIR/clipkeeper"
chmod +x "$INSTALL_DIR/clipkeeper"

# Create default config and data directories
CONFIG_DIR="${HOME}/.config/clipkeeper"
DATA_DIR="${HOME}/.local/share/clipkeeper"
mkdir -p "$CONFIG_DIR" "$DATA_DIR"
chmod 700 "$CONFIG_DIR" "$DATA_DIR"

echo "✓ Installed to ${INSTALL_DIR}/clipkeeper"
echo "  Config: ${CONFIG_DIR}"
echo "  Data:   ${DATA_DIR}"
echo ""
echo "Make sure ${INSTALL_DIR} is in your PATH."
