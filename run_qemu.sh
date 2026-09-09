#!/usr/bin/env bash
# ============================================================================
# AuraOS QEMU Launcher Script
# ============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="$SCRIPT_DIR/target/x86_64-unknown-none/release/bootimage-aura-kernel.bin"

# Check if bootable disk image exists
if [ ! -f "$BIN_PATH" ]; then
    echo "[AuraOS] Bootable image not found. Building release bootimage..."
    cd "$SCRIPT_DIR"
    cargo +nightly bootimage --release
fi

echo "============================================================"
echo "  Launching AuraOS in QEMU Virtual Machine..."
echo "  - Disk Image : $BIN_PATH"
echo "  - Serial Log : stdio (COM1 UART at 115200 baud)"
echo "  - VGA Screen : Hardware text mode 80x25"
echo "============================================================"

# Locate QEMU executable
QEMU_BIN="$(command -v qemu-system-x86_64 || true)"
if [ -z "$QEMU_BIN" ] && [ -x "/opt/local/bin/qemu-system-x86_64" ]; then
    QEMU_BIN="/opt/local/bin/qemu-system-x86_64"
elif [ -z "$QEMU_BIN" ] && [ -x "/usr/local/bin/qemu-system-x86_64" ]; then
    QEMU_BIN="/usr/local/bin/qemu-system-x86_64"
fi

if [ -z "$QEMU_BIN" ]; then
    echo "[Error] qemu-system-x86_64 not found in PATH or standard directories." >&2
    exit 1
fi

# Launch QEMU with raw hard drive image and COM1 directed to stdio
"$QEMU_BIN" \
    -drive format=raw,file="$BIN_PATH" \
    -serial stdio
