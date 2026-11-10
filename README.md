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
- `libs/opti-wm/`: Pure, hardened `#![no_std]` window geometry, z-order, and hit-testing engine.
- `libs/opti-libc/`: Pure `#![no_std]` userland standard library, syscall wrappers, and dynamic heap allocator (`sys_brk`).
- `userland/`: Standalone unprivileged Ring 3 binaries (`/bin/sysinfo`, `/bin/cat`, `/bin/optifetch`).
- `scripts/`: Build, packaging, host testing, disk formatting, and QEMU execution scripts.
- `docs/`: Technical architecture, locking, POSIX semantics, and Architectural Decision Records (ADRs).

## Milestone 9 Capabilities

- **Desktop Window Manager & Compositor (`gui` / `desktop`):** On-demand graphical desktop launched from `opti-sh`. Features tear-free double-buffered compositing on the linear GOP framebuffer (1280x800x32bpp), top system bar, dock/taskbar, and floating windows with full drag-and-drop movement.
- **PS/2 Auxiliary Device (Mouse) Driver (IRQ 12):** Intel 8042 controller communication, Compaq Status Byte configuration, and 3-byte packet decoding over IDT vector 44 with Dual 8259 PIC EOI management and screen boundary clamping.
- **High-Contrast Hardware Cursor Sprite:** 12x19 pixel arrow cursor with a 1px black outline and bright white interior rendered on top of the composition.
- **Four Native Diagnostic Mini Apps:**
  1. *Diagnostic Terminal (`opti-sh`):* Interactive shell session window inside the graphical desktop.
  2. *Hardware & SMBIOS Inspector:* CPU brand string, CPUID feature flags, motherboard/firmware details, and PCI inventory.
  3. *Storage & S.M.A.R.T. Manager:* SATA AHCI drives, GPT partition tables, and real-time S.M.A.R.T. health metrics.
  4. *Memory & PMM Monitor:* Graphical visual progress bars for physical PMM frame allocation and kernel virtual heap.
- **Pure Window Geometry Engine (`libs/opti-wm`):** Modular `#![no_std]` library managing window z-ordering, hierarchical hit-testing (`TitleBar`, `CloseButton`, `Body`, `Desktop`), and boundary clamping.
- **Terminal Cleanliness & UTF-8 Decoder:** Internal kernel logs route exclusively to COM1 serial and `dmesg` once the shell starts; 2-byte UTF-8 decoder maps `\xC3\xA3` -> `ã` and `\xC3\xB3` -> `ó` to accurately render `João Figueiró`.
- **Pre-Allocated User Paging Structures (Zero Leaks):** Pre-allocates intermediate page tables (PDPT, PD, PT) during `syscall::init()`, ensuring exact physical frame parity (`delta = 0`, `zero leak: true`) from the very first process execution.
- **POSIX Userland & ANSI Terminal:** Ring 3 unprivileged execution with dynamic `sys_brk` heap, System V AMD64 ABI `argc`/`argv` passing, `/proc` telemetry, and fastfetch clone `/bin/optifetch`.

## Testing

### Host Unit Tests

OptiOS separates pure protocol and parsing logic into host-testable library crates (`optios-ustar`, `optios-ahci-proto`, `optios-partition-table`, `optios-elf-loader`, `optios-ansi-parser`, and `optios-wm`).

To run host unit tests (62 tests):

```bash
./scripts/test.sh
```

Or invoke `cargo test` directly for library crates:

```bash
cargo test -p optios-ustar -p optios-ahci-proto -p optios-partition-table -p optios-elf-loader -p optios-ansi-parser -p optios-wm --target x86_64-unknown-linux-gnu
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
