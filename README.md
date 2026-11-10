<!--
// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Project Overview & Testing Instructions
// Date: 2026-11-09
-->

# OptiOS

OptiOS is an independent, 64-bit operating system developed from scratch in Rust (`no_std`), targeting modern x86-64 hardware with UEFI firmware and the Limine bootloader. Its initial purpose is to provide a UEFI-bootable live environment for computer hardware diagnostics and recovery.

## Project Structure

- `kernel/`: Custom bare-metal x86-64 `no_std` kernel.
- `libs/ustar/`: Pure, hardened USTAR archive parser for the initial ramdisk (initramfs).
- `libs/ahci-proto/`: Pure AHCI 1.0/1.3 protocol structures, bitfield builders, and ATA IDENTIFY decoder.
- `scripts/`: Build, packaging, host testing, and QEMU execution scripts.
- `docs/`: Technical architecture, locking, POSIX semantics, and Architectural Decision Records (ADRs).

## Current Milestone Scope & Limitations

- **SATA AHCI Storage:** Read-only drive discovery and ATA `IDENTIFY DEVICE` (`0xEC`) interrogation. Queries drive model, serial number, firmware revision, LBA48 total sectors, and S.M.A.R.T. feature set availability/enablement bits (from IDENTIFY words 82 and 85). Does **not** execute sector reads/writes (e.g. `READ DMA EXT`), SMART data attribute reads, or SMART self-tests.
- **POSIX VFS & Initramfs:** In-memory read-only filesystem mounted at `/` backed by the bootloader-provided USTAR initramfs archive. `vfs::open()` returns a global node index, and `vfs::read()` operates like `pread` with an explicit offset parameter. Does **not** implement per-process file descriptor tables, open file description cursors, `close()`, flags, or `lseek()`.

## Testing

### Host Unit Tests

OptiOS separates pure protocol and parsing logic into host-testable library crates (`optios-ustar` and `optios-ahci-proto`).

To run host unit tests:

```bash
./scripts/test.sh
```

Or invoke `cargo test` directly for individual library crates:

```bash
cargo test -p optios-ustar -p optios-ahci-proto --target x86_64-unknown-linux-gnu
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

To execute boot-time selftests (USTAR initramfs, VFS, and AHCI bounded polling/recovery):

```bash
./scripts/build.sh release --features selftest
./scripts/run.sh --test
```
