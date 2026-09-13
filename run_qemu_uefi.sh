#!/usr/bin/env bash
# ============================================================================
# AuraOS UEFI QEMU Launcher Script
# ============================================================================
# Boots AuraOS natively in 64-bit UEFI mode using OVMF firmware.

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
KERNEL_BIN="$SCRIPT_DIR/target/x86_64-unknown-none/release/aura-kernel"
UEFI_BIN="$SCRIPT_DIR/boot/uefi/target/x86_64-unknown-uefi/release/bootx64.efi"
ESP_DIR="$SCRIPT_DIR/target/esp"

# Parse command-line options
ENABLE_GDB=0
FORCE_REBUILD=0
HEADLESS=0
ENABLE_NET=1
SMP_COUNT=1
ENABLE_DEBUG_EXIT=1
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
        --smp)
            SMP_COUNT="$2"
            shift 2
            ;;
        --no-debug-exit)
            ENABLE_DEBUG_EXIT=0
            shift
            ;;
        --help|-h)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --debug, -d       Enable GDB stub on port 1234 and wait for connection"
            echo "  --rebuild, -r     Force rebuild of kernel & UEFI bootloader before launch"
            echo "  --nographic, -n   Run headless without VGA window (display none)"
            echo "  --no-net          Disable e1000 virtual network interface"
            echo "  --smp N           Number of virtual CPUs (default 1; use 2+ to test SMP)"
            echo "  --no-debug-exit   Do not attach the isa-debug-exit test device"
            echo "  --help, -h        Show this help message"
            exit 0
            ;;
        *)
            EXTRA_ARGS+=("$1")
            shift
            ;;
    esac
done

# Locate OVMF firmware
OVMF_PATH=""
OVMF_CANDIDATES=(
    "/usr/share/edk2/x64/OVMF.4m.fd"
    "/usr/share/OVMF/OVMF.fd"
    "/usr/share/ovmf/x64/OVMF.fd"
    "/usr/share/edk2-ovmf/x64/OVMF.fd"
    "/usr/share/edk2/ovmf/OVMF.fd"
)

for candidate in "${OVMF_CANDIDATES[@]}"; do
    if [ -f "$candidate" ]; then
        OVMF_PATH="$candidate"
        break
    fi
done

if [ -z "$OVMF_PATH" ]; then
    echo "[Error] OVMF firmware not found. Please install edk2-ovmf / ovmf package." >&2
    exit 1
fi

# Build kernel ELF if missing or requested
if [ ! -f "$KERNEL_BIN" ] || [ "$FORCE_REBUILD" -eq 1 ]; then
    echo "[AuraOS] Building release kernel ELF..."
    cd "$SCRIPT_DIR"
    cargo +nightly build --release
fi

# Build UEFI bootloader if missing or requested
if [ ! -f "$UEFI_BIN" ] || [ "$FORCE_REBUILD" -eq 1 ]; then
    echo "[AuraOS] Building UEFI bootloader..."
    cd "$SCRIPT_DIR"
    cargo +nightly build --manifest-path boot/uefi/Cargo.toml --target x86_64-unknown-uefi --release
fi

# Prepare EFI System Partition directory structure
mkdir -p "$ESP_DIR/EFI/BOOT"
cp "$UEFI_BIN" "$ESP_DIR/EFI/BOOT/BOOTX64.EFI"

echo "============================================================"
echo "  Launching AuraOS in UEFI QEMU Virtual Machine..."
echo "  - Firmware   : $OVMF_PATH"
echo "  - ESP Boot   : $ESP_DIR/EFI/BOOT/BOOTX64.EFI"
echo "  - Memory     : 256 MB"
echo "  - Serial Log : stdio (COM1 UART at 115200 baud)"
echo "  - Network    : $([ "$ENABLE_NET" -eq 1 ] && echo "e1000 (User NAT)" || echo "Disabled")"
echo "  - Debug GDB  : $([ "$ENABLE_GDB" -eq 1 ] && echo "Active (:1234, waiting)" || echo "Off")"
echo "  - CPUs       : $SMP_COUNT"
echo "  - Test Exit  : $([ "$ENABLE_DEBUG_EXIT" -eq 1 ] && echo "isa-debug-exit (port 0xf4)" || echo "Disabled")"
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
    -bios "$OVMF_PATH"
    -drive "file=fat:rw:$ESP_DIR,media=disk,format=raw"
    -m 256M
    -serial stdio
    -smp "$SMP_COUNT"
)

if [ "$ENABLE_DEBUG_EXIT" -eq 1 ]; then
    CMD+=(-device "isa-debug-exit,iobase=0xf4")
fi

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
