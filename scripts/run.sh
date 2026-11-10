#!/usr/bin/env bash
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: QEMU Q35 + OVMF Execution & Automated Milestone 5 Verification Script
# Date: 2026-11-09

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${PROJECT_ROOT}"

BUILD_DIR="${PROJECT_ROOT}/build"
ISO_IMAGE="${BUILD_DIR}/optios.iso"
TEST_DISK="${BUILD_DIR}/test_disk.img"
OVMF_BIOS="/usr/share/ovmf/OVMF.fd"
SERIAL_LOG="${BUILD_DIR}/serial.log"
SCREENSHOT_PPM="${BUILD_DIR}/screenshot.ppm"
SCREENSHOT_PNG="${BUILD_DIR}/screenshot.png"
SCREENSHOT_TAB4_PPM="${BUILD_DIR}/screenshot_m5_tab4.ppm"
SCREENSHOT_TAB4_PNG="${BUILD_DIR}/screenshot_m5_tab4.png"
SOCK="/tmp/qemu-optios-$$.sock"

MODE="${1:---gui}"

if [ ! -f "${ISO_IMAGE}" ]; then
    echo "==> ISO image not found. Triggering build..."
    ./scripts/build.sh release
fi

if [ ! -f "${OVMF_BIOS}" ]; then
    echo "ERROR: OVMF UEFI firmware not found at ${OVMF_BIOS}" >&2
    exit 1
fi

mkdir -p "${BUILD_DIR}"

# Provision 64 MiB virtual SATA disk for AHCI inspection if not present
if [ ! -f "${TEST_DISK}" ]; then
    echo "==> [OptiOS Test] Creating 64 MiB virtual SATA disk image (${TEST_DISK})..."
    dd if=/dev/zero of="${TEST_DISK}" bs=1M count=64 status=none
fi

if [ "${MODE}" = "--test" ] || [ "${MODE}" = "--headless" ]; then
    echo "==> [OptiOS Test] Executing automated verification on QEMU (Q35 + AHCI SSD, TCG mode, no KVM)..."
    rm -f "${SERIAL_LOG}" "${SCREENSHOT_PPM}" "${SCREENSHOT_PNG}" "${SCREENSHOT_TAB4_PPM}" "${SCREENSHOT_TAB4_PNG}" "${SOCK}"

    # Launch QEMU with Q35 chipset and attached SATA AHCI disk in background
    qemu-system-x86_64 \
        -machine q35 \
        -bios "${OVMF_BIOS}" \
        -cdrom "${ISO_IMAGE}" \
        -drive id=satadisk0,file="${TEST_DISK}",if=none,format=raw \
        -device ide-hd,drive=satadisk0,bus=ide.0,model="OptiOS_Test_SSD",serial="OPTI-2026-9901" \
        -m 512M \
        -serial file:"${SERIAL_LOG}" \
        -display none \
        -no-reboot \
        -monitor unix:"${SOCK}",server,nowait &
    QEMU_PID=$!

    # Allow time for UEFI bootstrap, kernel init, VFS mount, AHCI DMA IDENTIFY, and initial render (5.0s)
    sleep 5.0

    # Connect to QEMU monitor socket to inject keyboard events (IRQ 1), capture screendumps, and exit
    if [ -S "${SOCK}" ]; then
        python3 -c "
import socket, time
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    sock.connect('${SOCK}')
    time.sleep(0.3)
    # Switch to Tab 2 (PCI Inventory)
    sock.sendall(b'sendkey 2\n')
    time.sleep(0.3)
    # Switch to Tab 3 (Memory & Heap)
    sock.sendall(b'sendkey 3\n')
    time.sleep(0.3)
    # Switch to Tab 4 (Storage AHCI & VFS Initramfs) and capture Milestone 5 Tab 4 screenshot
    sock.sendall(b'sendkey 4\n')
    time.sleep(0.5)
    sock.sendall(b'screendump ${SCREENSHOT_TAB4_PPM}\n')
    time.sleep(0.4)
    # Also copy Tab 4 as the primary screenshot
    sock.sendall(b'screendump ${SCREENSHOT_PPM}\n')
    time.sleep(0.3)
    # Switch back to Tab 1 (System & SMBIOS)
    sock.sendall(b'sendkey 1\n')
    time.sleep(0.3)
    sock.sendall(b'quit\n')
    time.sleep(0.2)
finally:
    sock.close()
" || true
    fi

    # Ensure QEMU process terminated
    wait "${QEMU_PID}" 2>/dev/null || true
    rm -f "${SOCK}"

    echo "==> [OptiOS Test] Serial Console Output:"
    echo "--------------------------------------------------------"
    cat "${SERIAL_LOG}"
    echo "--------------------------------------------------------"

    # Objective compliance assertions for Milestone 5
    grep -q "OptiOS Kernel — Milestone 5" "${SERIAL_LOG}" || { echo "FAILURE: Milestone 5 boot banner missing"; exit 1; }
    grep -q "Limine Protocol: Base revision handshake successful." "${SERIAL_LOG}" || { echo "FAILURE: Limine protocol handshake missing"; exit 1; }
    grep -q "VFS initialized: Mounted USTAR initramfs" "${SERIAL_LOG}" || { echo "FAILURE: VFS USTAR initramfs mount missing"; exit 1; }
    grep -q "VFS Read Test (/etc/optios-release): NAME=\"OptiOS\"" "${SERIAL_LOG}" || { echo "FAILURE: POSIX VFS read test of /etc/optios-release failed"; exit 1; }
    grep -q "AHCI Controller" "${SERIAL_LOG}" || { echo "FAILURE: SATA AHCI controller initialization missing"; exit 1; }
    grep -q "OptiOS_Test_SSD" "${SERIAL_LOG}" || { echo "FAILURE: AHCI ATA IDENTIFY model 'OptiOS_Test_SSD' missing"; exit 1; }
    grep -q "OPTI-2026-9901" "${SERIAL_LOG}" || { echo "FAILURE: AHCI ATA IDENTIFY serial 'OPTI-2026-9901' missing"; exit 1; }
    grep -q "Timer Verification: PIT IRQ 0 active" "${SERIAL_LOG}" || { echo "FAILURE: PIT IRQ 0 timer tick verification failed"; exit 1; }
    grep -q "Keyboard Event: Switched dashboard view to Tab 4" "${SERIAL_LOG}" || { echo "FAILURE: Keyboard IRQ 1 Tab 4 switch missing"; exit 1; }

    # Convert screendumps if available
    if [ -f "${SCREENSHOT_TAB4_PPM}" ]; then
        echo "==> [OptiOS Test] Screendump PPM generated (${SCREENSHOT_TAB4_PPM}). Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_TAB4_PPM}" "${SCREENSHOT_TAB4_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_PPM}" ]; then
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_PPM}" "${SCREENSHOT_PNG}" || true
    fi

    echo "==> [OptiOS Test] Milestone 5 Automated Verification: PASSED SUCCESSFULLY!"
else
    echo "==> [OptiOS Run] Launching QEMU (Q35 chipset + AHCI SSD) with serial forwarded to stdio..."
    qemu-system-x86_64 \
        -machine q35 \
        -bios "${OVMF_BIOS}" \
        -cdrom "${ISO_IMAGE}" \
        -drive id=satadisk0,file="${TEST_DISK}",if=none,format=raw \
        -device ide-hd,drive=satadisk0,bus=ide.0,model="OptiOS_Test_SSD",serial="OPTI-2026-9901" \
        -m 512M \
        -serial stdio \
        -no-reboot
fi
