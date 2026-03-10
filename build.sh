#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

LINUX_TARGET="x86_64-unknown-linux-gnu"
WINDOWS_TARGET="x86_64-pc-windows-gnu"

echo "Building clipkeeper (Rust version)..."
echo ""

if ! command -v cargo &> /dev/null; then
    echo "Error: cargo not found. Install Rust from https://rustup.rs/"
    exit 1
fi

# Ensure Linux target is installed
echo "Ensuring build targets are installed..."
rustup target add "$LINUX_TARGET" 2>/dev/null || true

FAILED=0

# --- Linux build ---
echo ""
echo "=== Building for Linux ($LINUX_TARGET) ==="
if cargo build --release --target "$LINUX_TARGET"; then
    echo "✓ Linux build successful: target/$LINUX_TARGET/release/clipkeeper"
else
    echo "✗ Linux build failed."
    FAILED=1
fi

# --- Windows build (cross-compilation via cross) ---
echo ""
echo "=== Building for Windows ($WINDOWS_TARGET) ==="

if ! command -v cross &> /dev/null; then
    echo "✗ Skipping Windows build: 'cross' not found."
    echo "  Install with: cargo install cross"
else
    if cross build --release --target "$WINDOWS_TARGET"; then
        echo "✓ Windows build successful: target/$WINDOWS_TARGET/release/clipkeeper.exe"
    else
        echo "✗ Windows build failed."
        FAILED=1
    fi
fi

# --- Summary ---
echo ""
echo "==============================="
echo "  Build Summary"
echo "==============================="
[ -f "target/$LINUX_TARGET/release/clipkeeper" ] && echo "  Linux:   target/$LINUX_TARGET/release/clipkeeper" || echo "  Linux:   not built"
[ -f "target/$WINDOWS_TARGET/release/clipkeeper.exe" ] && echo "  Windows: target/$WINDOWS_TARGET/release/clipkeeper.exe" || echo "  Windows: not built"
echo ""

exit $FAILED
