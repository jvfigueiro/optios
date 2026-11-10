# ADR 0009: Milestone 8 — POSIX Userland, sys_brk, /proc & optifetch

- **Status**: Accepted
- **Date**: 2026-11-10
- **Author**: João Figueiró / OptiOS Core Team

## Context

Following Milestone 7's establishment of the fullscreen text console, hardware syscall subsystem, and Ring 3 execution, OptiOS required a robust userland environment. Running real programs requires:
1. Dynamic heap memory allocation in userland (`sys_brk`).
2. Complete process memory cleanup on `sys_exit` to prevent physical frame leaks.
3. System V AMD64 ABI compliant command-line argument passing (`argc`/`argv`).
4. File descriptor tracking and random access seeking (`sys_lseek`).
5. Rich virtual system telemetry (`/proc`) adhering to standard Unix conventions.
6. ANSI/VT100 escape sequence processing for formatted CLI rendering.
7. A modern userland diagnostic utility (`optifetch`) and standard file viewer (`cat`).

## Decisions

### 1. Pure `#![no_std]` ANSI Escape Sequence Parser (`libs/ansi-parser`)
- Extracted ANSI parsing into an independent host-testable library crate `libs/ansi-parser`.
- Implemented a robust state machine (`Ground`, `Escape`, `Csi`, `CsiParam`) that handles:
  - SGR (Select Graphic Rendition): Reset (0), Bold/Bright (1), Foreground (30..37 standard, 90..97 bright), Background (40..47 standard, 100..107 bright).
  - Clear Screen (`\x1b[2J`), Clear Line (`\x1b[2K`, `\x1b[K`).
  - Cursor Positioning (`\x1b[H`, `\x1b[row;colH`).
- Gracefully ignores unsupported or malformed escape sequences without dropping subsequent printable text.
- Validated with 10 pure host unit tests.

### 2. Fullscreen Console ANSI Integration (`kernel/src/drivers/video/console.rs`)
- Integrated `AnsiParser` into `ConsoleState` in the video driver.
- Supports colored output for kernel logs and userland applications.
- Text rendered using the 16-color ANSI palette mapped to 32-bit linear RGB framebuffers.

### 3. Dynamic Heap Allocation (`sys_brk` - 12) & Zero-Leak Reclamation
- Extended syscall dispatcher with `SYS_BRK` (12):
  - Programs manage heap break between `0x0080_0000` (`USER_HEAP_START`) and `0x0180_0000` (16 MiB max).
  - Kernel dynamically allocates 4 KiB frames from PMM, zero-initializes them to prevent information leakage, and maps them with `PRESENT | WRITABLE | USER_ACCESSIBLE | NO_EXECUTE`.
- Implemented `ProcessMemory` tracker in `kernel/src/syscall/loader.rs`:
  - Tracks all loaded segment pages, stack pages, and heap pages.
  - On `sys_exit`, `clean_current_process()` unmaps all user pages from VMM and returns physical frames to PMM via `free_frame`.
  - Kernel and shell selftests verify zero memory leaks (`before == after`).

### 4. POSIX System V AMD64 ABI Argument Passing (`argc` / `argv`)
- Implemented standard stack layout at `USER_STACK_TOP` (`0x0000_7FFF_FFFF_0000`):
  - String table storing NUL-terminated argument strings.
  - 16-byte aligned pointer array `argv[0..argc]` ending with a NULL sentinel pointer.
  - Arguments passed in registers according to AMD64 ABI: `RDI = argc`, `RSI = argv`.

### 5. File Descriptor Management & `SYS_LSEEK` (8)
- Added `USER_FD_TABLE` mapping process file descriptors (`3..16`) to open VFS node handles and tracking read offsets.
- Implemented `sys_lseek` (8) with `SEEK_SET` (0), `SEEK_CUR` (1), and `SEEK_END` (2).
- Automatically resets and closes all user file descriptors on process exit.

### 6. Virtual Telemetry Pseudo-Filesystem (`/proc`)
- Created `kernel/src/fs/proc.rs` providing synthetic read-only files generated on the fly:
  - `/proc/cpuinfo`: CPU vendor, model name, feature flags (SSE, AVX, etc.).
  - `/proc/meminfo`: Physical and kernel heap memory statistics in standard format.
  - `/proc/uptime`: System uptime in seconds and centiseconds, plus PIT tick counts.
  - `/proc/pci`: Enumerated PCI devices with bus, device, function, vendor, device ID, class, and BAR info.
  - `/proc/disks`: Detected AHCI SATA drives, GPT partitions, and S.M.A.R.T. health telemetry.
- Integrated seamlessly into VFS `open`, `read`, `stat`, and `list_dir`.

### 7. Userland Standard Library Runtime (`libs/opti-libc`)
- Created `#![no_std]` userland support crate providing:
  - Direct syscall wrappers (`sys_read`, `sys_write`, `sys_open`, `sys_close`, `sys_lseek`, `sys_brk`, `sys_getpid`, `sys_exit`, `sys_uname`).
  - `OptiAllocator`: Standard `GlobalAlloc` implementation on top of `sys_brk`, enabling `extern crate alloc` and standard Rust types (`Vec`, `String`, `format!`).
  - Standard I/O macros: `print!`, `println!`, and `Stdout`.
  - Argument parsing helper: `extract_args(argc, argv) -> Vec<&'static str>`.

### 8. Userland Utilities: `/bin/cat` and `/bin/optifetch`
- Implemented `/bin/cat`:
  - Ring 3 executable accepting command-line arguments.
  - Reads and displays files from VFS or `/proc` to standard output.
- Implemented `/bin/optifetch`:
  - Ring 3 fastfetch-style system information tool with stylized ASCII logo.
  - Displays OS, Kernel, Uptime, CPU, Motherboard, Memory, Storage, Terminal, and ANSI color palette.
  - Reads data directly from `/etc/optios-release` and `/proc/*` via POSIX syscalls.

## Consequences

- Full POSIX userland environment operational in Ring 3 with dynamic memory allocation, argument passing, and clean resource reclamation.
- Complete visual overhaul of console with rich ANSI color formatting.
- Total test coverage expanded to 57 host unit tests plus end-to-end QEMU automated test suite.
