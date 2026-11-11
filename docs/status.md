<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Subsystem Implementation Status & Verification Evidence
// Date: 2026-11-11
-->

# OptiOS — Subsystem Status Matrix

All features and subsystems are listed below with their current implementation status, QEMU test status, real hardware test status, and direct repository verification evidence. Any capability without automated or captured test evidence is marked `Unverified`.

| Subsystem / Feature | Status (Implemented / Partial / Planned) | Tested in QEMU (Yes / No) | Tested on Real Hardware (No) | Verification Evidence |
| :--- | :---: | :---: | :---: | :--- |
| **UEFI Boot Handshake (Limine Protocol)** | Implemented | Yes | No | [`kernel/src/boot/limine.rs`](../kernel/src/boot/limine.rs), [`scripts/run.sh`](../scripts/run.sh) |
| **x86-64 GDT, TSS (IST 1) & IDT Exception/IRQ Gates** | Implemented | Yes | No | [`kernel/src/arch/x86_64/gdt.rs`](../kernel/src/arch/x86_64/gdt.rs), [`kernel/src/arch/x86_64/idt.rs`](../kernel/src/arch/x86_64/idt.rs) |
| **CPU Security: CR4 SMEP/SMAP & Safe User Copy (`stac`/`clac`)** | Implemented | Yes | No | [`kernel/src/arch/x86_64/user_copy.rs`](../kernel/src/arch/x86_64/user_copy.rs), [`scripts/run.sh`](../scripts/run.sh) |
| **Physical Memory Manager (PMM Bitmap, Lower 1 MiB Reserved)** | Implemented | Yes | No | [`kernel/src/mm/pmm.rs`](../kernel/src/mm/pmm.rs), [`reports/10.1-verification.md`](reports/10.1-verification.md) |
| **Virtual Memory Manager (4-Level Paging, HHDM)** | Implemented (W^X enforced on code/data/stack/heap) | Yes | No | [`kernel/src/mm/vmm.rs`](../kernel/src/mm/vmm.rs), [`reports/closure-audit.md`](reports/closure-audit.md) |
| **Kernel Heap Allocator (`IrqSafe` Bump/Linked Allocator, 2 MiB)** | Implemented | Yes | No | [`kernel/src/mm/heap.rs`](../kernel/src/mm/heap.rs), [`locking.md`](locking.md) |
| **SMBIOS 2.8 & PCI Bus Enumeration (`0..=255`)** | Implemented | Yes | No | [`kernel/src/firmware/smbios.rs`](../kernel/src/firmware/smbios.rs), [`kernel/src/drivers/pci.rs`](../kernel/src/drivers/pci.rs) |
| **Dual 8259 PIC, PIT Timer (100 Hz), 16550 UART Serial** | Implemented | Yes | No | [`kernel/src/arch/x86_64/pic.rs`](../kernel/src/arch/x86_64/pic.rs), [`kernel/src/arch/x86_64/pit.rs`](../kernel/src/arch/x86_64/pit.rs), [`kernel/src/drivers/serial.rs`](../kernel/src/drivers/serial.rs) |
| **PS/2 Keyboard (IRQ 1) & PS/2 Mouse (IRQ 12)** | Implemented | Yes | No | [`kernel/src/drivers/keyboard.rs`](../kernel/src/drivers/keyboard.rs), [`kernel/src/drivers/mouse.rs`](../kernel/src/drivers/mouse.rs) |
| **UEFI GOP Double-Buffered Framebuffer & 160x50 ANSI Console** | Implemented | Yes | No | [`kernel/src/drivers/video/console.rs`](../kernel/src/drivers/video/console.rs), [`libs/ansi-parser/src/tests.rs`](../libs/ansi-parser/src/tests.rs) |
| **Desktop Window Manager (`optios-wm`, `gui` command)** | Implemented | Yes | No | [`libs/opti-wm/src/tests.rs`](../libs/opti-wm/src/tests.rs), [`kernel/src/gui/mod.rs`](../kernel/src/gui/mod.rs), [`images/m09-desktop-window-manager.png`](images/m09-desktop-window-manager.png) |
| **Driver Kit (`MmioRegion`, `DmaBuffer`, `InputEvent`, `/proc/drivers`)** | Partial (Core primitives & registry active) | Yes | No | [`kernel/src/drivers/kit/mod.rs`](../kernel/src/drivers/kit/mod.rs), [`scripts/run.sh`](../scripts/run.sh) |
| **SATA AHCI 1.0 Controller (Read-Only DMA, IDENTIFY, S.M.A.R.T.)** | Partial (512B sectors only; 4Kn unsupported) | Yes | No | [`kernel/src/drivers/ahci.rs`](../kernel/src/drivers/ahci.rs), [`libs/ahci-proto/src/tests.rs`](../libs/ahci-proto/src/tests.rs) |
| **MBR & GPT Partition Table Parser (CRC32 Verified)** | Partial (LBA bounds/overlap checks pending) | Yes | No | [`libs/partition-table/src/tests.rs`](../libs/partition-table/src/tests.rs) |
| **Read-Only USTAR Initramfs (`/`)** | Implemented | Yes | No | [`libs/ustar/src/tests.rs`](../libs/ustar/src/tests.rs), [`kernel/src/fs/tarfs.rs`](../kernel/src/fs/tarfs.rs) |
| **Read-Only FAT32 Filesystem (`/mnt/sda1`, `MNT_NOEXEC`, 8.3 & LFN)** | Partial (Cyclic chain & LFN checksum checks pending) | Yes | No | [`libs/fat32/src/tests.rs`](../libs/fat32/src/tests.rs), [`kernel/src/fs/vfs.rs`](../kernel/src/fs/vfs.rs) |
| **Synthetic `/proc` Filesystem (`version`, `meminfo`, `cpuinfo`, `drivers`, `usb`)** | Implemented | Yes | No | [`kernel/src/fs/proc.rs`](../kernel/src/fs/proc.rs), [`scripts/run.sh`](../scripts/run.sh) |
| **Ring 3 ELF64 Loader & Syscall Subsystem (`syscall`/`sysretq`, `sys_brk`)** | Implemented (W^X enforced, synchronous execution, POSIX syscalls) | Yes | No | [`libs/elf-loader/src/tests.rs`](../libs/elf-loader/src/tests.rs), [`kernel/src/syscall/mod.rs`](../kernel/src/syscall/mod.rs) |
| **Ring 3 Utilities (`/bin/sysinfo`, `/bin/cat`, `/bin/optifetch`, `/bin/edit`)** | Implemented | Yes | No | [`userland/`](../userland/), [`images/m10-console-fat32-xhci.png`](images/m10-console-fat32-xhci.png), [`images/m10-ring3-text-editor.png`](images/m10-ring3-text-editor.png) |
| **USB 3.0 xHCI Host Controller & Root Hub Enumeration (Slices U1–U2)** | Implemented | Yes | No | [`libs/xhci-proto/src/lib.rs`](../libs/xhci-proto/src/lib.rs), [`kernel/src/drivers/xhci.rs`](../kernel/src/drivers/xhci.rs) |
| **USB HID Boot Mouse (xHCI Slice U3 — Endpoint `0x81` DCI 3)** | Implemented | Yes | No | [`libs/xhci-proto/src/lib.rs`](../libs/xhci-proto/src/lib.rs), [`kernel/src/drivers/xhci.rs`](../kernel/src/drivers/xhci.rs), [`scripts/run.sh`](../scripts/run.sh) |
| **USB HID Boot Keyboard (xHCI Slice U4)** | Planned (Decoder in `xhci-proto`; kernel binding pending) | No | No | [`roadmap.md`](roadmap.md) (R2 Slice U4) |
| **USB Mass Storage Class / BOT (xHCI Slice U5)** | Planned | No | No | [`roadmap.md`](roadmap.md) (R2 Slice U5) |
| **Foreign Driver Compatibility Layer (`bsd_compat` / `kapi`)** | Planned | No | No | [`roadmap.md`](roadmap.md) (R3) |
| **NVMe Storage Driver, Block Cache, ext4/NTFS Read-Only** | Planned (Planned in R4) | No | No | [`roadmap.md`](roadmap.md) (R4) |
| **Preemptive Multitasking, Scheduler, Signals, Pipes & `fork`/`exec`/`wait`** | Planned (not implemented / planned in R5) | No | No | [`roadmap.md`](roadmap.md) (R5) |
| **Networking Stack & VegaUI TrueType Desktop** | Planned | No | No | [`roadmap.md`](roadmap.md) (R6, R7) |
| **Read-Write Storage Gate & System Installer** | Planned | No | No | [`roadmap.md`](roadmap.md) (R8) |
