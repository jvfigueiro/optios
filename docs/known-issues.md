<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Known Technical Limitations & Open Audit Items
// Date: 2026-11-11
-->

# OptiOS — Known Issues & Architectural Limitations

This document records the current operational boundaries of OptiOS and every open technical item identified in the Milestone 10 verification audit ([`docs/reports/10.1-verification.md`](reports/10.1-verification.md)).

---

## 1. Major Operational Limitations

1. **Untested on Real Physical Hardware (QEMU Only)**:
   - All boot, storage, memory, and USB tests have been executed exclusively in QEMU (`pc-q35` + Tianocore EDK II OVMF). Zero tests have been run on physical bare-metal machines.
2. **No USB Keyboard Input Yet (xHCI Slice U4 Pending)**:
   - While the native USB 3.0 xHCI driver enumerates devices and ingests live **USB HID Boot Mouse** reports (Slice U3), **USB HID Keyboard support is not implemented in the kernel yet** (Slice U4).
   - Console and shell keyboard input currently depend on the legacy 8042 PS/2 keyboard controller (`IRQ 1`). On physical UEFI machines without PS/2 hardware or legacy emulation after `ExitBootServices()`, keyboard input will not function until xHCI Slice U4 is completed.
3. **Single-Process Synchronous Userland Execution**:
   - Userland ELF64 binaries (`/bin/sysinfo`, `/bin/cat`, `/bin/optifetch`, `/bin/edit`) execute synchronously one at a time while the kernel shell waits for `sys_exit`.
   - Preemptive multitasking, per-process `CR3` page tables, `fork()`, `wait4()`, and IPC pipes are **not implemented / planned in R5**.
4. **No NVMe Storage Driver**:
   - PCI enumeration recognizes NVMe class codes (`01:08`), but only SATA AHCI (`01:06:01`) has an active driver. Native NVMe support is **Planned in R4**.

---

## 2. Audit Items Status (`docs/reports/10.1-verification.md` & `docs/reports/closure-audit.md`)

All Priority 0 (P0) security and memory protection items from `10.1-verification.md` have been **100% resolved and verified** (see [`docs/reports/closure-audit.md`](reports/closure-audit.md)):
- **P0.1 (CR4.SMEP/SMAP)**: Resolved in Phase 2.
- **P0.2 (VFS noexec Policy)**: Resolved in Phase 2.
- **P0.3 (W^X Enforcement on ELF Segments & User Stack, `sys_stat`)**: Resolved in Closure Task H10.
- **P0.4 (Per-Process File Descriptors)**: Resolved in Phase 2.
- **P0.5 (Kernel Memory Frame Allocation Accounting)**: Resolved in Phase 2.

Additionally:
- **P2.3 (Version String Divergence)**: Resolved in Task H10 (`OptiOS 0.10.4-alpha`).
- **Asset-1 (`font_data.in` Provenance)**: Resolved in Task H10 (replaced with clean public-domain IBM VGA 8x16 font).

The non-blocking Priority 1 and Priority 2 items below remain tracked on the engineering roadmap:

### Priority 1 — Correctness & Protocol Hardening (`P1`)

| ID | Issue Summary | Current Behavior & Risk | Target Resolution |
| :--- | :--- | :--- | :--- |
| **P1.1** | **Missing Automated AHCI Port 2 Abort & Second IDENTIFY Selftest** | `ahci.rs` restores `FRE` and `ST` on error recovery, but lacks an automated boot selftest that forces an abort on Port 2 and verifies a subsequent IDENTIFY command. | Add a deterministic Port 2 abort recovery test under the `selftest` feature flag. |
| **P1.2** | **FAT32 Cyclic Cluster Chain & LFN Checksum Validation** | `libs/fat32` caps cluster chain traversal at 65,536 clusters to prevent infinite loops, but returns `Ok(chain)` instead of `Err(Fat32Error::CyclicClusterChain)`. `next_cluster` does not verify `cluster < total_data_clusters + 2`, and LFN sequence contiguity / 8.3 checksum (offset 13) are not validated. | Return `Err(Fat32Error::CyclicClusterChain)` on loop cap, validate cluster upper bounds against BPB cluster count, and verify LFN checksums (Roadmap R4). |
| **P1.3** | **Hardcoded 512-Byte Sector Size (No 4Kn Disk Support)** | `libs/fat32`, `libs/partition-table`, and `kernel/src/drivers/ahci.rs` assume 512-byte logical sectors. Native 4096-byte sector (4Kn) disks are rejected. | Parameterize logical sector size from ATA IDENTIFY words 106/117 across `BlockDevice`, GPT/MBR, and FAT32 (Roadmap R4). |
| **P1.4** | **GPT Partition LBA Bounds & Overflow Checks** | `libs/partition-table` validates header and partition array CRC32, but does not use `checked_mul` for `num_entries * entry_size`, does not verify `start_lba <= end_lba` within `first_usable_lba..=last_usable_lba`, and does not reject overlapping partitions. | Add checked arithmetic and partition LBA range/overlap validation in `libs/partition-table` (Roadmap R4). |
| **P1.5** | **PMM Page-Table Frame Leak Outside Pre-Allocated User Ranges** | `vmm::preallocate_user_tables` pre-allocates intermediate PDPT/PD/PT frames for fixed userland ranges (`0x400000..0x1600000` and user stack). If an ELF binary maps segments outside those virtual ranges, new intermediate page table frames are allocated on demand and are not freed in `clean_current_process()`. | Introduce per-process `CR3` address spaces and recursive page-table teardown on process exit (Roadmap R5). |
| **P1.6** | **QEMU Boot Order with Attached Bootable SATA ESP** | If a test SATA disk contains a valid `EFI/BOOT/BOOTX64.EFI` executable, OVMF may prioritize SATA Port 0 over the CD-ROM ISO unless explicit `bootindex` is passed. | Pass explicit `bootindex` flags in `scripts/run.sh`. |
| **P1.7** | **PS/2 Mouse Lock Not IRQ-Safe (`MOUSE_LOCK`)** | In `kernel/src/drivers/mouse.rs`, `on_irq12()` updates `CURRENT_MOUSE` in interrupt context without acquiring `MOUSE_LOCK`, and `get_state()` acquires `MOUSE_LOCK` without disabling interrupts (`cli`). | Replace `MOUSE_LOCK` with an IRQ-safe spinlock (`cli`/`sti` RFLAGS preservation) or atomic state snapshot. |

### Priority 2 — Testing & Versioning Hygiene (`P2`)

| ID | Issue Summary | Current Behavior & Risk | Target Resolution |
| :--- | :--- | :--- | :--- |
| **P2.1** | **Host Unit Test for `AllowedCommand` Write Opcode Rejection** | `AllowedCommand` in `libs/ahci-proto` prevents constructing write commands in safe Rust, but lacks a dedicated unit test asserting write opcodes (`0x30`, `0x34`, `0x35`) are rejected. | Add an explicit unit test in `libs/ahci-proto/src/tests.rs`. |
| **P2.4** | **Historical Milestone 5 Errata Note** | Earlier Milestone 5 notes are captured in [`docs/DECISIONS/0006-milestone-5-1-hardening-and-ahci-robustness.md`](DECISIONS/0006-milestone-5-1-hardening-and-ahci-robustness.md) rather than a standalone `docs/reports/` file. | Documented in ADR 0006 and [`docs/reports/10.1-verification.md`](reports/10.1-verification.md). |
