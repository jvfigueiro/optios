# ADR 0008: Milestone 7 — Fullscreen Text Console, Ring 3 Userland, POSIX Syscalls & ELF64 Loader

- **Status**: Accepted
- **Date**: 2026-11-10
- **Author**: João Figueiró / OptiOS Core Team

## Context

Prior to Milestone 7, OptiOS presented system diagnostics via an in-kernel 5-tab graphical dashboard. While functional, housing a multi-tab GUI directly in the kernel ran counter to the Unix philosophy and POSIX architectural principles defined in `AGENTS.md`. A clean, robust operating system kernel should focus on core primitives (process isolation, memory protection, hardware abstraction, virtual filesystem) and expose userland interfaces rather than bloated in-kernel visual dashboards.

Furthermore, user applications must execute in Ring 3 (unprivileged user mode) isolated from Ring 0 (kernel mode), interacting strictly via standard POSIX system calls (`syscall`/`sysretq`).

## Decisions

### 1. Deprecation of Graphical Dashboard in Favor of Fullscreen Text Console
- Replaced the kernel multi-tab dashboard with a classical, fullscreen text console (`/dev/tty0` style) rendered directly onto the linear GOP framebuffer (160 columns x 50 rows at 1280x800 resolution with 8x16 bitmap typography).
- Maintained linear RAM double buffering (4 MiB back-buffer) with hardware-accelerated memory blits on frame commit.
- Implemented hardware/software auto-scrolling (`scroll_up`) that shifts pixel rows upward in the back-buffer and clears newly exposed lines with pure black (`#000000`).
- Synchronized all kernel log messages (`[INFO ]`, `[WARN ]`, `[ERROR]`, etc.) to both COM1 serial (115200 baud) and the video console in real-time.
- Implemented an in-memory kernel circular log buffer (`dmesg`) accessible via the shell.

### 2. Character Set & Latin-1 Glyphs
- Extended `font.rs` with custom 8x16 glyph bitmaps for `GLYPH_A_TILDE` (`ã`, `\xE3`) and `GLYPH_O_ACUTE` (`ó`, `\xF3`) to accurately render the project maintainer's name: `João Figueiró`.

### 3. AMD64 Hardware Syscall Subsystem (Ring 3 <-> Ring 0)
- Configured Model Specific Registers (MSRs) via `rdmsr` / `wrmsr`:
  - `IA32_EFER` (0xC0000080): Enabled `SCE` (System Call Extensions, bit 0).
  - `IA32_STAR` (0xC0000081): Configured Ring 0 Target CS/SS (`0x08`/`0x10`) and Ring 3 Target CS/SS (`0x20`/`0x18`).
  - `IA32_LSTAR` (0xC0000082): Registered the 64-bit kernel entry point `syscall_entry`.
  - `IA32_FMASK` (0xC0000084): Masked `RFLAGS.IF` (0x200), ensuring interrupts are disabled immediately upon entering the kernel syscall dispatcher.
- Configured TSS `rsp[0]` to point to a dedicated, page-aligned kernel syscall stack.
- Ensured segment selector alignments: User CS = `0x20 | 3 = 0x23`, User SS/DS/ES/FS/GS = `0x18 | 3 = 0x1B`.

### 4. POSIX System Call Dispatcher & Safety Boundaries
- Implemented standard AMD64 Linux/POSIX syscall numbering and ABI:
  - `SYS_READ` (0): Reading from `fd=0` (stdin via keyboard poll) or VFS nodes.
  - `SYS_WRITE` (1): Writing to `fd=1`/`fd=2` (stdout/stderr via console & serial).
  - `SYS_OPEN` (2): Opening VFS paths.
  - `SYS_CLOSE` (3): Closing open file descriptors.
  - `SYS_STAT` (4): Querying file metadata.
  - `SYS_GETPID` (39): Returns current process PID (1).
  - `SYS_EXIT` (60): Terminates user process and safely transitions execution back to kernel.
  - `SYS_UNAME` (63): Populates standard POSIX `struct utsname`.
- Enforced strict userland memory safety: All user pointer arguments are checked against `USER_SPACE_LIMIT` (`0x0000_8000_0000_0000`), immediately returning `-EFAULT` (`-14`) on kernel address overlap.
- Re-enabled interrupts (`STI`) upon returning to kernel execution context from `sys_exit`.

### 5. Memory Management (VMM) User-Space Support
- Updated `vmm::map_page` and table walkers to propagate the `USER_ACCESSIBLE` (bit 2) flag through PML4, PDPT, PD, and PT levels whenever mapping userland memory.

### 6. Pure `#![no_std]` ELF64 Loader (`libs/elf-loader`)
- Extracted ELF parsing into a modular library crate `libs/elf-loader` with zero kernel dependencies.
- Validates 64-bit ELF magic (`\x7fELF`), AMD64 machine type (`0x3E`), `PT_LOAD` segments, memory alignment, and strictly enforces userland address boundaries (`< 0x0000_8000_0000_0000`).
- Covered with 6 host unit tests testing valid binaries, truncated headers, corrupt magic, wrong architecture, filesz > memsz, and kernel space overlap rejection.

### 7. Standalone Userland Utility (`userland/sysinfo`)
- Authored a real `#![no_std]` Rust binary linked with `userland/linker.ld` at virtual base `0x0040_0000`.
- Executes in unprivileged Ring 3 and issues `sys_write`, `sys_getpid`, `sys_uname`, and `sys_exit`.
- Packaged as `/bin/sysinfo` in `/boot/initramfs.tar`.

## Consequences
- The kernel is stripped of unnecessary GUI rendering logic, resulting in faster boots, lower memory usage, and alignment with Unix/POSIX principles.
- User programs can now be compiled independently from the kernel and executed safely in Ring 3.
- Automated host test suite expanded to 47 passing tests across 4 libraries.
