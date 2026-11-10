#!/usr/bin/env bash
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: QEMU Q35 + OVMF Execution & Automated Milestone 10 Verification Script
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
SCREENSHOT_M8_PPM="${BUILD_DIR}/screenshot_m8.ppm"
SCREENSHOT_M8_PNG="${BUILD_DIR}/screenshot_m8.png"
SCREENSHOT_M9_CONSOLE_PPM="${BUILD_DIR}/screenshot_m9_console.ppm"
SCREENSHOT_M9_CONSOLE_PNG="${BUILD_DIR}/screenshot_m9_console.png"
SCREENSHOT_M9_DESKTOP_PPM="${BUILD_DIR}/screenshot_m9_desktop.ppm"
SCREENSHOT_M9_DESKTOP_PNG="${BUILD_DIR}/screenshot_m9_desktop.png"
SCREENSHOT_M10_CONSOLE_PPM="${BUILD_DIR}/screenshot_m10_console.ppm"
SCREENSHOT_M10_CONSOLE_PNG="${BUILD_DIR}/screenshot_m10_console.png"
SCREENSHOT_M10_EDITOR_PPM="${BUILD_DIR}/screenshot_m10_editor.ppm"
SCREENSHOT_M10_EDITOR_PNG="${BUILD_DIR}/screenshot_m10_editor.png"
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

    # Allow time for UEFI bootstrap, kernel init, VFS mount, AHCI DMA IDENTIFY, and shell session (6.5s)
    sleep 6.5

    # Connect to QEMU monitor socket to inject keyboard commands, capture terminal screenshot, and exit
    if [ -S "${SOCK}" ]; then
        python3 -c "
