#!/usr/bin/env bash
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: QEMU Q35 + OVMF Execution & Automated Milestone 6 Verification Script
# Date: 2026-11-10

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
SCREENSHOT_TAB5_PPM="${BUILD_DIR}/screenshot_m6_tab5_shell.ppm"
SCREENSHOT_TAB5_PNG="${BUILD_DIR}/screenshot_m6_tab5_shell.png"
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

# Provision 64 MiB virtual SATA disk with real GPT partition table if not present
if [ ! -f "${TEST_DISK}" ]; then
    echo "==> [OptiOS Test] Creating 64 MiB virtual SATA disk image with GPT (${TEST_DISK})..."
    python3 "${PROJECT_ROOT}/scripts/format_gpt_disk.py" "${TEST_DISK}"
fi

if [ "${MODE}" = "--test" ] || [ "${MODE}" = "--headless" ]; then
    echo "==> [OptiOS Test] Executing automated verification on QEMU (Q35 + AHCI SSD, TCG mode, no KVM)..."
    rm -f "${SERIAL_LOG}" "${SCREENSHOT_PPM}" "${SCREENSHOT_PNG}" "${SCREENSHOT_TAB4_PPM}" "${SCREENSHOT_TAB4_PNG}" "${SCREENSHOT_TAB5_PPM}" "${SCREENSHOT_TAB5_PNG}" "${SOCK}"

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
    # Switch to Tab 4 (Storage AHCI & VFS Initramfs) and capture Tab 4 screenshot
    sock.sendall(b'sendkey 4\n')
    time.sleep(0.5)
    sock.sendall(b'screendump ${SCREENSHOT_TAB4_PPM}\n')
    time.sleep(0.4)
    # Switch to Tab 5 (Interactive Diagnostic Shell - opti-sh)
    sock.sendall(b'sendkey f5\n')
    time.sleep(0.5)
    # Send interactive command: lsblk
    for k in ['l', 's', 'b', 'l', 'k', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(0.4)
    # Send interactive command: cat /etc/motd
    for k in ['c', 'a', 't', 'spc', 'slash', 'e', 't', 'c', 'slash', 'm', 'o', 't', 'd', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(0.5)
    # Capture Tab 5 shell screendump and primary screendump
    sock.sendall(b'screendump ${SCREENSHOT_TAB5_PPM}\n')
    time.sleep(0.3)
    sock.sendall(b'screendump ${SCREENSHOT_PPM}\n')
    time.sleep(0.3)
    # Switch back to Tab 1 (System & SMBIOS)
    sock.sendall(b'sendkey f1\n')
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

    # Objective compliance assertions for Milestone 6
    grep -q "OptiOS Kernel — Milestone 6" "${SERIAL_LOG}" || { echo "FAILURE: Milestone 6 boot banner missing"; exit 1; }
    grep -q "Limine Protocol: Base revision handshake successful." "${SERIAL_LOG}" || { echo "FAILURE: Limine protocol handshake missing"; exit 1; }
    grep -q "VFS initialized: Mounted USTAR initramfs" "${SERIAL_LOG}" || { echo "FAILURE: VFS USTAR initramfs mount missing"; exit 1; }
    grep -q "VFS Read Test (/etc/optios-release): NAME=\"OptiOS\"" "${SERIAL_LOG}" || { echo "FAILURE: POSIX VFS read test of /etc/optios-release failed"; exit 1; }
    grep -q "AHCI Controller" "${SERIAL_LOG}" || { echo "FAILURE: SATA AHCI controller initialization missing"; exit 1; }
    grep -q "OptiOS_Test_SSD" "${SERIAL_LOG}" || { echo "FAILURE: AHCI ATA IDENTIFY model 'OptiOS_Test_SSD' missing"; exit 1; }
    grep -q "OPTI-2026-9901" "${SERIAL_LOG}" || { echo "FAILURE: AHCI ATA IDENTIFY serial 'OPTI-2026-9901' missing"; exit 1; }
    grep -q "Partition Table: Protective MBR found" "${SERIAL_LOG}" || { echo "FAILURE: Protective MBR detection missing"; exit 1; }
    grep -q "GPT Header verified successfully" "${SERIAL_LOG}" || { echo "FAILURE: GPT header verification missing"; exit 1; }
    grep -q "EFI System" "${SERIAL_LOG}" || { echo "FAILURE: EFI System partition detection missing"; exit 1; }
    grep -q "OptiOS Customer Data" "${SERIAL_LOG}" || { echo "FAILURE: OptiOS Customer Data partition detection missing"; exit 1; }
    grep -q "Keyboard Event: Switched dashboard view to Tab 5" "${SERIAL_LOG}" || { echo "FAILURE: Keyboard Tab 5 switch missing"; exit 1; }
    grep -q "opti-sh: Executing command 'lsblk'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of 'lsblk' missing"; exit 1; }
    grep -q "opti-sh: Executing command 'cat /etc/motd'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of 'cat /etc/motd' missing"; exit 1; }

    # Convert screendumps if available
    if [ -f "${SCREENSHOT_TAB4_PPM}" ]; then
        echo "==> [OptiOS Test] Screendump PPM generated (${SCREENSHOT_TAB4_PPM}). Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_TAB4_PPM}" "${SCREENSHOT_TAB4_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_TAB5_PPM}" ]; then
        echo "==> [OptiOS Test] Screendump PPM generated (${SCREENSHOT_TAB5_PPM}). Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_TAB5_PPM}" "${SCREENSHOT_TAB5_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_PPM}" ]; then
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_PPM}" "${SCREENSHOT_PNG}" || true
    fi

    echo "==> [OptiOS Test] Milestone 6 Automated Verification: PASSED SUCCESSFULLY!"
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
