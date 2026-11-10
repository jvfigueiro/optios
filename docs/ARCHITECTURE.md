# OptiOS — System Architecture

## 1. Overview

OptiOS is an independent x86-64 operating system written in Rust (`no_std`), designed initially as a live environment for computer hardware diagnostics and system recovery booting exclusively via UEFI.

The project adheres to Unix-like architecture principles and aims for progressive POSIX compatibility for system call interfaces and user applications, while retaining complete independence from Linux or any other existing kernel codebase.

## 2. Kernel Layers

The kernel is structured in modular layers with strict boundaries:

```
+--------------------------------------------------------+
|                     User Space                         |
|      (Diagnostic Tools, GUI, Modular Applications)     |
+--------------------------------------------------------+
                           |
                           v  (Future Syscall ABI: System V AMD64)
+--------------------------------------------------------+
|                      Syscalls                          |
+--------------------------------------------------------+
|  VFS / Filesystems  |  Process / Scheduler | Networking|
+--------------------------------------------------------+
|              POSIX & Virtual File System               |
|        - POSIX.1-2024 Errno & KernelResult<T>          |
|        - Virtual File System (vfs::open/read/stat/ls)  |
|        - USTAR Initramfs Driver (tarfs mounted at /)   |
+--------------------------------------------------------+
|             Memory Management (mm)                     |
|        - Physical Memory Manager (PMM - Bitmap)        |
|        - Virtual Memory / 4-Level Paging (VMM + MMIO)  |
|        - IrqSafe Kernel Heap Allocator (Rust alloc)    |
+--------------------------------------------------------+
|            Firmware & Hardware Discovery               |
|        - SMBIOS / DMI 2.x & 3.x Structure Parser       |
|        - PCI / PCIe Configuration Space Scanner (0..255)|
+--------------------------------------------------------+
|                  Drivers & I/O                         |
|        - Serial Console (16550 UART)                   |
|        - Framebuffer (UEFI GOP / Bitmap Text Engine)   |
|        - PS/2 Keyboard (IRQ 1 / Lock-Free Ring Buffer) |
|        - SATA AHCI 1.0 Read-Only Driver + S.M.A.R.T.   |
+--------------------------------------------------------+
|              Architecture Abstraction                  |
|        - arch/x86_64: I/O Ports, CPUID, GDT/TSS, IDT   |
|        - arch/x86_64: Dual 8259 PIC & 100 Hz PIT Timer |
|        - boot/: Limine Protocol Interface & Parsing    |
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
   - Remaps the Dual 8259 PIC to vectors `32..47`, configures the PIT Channel 0 timer at 100 Hz, and initializes the PS/2 keyboard driver;
   - Renders the multi-tab interactive diagnostic dashboard and enters the interrupt-driven (`sti` + `hlt`) event loop.

## 4. Debugging and Logging Strategy

- **Serial Console (COM1 - 0x3F8):** Primary low-level telemetry channel. Operates without heap allocations or GPU drivers.
- **Framebuffer:** Primary user visual diagnostic console.
- **QEMU Telemetry:** Serial redirected to file/stdio; automated screendump verification.