import socket, time
sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
try:
    sock.connect('${SOCK}')
    time.sleep(0.3)
    # 1. Send interactive command: ls /mnt/sda1
    for k in ['l', 's', 'spc', 'slash', 'm', 'n', 't', 'slash', 's', 'd', 'a', '1', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(0.8)
    # 2. Send interactive command: /bin/cat /mnt/sda1/recovery_log.txt
    for k in ['slash', 'b', 'i', 'n', 'slash', 'c', 'a', 't', 'spc', 'slash', 'm', 'n', 't', 'slash', 's', 'd', 'a', '1', 'slash', 'r', 'e', 'c', 'o', 'v', 'e', 'r', 'y', 'shift-minus', 'l', 'o', 'g', 'dot', 't', 'x', 't', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(0.8)
    # 3. Capture Fullscreen Text Console screendump with ls and cat output
    sock.sendall(b'screendump ${SCREENSHOT_M10_CONSOLE_PPM}\n')
    time.sleep(0.4)
    sock.sendall(b'screendump ${SCREENSHOT_M9_CONSOLE_PPM}\n')
    time.sleep(0.4)
    # 4. Launch Ring 3 Editor: /bin/edit /mnt/sda1/recovery_log.txt
    for k in ['slash', 'b', 'i', 'n', 'slash', 'e', 'd', 'i', 't', 'spc', 'slash', 'm', 'n', 't', 'slash', 's', 'd', 'a', '1', 'slash', 'r', 'e', 'c', 'o', 'v', 'e', 'r', 'y', 'shift-minus', 'l', 'o', 'g', 'dot', 't', 'x', 't', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(1.2)
    # 5. Capture Ring 3 Text Editor screendump
    sock.sendall(b'screendump ${SCREENSHOT_M10_EDITOR_PPM}\n')
    time.sleep(0.4)
    # 6. Exit editor back to opti-sh shell (send 'q')
    sock.sendall(b'sendkey q\n')
    time.sleep(0.8)
    # 7. Enter Desktop Window Manager: gui
    for k in ['g', 'u', 'i', 'ret']:
        sock.sendall(f'sendkey {k}\n'.encode())
        time.sleep(0.08)
    time.sleep(1.2)
    # 8. Simulate mouse movement and dragging via QEMU monitor
    sock.sendall(b'mouse_move 60 40\n')
    time.sleep(0.2)
    sock.sendall(b'mouse_button 1\n')
    time.sleep(0.2)
    sock.sendall(b'mouse_move 80 50\n')
    time.sleep(0.2)
    sock.sendall(b'mouse_button 0\n')
    time.sleep(0.4)
    # 9. Capture Desktop Graphical Environment screendump
    sock.sendall(b'screendump ${SCREENSHOT_M9_DESKTOP_PPM}\n')
    time.sleep(0.4)
    sock.sendall(b'screendump ${SCREENSHOT_PPM}\n')
    time.sleep(0.4)
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

    # Objective compliance assertions for Milestone 10
    grep -q "OptiOS Kernel — Milestone 10: FAT32 Storage VFS Mounts & Ring 3 Text Inspector" "${SERIAL_LOG}" || { echo "FAILURE: Milestone 10 boot banner missing"; exit 1; }
    grep -q "Limine Protocol: Base revision handshake successful." "${SERIAL_LOG}" || { echo "FAILURE: Limine protocol handshake missing"; exit 1; }
    grep -q "VFS initialized: Mounted USTAR initramfs" "${SERIAL_LOG}" || { echo "FAILURE: VFS USTAR initramfs mount missing"; exit 1; }
    grep -q "VFS: Mounted FAT32 filesystem on /dev/sda1" "${SERIAL_LOG}" || { echo "FAILURE: VFS FAT32 mount on /dev/sda1 missing"; exit 1; }
    grep -q "AHCI Controller" "${SERIAL_LOG}" || { echo "FAILURE: SATA AHCI controller initialization missing"; exit 1; }
    grep -q "OptiOS_Test_SSD" "${SERIAL_LOG}" || { echo "FAILURE: AHCI ATA IDENTIFY model 'OptiOS_Test_SSD' missing"; exit 1; }
    grep -q "Syscall subsystem initialized" "${SERIAL_LOG}" || { echo "FAILURE: Syscall subsystem initialization missing"; exit 1; }
    grep -q "OptiOS 1.0" "${SERIAL_LOG}" || { echo "FAILURE: OptiOS 1.0 MOTD header missing"; exit 1; }
    grep -q "Copyright (C) 2026 João Figueiró" "${SERIAL_LOG}" || grep -q "Copyright (C) 2026 Joao Figueiro" "${SERIAL_LOG}" || { echo "FAILURE: Copyright MOTD missing"; exit 1; }
    grep -q "opti-sh: Executing command 'ls /mnt/sda1'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of 'ls /mnt/sda1' missing"; exit 1; }
    grep -q "opti-sh: Executing command '/bin/cat /mnt/sda1/recovery_log.txt'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of '/bin/cat /mnt/sda1/recovery_log.txt' missing"; exit 1; }
    grep -q "Executing ELF64 binary '/bin/cat' in Ring 3" "${SERIAL_LOG}" || { echo "FAILURE: /bin/cat Ring 3 execution missing"; exit 1; }
    grep -q "OptiOS Hardware Diagnostic & System Recovery Log" "${SERIAL_LOG}" || { echo "FAILURE: recovery_log.txt content missing from cat output"; exit 1; }
    grep -q "Status: Clean Recovery Data Verified" "${SERIAL_LOG}" || { echo "FAILURE: recovery_log.txt status verified text missing"; exit 1; }
    grep -q "opti-sh: Executing command '/bin/edit /mnt/sda1/recovery_log.txt'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of '/bin/edit /mnt/sda1/recovery_log.txt' missing"; exit 1; }
    grep -q "Executing ELF64 binary '/bin/edit' in Ring 3" "${SERIAL_LOG}" || { echo "FAILURE: /bin/edit Ring 3 execution missing"; exit 1; }
    grep -q "zero leak: true" "${SERIAL_LOG}" || { echo "FAILURE: Zero memory leak verification missing"; exit 1; }
    grep -q "opti-sh: Executing command 'gui'" "${SERIAL_LOG}" || { echo "FAILURE: opti-sh execution of 'gui' missing"; exit 1; }

    # Convert screendumps if available
    if [ -f "${SCREENSHOT_M10_CONSOLE_PPM}" ]; then
        echo "==> [OptiOS Test] Console Screendump PPM generated. Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_M10_CONSOLE_PPM}" "${SCREENSHOT_M10_CONSOLE_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_M10_EDITOR_PPM}" ]; then
        echo "==> [OptiOS Test] Editor Screendump PPM generated. Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_M10_EDITOR_PPM}" "${SCREENSHOT_M10_EDITOR_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_M9_CONSOLE_PPM}" ]; then
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_M9_CONSOLE_PPM}" "${SCREENSHOT_M9_CONSOLE_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_M9_DESKTOP_PPM}" ]; then
        echo "==> [OptiOS Test] Desktop Screendump PPM generated. Converting to PNG..."
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_M9_DESKTOP_PPM}" "${SCREENSHOT_M9_DESKTOP_PNG}" || true
    fi
    if [ -f "${SCREENSHOT_PPM}" ]; then
        python3 "${PROJECT_ROOT}/scripts/ppm2png.py" "${SCREENSHOT_PPM}" "${SCREENSHOT_PNG}" || true
    fi

    echo "==> [OptiOS Test] Milestone 10 Automated Verification: PASSED SUCCESSFULLY!"
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
