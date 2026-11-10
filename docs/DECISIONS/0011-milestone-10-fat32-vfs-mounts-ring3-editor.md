# ADR 0011: Milestone 10 — Read-Only FAT32 Storage Driver, VFS Dynamic Mounts & Ring 3 Text Inspector

- **Status**: Accepted
- **Date**: 2026-11-10
- **Author**: João Figueiró / OptiOS Core Team

## Context

With Milestone 9 completing the hardware foundation, window compositor, and userland execution infrastructure, OptiOS required real storage filesystem access to fulfill its core purpose as a computer diagnostics and recovery system.

Real-world recovery scenarios mandate inspecting customer partitions (such as EFI System Partitions and USB drive FAT32 volumes) directly from unprivileged userland without risking disk corruption.

Key architectural requirements for Milestone 10:
1. **Decoupled, Pure FAT32 Driver (`libs/fat32`)**: Pure `#![no_std]` crate implementing Read-Only FAT32 parsing on any generic `BlockDevice` backend, supporting BPB parameters, FAT cluster chains, 8.3 short names, and Long File Names (LFN) in UCS-2 decoded to UTF-8.
2. **VFS Mount Table & Transparent File Routing**: Dynamic mountpoints in `vfs.rs` routing paths starting with `/mnt/sda1` to the mounted FAT32 partition, while preserving existing `/` (USTAR Initramfs) and `/proc` (kernel telemetry).
3. **Hardware Storage Integration**: Mounting AHCI Port 0 Partition 1 (discovered via GPT parser) at boot at `/mnt/sda1`.
4. **Ring 3 Kilo-Style Text Inspector & Editor (`/bin/edit`)**: Standalone userland ELF64 executable rendering full-screen file buffers with ANSI escape sequences, reverse-video status bars (`\x1b[7m`), arrow-key scrolling, and clean exit signals (`Ctrl+Q` / `0x11` or `'q'`).
5. **Keyboard Driver Raw Mode**: Instant unbuffered key capture in `sys_read(0, buf, count)` for navigation arrows (`\x1b[A`..`\x1b[D`), control characters (`Ctrl+Q`), Enter, Backspace, and Escape.

## Decisions

### 1. Pure Read-Only FAT32 Filesystem Crate (`libs/fat32`)
- Created `#![no_std]` crate `libs/fat32` consuming the `BlockDevice` trait from `libs/ahci-proto`.
- Implemented `Fat32Bpb::parse` validating Sector 0 signatures (`0x55AA`), sector size (512 bytes), power-of-two cluster sizing, FAT counts, and root cluster (>= 2).
- Implemented `PartitionBlockDevice<D>` offsetting logical LBA requests to arbitrary partition starting offsets on raw disks.
- Traversed cluster chains from FAT1 (`0x0FFF_FFF8`..`0x0FFF_FFFF` end-of-chain markers).
- Reassembled multi-part LFN directory entries in ascending sequence order, decoding UTF-16LE characters into clean UTF-8 strings.
- Added 6 host unit tests testing BPB parsing, 8.3 short names, LFN files, directory traversal, and multi-cluster file reading (bringing total host test suite to 68 tests passing in 0.00s).

### 2. VFS Namespace Mountpoint Routing (`kernel/src/fs/vfs.rs`)
- Assigned distinct file descriptor ranges:
  - `0x0000_0000..0x4000_0000`: USTAR Initramfs files (`/`).
  - `0x4000_0000..0x8000_0000`: FAT32 partition files (`/mnt/sda1`).
  - `0x8000_0000..0xFFFF_FFFF`: Pseudo-filesystem files (`/proc`).
- Implemented `mount_fat32_sda1(port, start_lba, total_sectors)` encapsulating the AHCI port in a `KernelAhciDevice` and initializing `Fat32FileSystem`.
- Routed `vfs::open`, `vfs::read`, `vfs::close`, `vfs::stat`, and `vfs::list_dir` across `/`, `/proc`, `/mnt`, and `/mnt/sda1` seamlessly.
- Enabled standard userland utilities (`/bin/cat` and `ls`) to traverse and read customer files on `/mnt/sda1` transparently.

### 3. PS/2 Keyboard Driver & Syscall Raw Mode
- Enhanced `keyboard.rs` with `CTRL_ACTIVE` tracking scancodes `0x1D` (make) and `0x9D` (break).
- Mapped control keys (e.g. `Ctrl+Q` -> `0x11`) in key decoding.
- Refactored `sys_read(0, buf, count)` in `syscall/mod.rs` to support non-blocking raw mode key polling and escape sequence emission:
  - Arrow keys: `\x1b[A` (Up), `\x1b[B` (Down), `\x1b[C` (Right), `\x1b[D` (Left).
  - Special keys: `\n` (Enter), `0x08` (Backspace), `\x1b` (Escape), `\t` (Tab).
  - Stored excess bytes in `STDIN_PENDING` to ensure byte-by-byte reads or multi-byte buffers receive complete escape sequences cleanly.

### 4. Ring 3 Text Inspector & Editor (`/bin/edit`)
- Built `userland/edit` as an unprivileged Ring 3 ELF64 Rust executable linked against `optios-libc`.
- Implemented file loading via `sys_open`/`sys_read`/`sys_close` into dynamic heap memory allocated via `sys_brk`.
- Rendered 160x50 terminal views with ANSI escape sequences:
  - `\x1b[2J\x1b[H` screen clearing and cursor homing.
  - Rows 1..48: Text buffer lines with empty-line `~` indicators and `\x1b[K` line clearing.
  - Row 49: High-contrast reverse-video status bar (`\x1b[7m`) showing filename, line count, cursor position, and shortcut hints.
  - Row 50: Status and message feedback bar.
- Supported arrow-key scrolling and clean exit on `Ctrl+Q` (`0x11`) or `'q'` restoring cursor and terminal state.

### 5. Automated Verification & Hybrid RAW ISO
- Formatted Partition 1 of `test_disk.img` as a compliant FAT32 filesystem containing `recovery_log.txt` via `scripts/format_gpt_disk.py`.
- Automated test sequence in `scripts/run.sh --test` verified:
  1. `ls /mnt/sda1` -> returns `recovery_log.txt` and `README.TXT`.
  2. `/bin/cat /mnt/sda1/recovery_log.txt` -> outputs recovery log contents.
  3. `/bin/edit /mnt/sda1/recovery_log.txt` -> launches Ring 3 editor and captures screendump.
  4. Exits editor back to shell with exact physical memory parity (`zero leak: true`).
- Verified hybrid RAW flashing compatibility of `optios.iso` via xorriso for direct USB pendrive installation.

## Consequences

- OptiOS is now a functional offline recovery environment capable of inspecting FAT32/ESP filesystems on SATA drives.
- The VFS architecture cleanly supports multi-filesystem mounting with zero kernel heap fragmentation and zero memory leaks.
- Users and automated diagnostics can view and navigate logs and system files in userland via `/bin/cat` and `/bin/edit`.
