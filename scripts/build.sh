#!/usr/bin/env bash
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: Kernel Compilation, USTAR Initramfs Builder & UEFI ISO Packager
# Date: 2026-11-10

set -euo pipefail

# Project root directory
PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${PROJECT_ROOT}"

BUILD_MODE="${1:-release}"
EXTRA_CARGO_FLAGS="${@:2}"
TARGET="x86_64-unknown-none"
BUILD_DIR="${PROJECT_ROOT}/build"
INITRAMFS_STAGE="${BUILD_DIR}/initramfs_root"
ISO_ROOT="${BUILD_DIR}/iso_root"
OUTPUT_ISO="${BUILD_DIR}/optios.iso"
TEST_DISK="${BUILD_DIR}/test_disk.img"
LIMINE_DIR="${PROJECT_ROOT}/vendor/limine"

# Ensure test disk with GPT is formatted
if [ ! -f "${TEST_DISK}" ]; then
    python3 "${PROJECT_ROOT}/scripts/format_gpt_disk.py" "${TEST_DISK}"
fi

echo "==> [OptiOS Build] Compilation mode: ${BUILD_MODE} ${EXTRA_CARGO_FLAGS}"

# 1. Compile the kernel binary
if [ "${BUILD_MODE}" = "release" ]; then
    cargo build --package optios-kernel --release ${EXTRA_CARGO_FLAGS}
    KERNEL_BIN="${PROJECT_ROOT}/target/${TARGET}/release/optios-kernel"
else
    cargo build --package optios-kernel ${EXTRA_CARGO_FLAGS}
    KERNEL_BIN="${PROJECT_ROOT}/target/${TARGET}/debug/optios-kernel"
fi

if [ ! -f "${KERNEL_BIN}" ]; then
    echo "ERROR: Kernel binary not found at ${KERNEL_BIN}" >&2
    exit 1
fi

echo "==> [OptiOS Build] Kernel compiled: ${KERNEL_BIN}"

# 1.1 Compile Ring 3 Userland Utilities (/bin/sysinfo, /bin/cat, /bin/optifetch)
echo "==> [OptiOS Build] Compiling Ring 3 userland utilities..."
RUSTFLAGS="-C relocation-model=static -C link-arg=-Tuserland/linker.ld -C link-arg=-nostdlib -C link-arg=-static -C link-arg=-z -C link-arg=max-page-size=0x1000" \
    cargo build --package optios-sysinfo --package optios-cat --package optios-optifetch --release --target "${TARGET}"
SYSINFO_BIN="${PROJECT_ROOT}/target/${TARGET}/release/optios-sysinfo"
CAT_BIN="${PROJECT_ROOT}/target/${TARGET}/release/optios-cat"
OPTIFETCH_BIN="${PROJECT_ROOT}/target/${TARGET}/release/optios-optifetch"

if [ ! -f "${SYSINFO_BIN}" ] || [ ! -f "${CAT_BIN}" ] || [ ! -f "${OPTIFETCH_BIN}" ]; then
    echo "ERROR: Userland binaries not found at target directory" >&2
    exit 1
fi

# 2. Provision Limine UEFI binaries if not present
if [ ! -f "${LIMINE_DIR}/BOOTX64.EFI" ] || [ ! -f "${LIMINE_DIR}/limine-uefi-cd.bin" ]; then
    echo "==> [OptiOS Build] Cloning official Limine UEFI binaries (v8.x-binary)..."
    mkdir -p "${LIMINE_DIR}"
    git clone --depth 1 -b v8.x-binary https://github.com/limine-bootloader/limine "${LIMINE_DIR}"
fi

# 3. Generate USTAR Initramfs archive (/boot/initramfs.tar)
rm -rf "${INITRAMFS_STAGE}"
mkdir -p "${INITRAMFS_STAGE}/etc" "${INITRAMFS_STAGE}/bin" "${INITRAMFS_STAGE}/usr/share/hwdata"

