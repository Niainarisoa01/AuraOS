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

# Launch QEMU with raw hard drive image and COM1 directed to stdio
qemu-system-x86_64 \
    -drive format=raw,file="$BIN_PATH" \
    -serial stdio
