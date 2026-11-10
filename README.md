<!--
// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Project Overview & Testing Instructions
// Date: 2026-11-10
-->

# OptiOS

OptiOS is an independent, 64-bit operating system developed from scratch in Rust (`no_std`), targeting modern x86-64 hardware with UEFI firmware and the Limine bootloader. Its initial purpose is to provide a UEFI-bootable live environment for computer hardware diagnostics and recovery.

## Project Structure

- `kernel/`: Custom bare-metal x86-64 `no_std` kernel.
- `libs/ustar/`: Pure, hardened USTAR archive parser for the initial ramdisk (initramfs).
- `libs/ahci-proto/`: Pure AHCI 1.0/1.3 protocol structures, bitfield builders, S.M.A.R.T. parser, and `BlockDevice` trait.
- `libs/partition-table/`: Pure, hardened MBR and GPT partition table parsers with CRC32 integrity validation.
- `libs/elf-loader/`: Pure, hardened ELF64 binary parser and `PT_LOAD` segment validator.
- `userland/`: Standalone unprivileged Ring 3 binaries (e.g., `userland/sysinfo`).
- `scripts/`: Build, packaging, host testing, disk formatting, and QEMU execution scripts.
- `docs/`: Technical architecture, locking, POSIX semantics, and Architectural Decision Records (ADRs).

## Milestone 7 Capabilities

- **Pure Fullscreen Text Console (`/dev/tty0` Style):** Replaces kernel GUI dashboards with a classical, high-contrast text terminal (160x50 characters at 1280x800) rendered over linear GOP video with 4 MiB RAM double buffering and hardware/software row scrolling.
- **Boot Verbose on Screen & COM1:** Real-time synchronized kernel logging across video display and COM1 serial console, complete with an in-memory `dmesg` ring buffer.
- **Ring 3 Unprivileged Userland Execution:** AMD64 hardware MSRs (`IA32_EFER.SCE`, `IA32_STAR`, `IA32_LSTAR`, `IA32_FMASK`) configured for `syscall`/`sysretq` dispatch. Unprivileged execution occurs in Ring 3 (`CS=0x23`, `SS=0x1B`) isolated from Ring 0 with a dedicated TSS `rsp[0]` kernel stack.
- **POSIX System Call Subsystem:** Implements core POSIX system calls: `sys_read` (0), `sys_write` (1), `sys_open` (2), `sys_close` (3), `sys_stat` (4), `sys_getpid` (39), `sys_exit` (60), and `sys_uname` (63) with strict `< 0x0000_8000_0000_0000` memory bounds validation.
- **ELF64 Loader (`libs/elf-loader`):** Standalone library parsing ELF64 headers and `PT_LOAD` segments with comprehensive unit tests and zero-fill BSS support.
- **Ring 3 Userland Utility (`/bin/sysinfo`):** Standalone `no_std` Rust binary packaged inside `/boot/initramfs.tar` verifying Ring 3 execution and syscall interoperation.
- **SATA AHCI Storage & DMA Engine:** Read-only drive discovery, ATA `IDENTIFY DEVICE` (`0xEC`), ATA `READ DMA EXT` (`0x25` LBA48), and ATA `SMART READ DATA` (`0xB0` / `0xD0`) interrogation. Strict read-only safety is guaranteed by construction through `AllowedCommand`.
- **Partition Discovery (MBR/GPT):** Discovers Protective MBR (`0xEE`) and parses GUID Partition Tables (GPT) with IEEE 802.3 CRC32 header and entry array validation and UTF-16LE partition label decoding.
- **Interactive Diagnostic Shell (`opti-sh`):** Featuring line editing, history navigation, scrollback, external ELF binary execution, and built-in diagnostic commands (`help`, `uname`, `ls`, `cat`, `stat`, `lspci`, `lsblk`, `smart`, `free`, `smbios`, `dmesg`, `clear`, `uptime`). Prompt: `optios - # `.

## Testing

### Host Unit Tests

OptiOS separates pure protocol and parsing logic into host-testable library crates (`optios-ustar`, `optios-ahci-proto`, `optios-partition-table`, and `optios-elf-loader`).

To run host unit tests:

```bash
./scripts/test.sh
```

Or invoke `cargo test` directly for library crates:

```bash
cargo test -p optios-ustar -p optios-ahci-proto -p optios-partition-table -p optios-elf-loader --target x86_64-unknown-linux-gnu
```

> **Note on Whole-Workspace `cargo test`:**
> Running `cargo test --workspace` or `cargo test` from the workspace root will fail because `optios-kernel` is a `#![no_std]` bare-metal binary crate that defines a custom `#[panic_handler]`. When `cargo test` runs against a binary crate, Cargo links `std` for its test harness, leading to duplicate `panic_impl` symbol definition errors between `std` and the kernel's bare-metal panic handler. Host unit testing is therefore intentionally partitioned to library packages using `./scripts/test.sh`.

### Bare-Metal & QEMU Integration Testing

To build the kernel and bootable UEFI ISO:

```bash
./scripts/build.sh release
```

To run under QEMU with modern Q35 PCIe / SATA AHCI chipset emulation:

```bash
./scripts/run.sh
```

To execute automated non-interactive verification:

```bash
./scripts/run.sh --test
```

To execute boot-time selftests (USTAR initramfs, VFS, AHCI bounded polling/recovery, DMA LBA48 reads, and S.M.A.R.T. read):

```bash
./scripts/build.sh release --features selftest
./scripts/run.sh --test
```
