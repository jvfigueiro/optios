<!--
// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: System Architecture Specification
// Date: 2026-11-10
-->

# OptiOS — System Architecture

## 1. Overview

OptiOS is an independent x86-64 operating system written in Rust (`no_std`), designed initially as a live environment for computer hardware diagnostics and system recovery booting exclusively via UEFI.

The project adheres to Unix-like architecture principles and aims for progressive POSIX compatibility for system call interfaces and user applications, while retaining complete independence from Linux or any other existing kernel codebase.

## 2. Kernel Layers

The kernel is structured in modular layers with strict boundaries:

```
+--------------------------------------------------------+
|                     User Space                         |
|     (Unprivileged Ring 3: /bin/optifetch, /bin/cat,    |
|      /bin/sysinfo, libs/opti-libc dynamic brk heap)    |
+--------------------------------------------------------+
                           |
                           v  (POSIX System Calls: SYS_READ, WRITE, OPEN, CLOSE, STAT, LSEEK, BRK, GETPID, EXIT, UNAME)
+--------------------------------------------------------+
|             Interactive Shell & Terminal               |
|     - opti-sh: Prompt 'optios - # ', argc/argv exec    |
|     - Fullscreen Text Console (/dev/tty0 Style)        |
|     - ANSI / VT100 SGR & Cursor Parser (libs/ansi)     |
|     - Hardware / Software Video Scrolling & dmesg      |
+--------------------------------------------------------+
|              POSIX & Virtual File System               |
|     - POSIX.1-2024 Errno & KernelResult<T>             |
|     - Virtual File System (vfs::open/read/stat/ls)     |
|     - Virtual Telemetry Filesystem (/proc)             |
|     - USTAR Initramfs Driver (libs/ustar at /)         |
|     - ELF64 Binary Loader (libs/elf-loader)            |
|     - User FD Table (3..16) & sys_lseek offsets        |
+--------------------------------------------------------+
|             Storage & Partition Management             |
|     - BlockDevice Trait (libs/ahci-proto)              |
|     - MBR & GPT Pure Parsers (libs/partition-table)    |
|     - SATA AHCI DMA Sector Reads (READ DMA EXT)        |
|     - S.M.A.R.T. Health Telemetry (SMART READ DATA)    |
+--------------------------------------------------------+
|             Memory Management (mm)                     |
|     - Physical Memory Manager (PMM - Bitmap)           |
|     - Virtual Memory / 4-Level Paging (VMM + MMIO)     |
|     - User-Accessible Page Table Propagation           |
|     - Dynamic User Heap (sys_brk 0x0080_0000..16 MiB)  |
|     - Process Memory Tracker & Zero-Leak Reclamation   |
|     - IrqSafe Kernel Heap Allocator (Rust alloc)       |
|     - Video RAM Backbuffer (0xFFFF_D800_0000_0000)     |
+--------------------------------------------------------+
|            Firmware & Hardware Discovery               |
|     - SMBIOS / DMI 2.x & 3.x Structure Parser          |
|     - PCI / PCIe Configuration Space Scanner (0..255)  |
+--------------------------------------------------------+
|                  Drivers & I/O                         |
|     - Serial Console (16550 UART)                      |
|     - Framebuffer GOP with RAM Double Buffering        |
|     - PS/2 Keyboard (IRQ 1 / Lock-Free Ring Buffer)    |
|     - SATA AHCI 1.0/1.3 Read-Only Storage Driver       |
+--------------------------------------------------------+
|              Architecture Abstraction                  |
|     - arch/x86_64: I/O Ports, CPUID, GDT/TSS, IDT      |
|     - arch/x86_64: MSRs (IA32_EFER, STAR, LSTAR, FMASK)|
|     - arch/x86_64: Dual 8259 PIC & 100 Hz PIT Timer    |
|     - boot/: Limine Protocol Interface & Parsing       |
+--------------------------------------------------------+
```

## 3. Boot and Handshake Protocol

1. UEFI firmware invokes the Limine bootloader (`BOOTX64.EFI`).
2. Limine reads configuration from `limine.conf`.
3. Limine validates the kernel ELF64 executable and locates the `.limine_requests` section.
4. Limine:
   - Configures initial 64-bit Long Mode paging with the kernel located in the Higher Half (`0xffffffff80000000`);
   - Maps physical memory with a virtual higher-half offset (HHDM - Higher Half Direct Map);
   - Queries and sets up the display mode via UEFI GOP;
   - Fills response pointers in the kernel request structures (Framebuffer, Memory Map, HHDM, SMBIOS);
   - Jumps to the entry point `_start`.
5. The OptiOS kernel takes control:
   - Initializes the 16550 UART serial port (COM1) for early telemetry;
   - Sets up dedicated kernel GDT and TSS with IST 1 for stack isolation during Double Faults;
   - Loads the IDT with exception dispatchers (`#DE`, `#UD`, `#DF`, `#GP`, `#PF`) and hardware IRQ gates (`32`, `33`);
   - Parses the physical memory map and initializes the Physical Memory Manager (PMM) with low-memory (`< 1 MiB`) reservation;
   - Initializes the 4-Level Virtual Memory Manager (VMM) and maps the 2 MiB `IrqSafe` Kernel Heap (`0xFFFF_C000_0000_0000`);
   - Maps and inspects SMBIOS / DMI firmware tables (Types 0, 1, 2) and enumerates all PCI/PCIe buses (`0..=255`);
   - Mounts the USTAR initramfs archive at `/` via `libs/ustar`;
   - Remaps the Dual 8259 PIC to vectors `32..47`, configures the PIT Channel 0 timer at 100 Hz, and initializes the PS/2 keyboard driver;
   - Initializes the SATA AHCI controller, discovers attached drives, executes ATA `IDENTIFY DEVICE`, reads S.M.A.R.T. health data, reads LBA 0/1 to parse Protective MBR and GPT partitions via `libs/partition-table`;
   - Allocates 4000 KiB from PMM for the Framebuffer backbuffer, enabling tear-free RAM double buffering;
   - Initializes the Fullscreen Classical Text Console (/dev/tty0 style, 160x50 at 1280x800);
   - Configures AMD64 Syscall MSRs (IA32_EFER, STAR, LSTAR, FMASK) and TSS dedicated kernel stack;
   - Initializes the `opti-sh` interactive diagnostic shell with MOTD banner;
   - Enters the fullscreen console interactive event loop and waits for keyboard interrupts (`sti` + `hlt`).

## 4. Debugging and Logging Strategy

- **Serial Console (COM1 - 0x3F8):** Low-level hardware telemetry channel (115200 baud 8N1). Operates without heap allocations or GPU drivers.
- **Fullscreen Framebuffer Console:** Primary text terminal with 160x50 character grid, automatic line scrolling, and `dmesg` ring buffer.
- **QEMU Telemetry:** Serial redirected to file/stdio; automated keystroke injection and screendump verification.
