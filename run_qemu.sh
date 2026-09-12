#!/usr/bin/env bash
# ============================================================================
# AuraOS QEMU Launcher Script
# ============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_PATH="$SCRIPT_DIR/target/x86_64-unknown-none/release/bootimage-aura-kernel.bin"

# Parse command-line options
ENABLE_GDB=0
FORCE_REBUILD=0
HEADLESS=0
ENABLE_NET=1
EXTRA_ARGS=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --debug|-d)
            ENABLE_GDB=1
            shift
            ;;
        --rebuild|-r)
            FORCE_REBUILD=1
            shift
            ;;
        --nographic|-n)
            HEADLESS=1
            shift
            ;;
        --no-net)
            ENABLE_NET=0
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --debug, -d      Enable GDB stub on port 1234 and wait for connection"
            echo "  --rebuild, -r    Force rebuild of release bootimage before launch"
            echo "  --nographic, -n  Run headless without VGA window (display none)"
            echo "  --no-net         Disable e1000 virtual network interface"
            echo "  --help, -h       Show this help message"
            exit 0
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

# Build bootable disk image if missing or requested
if [ ! -f "$BIN_PATH" ] || [ "$FORCE_REBUILD" -eq 1 ]; then
    echo "[AuraOS] Building release bootimage..."
    cd "$SCRIPT_DIR"
    cargo +nightly bootimage --release
fi

echo "============================================================"
echo "  Launching AuraOS in QEMU Virtual Machine..."
echo "  - Disk Image : $BIN_PATH"
echo "  - Memory     : 256 MB"
echo "  - Serial Log : stdio (COM1 UART at 115200 baud)"
echo "  - Network    : $([ "$ENABLE_NET" -eq 1 ] && echo "e1000 (User NAT)" || echo "Disabled")"
echo "  - Debug GDB  : $([ "$ENABLE_GDB" -eq 1 ] && echo "Active (:1234, waiting)" || echo "Off")"
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

CMD=(
    "$QEMU_BIN"
    -drive "format=raw,file=$BIN_PATH"
    -m 256M
    -serial stdio
)

if [ "$ENABLE_NET" -eq 1 ]; then
    CMD+=(
        -netdev user,id=net0
        -device e1000,netdev=net0
    )
fi

if [ "$ENABLE_GDB" -eq 1 ]; then
    CMD+=(-s -S)
fi

if [ "$HEADLESS" -eq 1 ]; then
    CMD+=(-display none)
fi

CMD+=("${EXTRA_ARGS[@]}")

exec "${CMD[@]}"