cp "${SYSINFO_BIN}" "${INITRAMFS_STAGE}/bin/sysinfo"
cp "${CAT_BIN}" "${INITRAMFS_STAGE}/bin/cat"
cp "${OPTIFETCH_BIN}" "${INITRAMFS_STAGE}/bin/optifetch"

cat << 'EOF' > "${INITRAMFS_STAGE}/etc/optios-release"
NAME="OptiOS"
VERSION="1.0.0-alpha (Milestone 8)"
ID=optios
ARCH="x86_64"
FIRMWARE="UEFI"
BOOTLOADER="Limine v8.x"
VFS_ROOT="USTAR Initramfs (Read-Only) + /proc"
STORAGE_STACK="SATA AHCI 1.0 DMA + S.M.A.R.T. Telemetry"
USERLAND="Ring 3 (Syscalls, sys_brk alloc, argc/argv & ELF64)"
EOF

cat << 'EOF' > "${INITRAMFS_STAGE}/etc/motd"
OptiOS 1.0
Copyright (C) 2026 João Figueiró. All Rights Reserved.
EOF

cat << 'EOF' > "${INITRAMFS_STAGE}/usr/share/hwdata/pci.ids"
8086  Intel Corporation
	29c0  82G33/G31/P35/P31 Express DRAM Controller (Q35)
	2918  82801IB (ICH9) LPC Interface Controller
	2922  82801IR/IO/IH (ICH9R/DO/DH) 6 port SATA Controller [AHCI mode]
	2930  82801I (ICH9 Family) SMBus Controller
	10d3  82574L Gigabit Network Connection
1234  QEMU / Bochs Virtual Device
	1111  QEMU Standard VGA Framebuffer
EOF

cat << 'EOF' > "${INITRAMFS_STAGE}/etc/diag-manifest.conf"
[diagnostics]
enable_smbios=true
enable_pci_scan=true
enable_ahci_smart=true
enforce_readonly_storage=true
EOF

# 4. Prepare ISO filesystem tree
rm -rf "${ISO_ROOT}"
mkdir -p "${ISO_ROOT}/EFI/BOOT" "${ISO_ROOT}/boot"

tar --format=ustar -cf "${ISO_ROOT}/boot/initramfs.tar" -C "${INITRAMFS_STAGE}" \
    etc/optios-release \
    etc/motd \
    etc/diag-manifest.conf \
    usr/share/hwdata/pci.ids \
    bin/sysinfo \
    bin/cat \
    bin/optifetch

echo "==> [OptiOS Build] USTAR Initramfs created: ${ISO_ROOT}/boot/initramfs.tar"

# Copy kernel and Limine bootloader assets
cp "${KERNEL_BIN}" "${ISO_ROOT}/optios-kernel"
cp "${LIMINE_DIR}/limine-uefi-cd.bin" "${ISO_ROOT}/"
cp "${LIMINE_DIR}/BOOTX64.EFI" "${ISO_ROOT}/EFI/BOOT/BOOTX64.EFI"

# Write Limine configuration
cat << 'EOF' > "${ISO_ROOT}/limine.conf"
timeout: 0
default_entry: 1

/OptiOS
    protocol: limine
    path: boot():/optios-kernel
    module_path: boot():/boot/initramfs.tar
EOF

# 5. Generate hybrid UEFI bootable ISO via xorriso
mkdir -p "${BUILD_DIR}"
xorriso -as mkisofs -R -r -J \
    -efi-boot-part --efi-boot-image \
    -e limine-uefi-cd.bin \
    -no-emul-boot \
    -o "${OUTPUT_ISO}" \
    "${ISO_ROOT}" 2>&1 | grep -v "xorriso : NOTE :"

echo "==> [OptiOS Build] Bootable UEFI ISO created successfully:"
echo "    ${OUTPUT_ISO}"
