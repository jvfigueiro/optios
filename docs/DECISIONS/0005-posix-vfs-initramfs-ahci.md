# ADR 0005: POSIX Foundation, USTAR Initramfs VFS, and Read-Only SATA AHCI Driver

## Status

Accepted — 2026-11-09

## Context

To evolve OptiOS into a functional live diagnostic and recovery environment aligned with Unix/POSIX principles, the kernel requires:

1. **POSIX Error Semantics (`Errno`):** A canonical error representation (`POSIX.1-2024`) shared across virtual filesystem operations, device drivers, and future system calls.
2. **Virtual File System (VFS) & Initial RAM Filesystem (Initramfs):** A root filesystem (`/`) mounted in memory at boot time from a bootloader-provided archive (`initramfs.tar`), enabling the kernel and diagnostic tools to inspect configuration files, release metadata (`/etc/optios-release`), and hardware databases (`/usr/share/hwdata/pci.ids`).
3. **SATA AHCI Storage Driver & S.M.A.R.T. Telemetry:** A native driver for the Intel ICH9 / PCIe SATA AHCI 1.0 controller (`01:06:01`) using Memory-Mapped I/O (MMIO) via `ABAR` (`BAR5`) and DMA Command Lists to issue ATA `IDENTIFY DEVICE` (`0xEC`), extracting drive model, serial number, firmware revision, LBA48 sector count, and S.M.A.R.T. health capabilities without risking writes to attached media.

## Decisions

1. **POSIX Error Code Module (`kernel/src/posix/errno.rs`):**
   - Define `Errno` with standard POSIX integer codes (`EPERM`, `ENOENT`, `EIO`, `EBADF`, `ENOMEM`, `EACCES`, `EFAULT`, `EBUSY`, `EEXIST`, `ENODEV`, `ENOTDIR`, `EISDIR`, `EINVAL`, `ENOSYS`) and `pub type KernelResult<T> = Result<T, Errno>;`.
2. **USTAR Initramfs & Read-Only VFS (`kernel/src/fs/tarfs.rs` & `vfs.rs`):**
   - Generate `/boot/initramfs.tar` in standard POSIX.1-1988 USTAR format (`ustar` magic at offset 257, 512-byte blocks) during `scripts/build.sh` and load it via Limine's `ModuleRequest`.
   - Expose POSIX-style VFS operations (`vfs::open`, `vfs::read`, `vfs::stat`, `vfs::list_dir`) operating directly over the in-memory USTAR archive mounted at `/`.
3. **Read-Only SATA AHCI Driver (`kernel/src/drivers/ahci.rs`):**
   - Locate PCI class `0x01`, subclass `0x06`, ProgIF `0x01` (AHCI 1.0).
   - Enable PCI Bus Master (bit 2) and Memory Space (bit 1) in the PCI Command register (`0x04`).
   - Map the HBA Memory Registers (`ABAR` / `BAR5`) into the HHDM with `PRESENT | WRITABLE | WRITE_THROUGH | CACHE_DISABLE | NO_EXECUTE` (`PWT` and `PCD` bits in x86-64 PTEs) to guarantee uncached MMIO accesses.
   - Allocate 4 KiB-aligned physical frames via `pmm::alloc_frame()` for the Command List, Received FIS, and Command Table (`CFIS` + `PRDT`), and issue ATA `IDENTIFY DEVICE` (`0xEC`) to active SATA ports (`SSTS.DET == 3`, `SSTS.IPM == 1`, `SIG == 0x00000101`).
   - Strictly enforce **Read-Only / Diagnostic Mode**: no sector write commands (`WRITE DMA EXT`, etc.) are implemented or exposed.

## Consequences

- OptiOS can inspect real and virtual SATA drives and read files from `/` via standard POSIX VFS semantics immediately upon boot.
- Uncached MMIO page flags (`WRITE_THROUGH | CACHE_DISABLE`) in the VMM prevent stale CPU cache reads when polling AHCI HBA registers (`PxCI`, `PxIS`, `PxTFD`).
