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
- `libs/ansi-parser/`: Pure, hardened `#![no_std]` ANSI/VT100 escape sequence parser.
- `libs/opti-libc/`: Pure `#![no_std]` userland standard library, syscall wrappers, and dynamic heap allocator (`sys_brk`).
- `userland/`: Standalone unprivileged Ring 3 binaries (`/bin/sysinfo`, `/bin/cat`, `/bin/optifetch`).
- `scripts/`: Build, packaging, host testing, disk formatting, and QEMU execution scripts.
- `docs/`: Technical architecture, locking, POSIX semantics, and Architectural Decision Records (ADRs).

## Milestone 8 Capabilities

- **ANSI / VT100 Terminal Emulation:** Standalone `#![no_std]` parser (`libs/ansi-parser`) powering the fullscreen console (`160x50` at `1280x800`). Supports SGR (0 reset, 1 bold, 30..37 standard FG, 90..97 bright FG, 40..47 standard BG, 100..107 bright BG), clear screen (`\x1b[2J`), and cursor positioning (`\x1b[H`, `\x1b[r;cH`).
- **Dynamic Userland Heap (`sys_brk` - 12):** Allows Ring 3 processes to dynamically expand their program break up to 16 MiB. PMM dynamically allocates 4 KiB frames with zero-initialization and `NO_EXECUTE` protection.
- **Zero-Memory-Leak Reclamation:** Complete process memory cleanup on `sys_exit` (60), unmapping VMM tables and freeing all physical frames (code, stack, heap) back to PMM.
- **System V AMD64 ABI Argument Passing (`argc`/`argv`):** Full POSIX argument passing laid out on the user stack with pointer array and NUL-terminated strings.
- **File Descriptors & `sys_lseek` (8):** Userland file descriptor table (`3..16`) supporting random access seeking with `SEEK_SET`, `SEEK_CUR`, and `SEEK_END`.
- **Virtual Telemetry Pseudo-Filesystem (`/proc`):** Synthetic files generated on the fly (`/proc/cpuinfo`, `/proc/meminfo`, `/proc/uptime`, `/proc/pci`, `/proc/disks`) seamlessly integrated into VFS.
- **Userland Standard Library (`libs/opti-libc`):** Provides syscall wrappers, `print!`/`println!` macros, and `OptiAllocator` implementing `GlobalAlloc` via `sys_brk` (enabling `extern crate alloc`).
- **Real Ring 3 Applications:**
  - `/bin/optifetch`: Fastfetch clone in unprivileged Ring 3 with colored ASCII art, dynamic telemetry, and ANSI palette display.
  - `/bin/cat`: Userland utility reading files from VFS or `/proc` via `argc`/`argv`.
  - `/bin/sysinfo`: System information and `sys_uname` tester.
- **Pure Fullscreen Text Console (`/dev/tty0` Style):** High-contrast text terminal rendered over linear GOP video with 4 MiB RAM double buffering and hardware/software row scrolling.
- **Boot Verbose on Screen & COM1:** Real-time synchronized kernel logging across video display and COM1 serial console, complete with an in-memory `dmesg` ring buffer.
- **SATA AHCI Storage & DMA Engine:** Read-only drive discovery, ATA `IDENTIFY DEVICE` (`0xEC`), ATA `READ DMA EXT` (`0x25` LBA48), and ATA `SMART READ DATA` (`0xB0` / `0xD0`) interrogation. Strict read-only safety is guaranteed by construction through `AllowedCommand`.
- **Partition Discovery (MBR/GPT):** Discovers Protective MBR (`0xEE`) and parses GUID Partition Tables (GPT) with IEEE 802.3 CRC32 header and entry array validation and UTF-16LE partition label decoding.
- **Interactive Diagnostic Shell (`opti-sh`):** Featuring line editing, history navigation, scrollback, external ELF binary execution, and built-in diagnostic commands. Prompt: `optios - # `.

## Testing

### Host Unit Tests

OptiOS separates pure protocol and parsing logic into host-testable library crates (`optios-ustar`, `optios-ahci-proto`, `optios-partition-table`, `optios-elf-loader`, and `optios-ansi-parser`).

To run host unit tests (57 tests):

```bash
./scripts/test.sh
```

Or invoke `cargo test` directly for library crates:

```bash
cargo test -p optios-ustar -p optios-ahci-proto -p optios-partition-table -p optios-elf-loader -p optios-ansi-parser --target x86_64-unknown-linux-gnu
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
