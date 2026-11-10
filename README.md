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
- `scripts/`: Build, packaging, host testing, disk formatting, and QEMU execution scripts.
- `docs/`: Technical architecture, locking, POSIX semantics, and Architectural Decision Records (ADRs).

## Milestone 6 Capabilities

- **SATA AHCI Storage & DMA Engine:** Read-only drive discovery, ATA `IDENTIFY DEVICE` (`0xEC`), ATA `READ DMA EXT` (`0x25` LBA48), and ATA `SMART READ DATA` (`0xB0` / `0xD0`) interrogation. Sector reads are chunked in 8-sector (4 KiB) transactions referencing dedicated single physical page frames. Strict read-only safety is guaranteed by construction through `AllowedCommand`.
- **Partition Discovery (MBR/GPT):** Discovers Protective MBR (`0xEE`) and parses GUID Partition Tables (GPT) with IEEE 802.3 CRC32 header and entry array validation and UTF-16LE partition label decoding.
- **RAM Double Buffering:** Eliminates visual tearing on the UEFI GOP linear framebuffer by allocating 4000 KiB from PMM mapped at `0xFFFF_D800_0000_0000`, flushing to physical MMIO via 64-bit word transfers on `present()`.
- **Interactive Diagnostic Shell (`opti-sh`):** Accessible via Tab 5 / `F5`, featuring line editing, history navigation, scrollback, and 12 built-in diagnostic commands (`help`, `uname`/`ver`, `ls`, `cat`, `stat`, `lspci`, `lsblk`, `smart`, `free`/`meminfo`, `smbios`, `clear`, `uptime`).
- **POSIX VFS & Initramfs:** In-memory read-only filesystem mounted at `/` backed by the bootloader-provided USTAR initramfs archive.

## Testing

### Host Unit Tests

OptiOS separates pure protocol and parsing logic into host-testable library crates (`optios-ustar`, `optios-ahci-proto`, and `optios-partition-table`).

To run host unit tests:

```bash
./scripts/test.sh
```

Or invoke `cargo test` directly for library crates:

```bash
cargo test -p optios-ustar -p optios-ahci-proto -p optios-partition-table --target x86_64-unknown-linux-gnu
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
