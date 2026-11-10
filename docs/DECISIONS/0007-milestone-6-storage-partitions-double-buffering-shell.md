# ADR 0007: Milestone 6 — Storage DMA, Partitions (MBR/GPT), Double Buffering & Interactive Shell

## Status

Accepted — 2026-11-10

## Context

Following the stabilization and formal release of Milestone 5.1 (`v0.5.1`), OptiOS required expansion across three core functional tracks to advance towards a full-featured recovery environment:
1. **Storage DMA & Disk Introspection:**
   - Extending the read-only AHCI driver from device identification to sector data retrieval (`READ DMA EXT` LBA48) and drive health monitoring (`SMART READ DATA`).
   - Introducing an architectural `BlockDevice` abstraction in `libs/ahci-proto`.
2. **Partition Table Parsing:**
   - Detecting and parsing Master Boot Record (MBR) and GUID Partition Table (GPT) structures.
   - Performing robust integrity validation via standard IEEE 802.3 CRC32 checksums and UTF-16LE partition name decoding.
3. **Display Subsystem Hardening:**
   - Eliminating visual artifacts and flicker in the UEFI GOP linear framebuffer through RAM double buffering.
4. **Interactive Command-Line Diagnostics:**
   - Providing an on-screen diagnostic shell (`opti-sh`) on Tab 5 with line editing, command history, and integration with kernel subsystems (VFS, AHCI, PCI, SMBIOS, PMM, Heap).

## Decisions

### 1. Pure AHCI Protocol Expansion & `BlockDevice` Trait (`libs/ahci-proto`)
- Expanded `AllowedCommand` enum with `ReadDmaExt = 0x25` and `SmartReadData = 0xB0`. Read-only safety by construction is preserved: no write commands exist in the enum, and the PRDT write bit remains hardcoded to 0.
- Implemented `build_read_dma_ext_fis` for 48-bit LBA addressing with 16-bit sector counts.
- Implemented `build_smart_read_data_fis` with ATA S.M.A.R.T. key registers (`LBA Mid = 0x4F`, `LBA High = 0xC2`, `Feature = 0xD0`).
- Implemented `parse_smart_data` extracting critical diagnostic telemetry: Temperature, Power-On Hours, Power Cycles, and Reallocated Sector Count.
- Defined the synchronous `BlockDevice` trait:
  ```rust
  pub trait BlockDevice {
      type Error;
      fn sector_size(&self) -> u32;
      fn total_sectors(&self) -> u64;
      fn read_blocks(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), Self::Error>;
  }
  ```

### 2. Physical Page-Safe DMA Sector Reading (`AhciPort`)
- Allocated a dedicated, persistent 4 KiB physical page frame (`data_frame`) per port for DMA sector transfers, eliminating dynamic DMA heap allocations.
- Constrained sector read transfers to a maximum of 8 sectors (4096 bytes) per command transaction. Transfers spanning multiple chunks loop transparently, ensuring PRDT physical buffers never cross 4 KiB frame boundaries.
- Implemented `BlockDevice` for `AhciPort`, returning `AhciError::ReadOnlyDrive` for any write operations.

### 3. Pure Partition Table Parsing Library (`libs/partition-table`)
- Created a standalone `#![no_std]` + `alloc` workspace member crate (`libs/partition-table`) with full host unit test coverage.
- Implemented bitwise standard IEEE 802.3 CRC32 calculation.
- Implemented UTF-16LE string decoding with null-terminator trimming.
- Implemented `parse_mbr` validating the `0x55AA` signature and identifying Protective MBRs (`0xEE`).
- Implemented `parse_gpt` validating the `EFI PART` signature, revision `1.0`, header CRC32, and partition entry array CRC32. Standard UEFI Partition Type GUIDs (EFI System, Microsoft Basic Data, Linux Filesystem, Swap, OptiOS System) are mapped to human-readable strings.

### 4. RAM Double Buffering for Linear Framebuffer
- Allocated 4000 KiB (1000 contiguous 4 KiB physical frames) from PMM during video initialization. GOP dimensions 1280x800x32bpp exceed the 2 MiB kernel heap, so frames are mapped directly to a dedicated higher-half virtual address space (`BACKBUFFER_VIRT_BASE = 0xFFFF_D800_0000_0000`) via `vmm::map_page`.
- All drawing primitives (`fill_rect`, `draw_string`, `draw_pixel`) write to the cached backbuffer in system RAM.
- Added `present()` method to `Framebuffer`, performing fast 64-bit word copies from the backbuffer to the physical MMIO GOP framebuffer, eliminating visual tearing and flicker.

### 5. Interactive Diagnostic Shell (`opti-sh`) on Tab 5
- Implemented `Shell` in `kernel/src/shell/` supporting:
  - Line buffer editing with printable characters, Backspace, and Enter.
  - Command history recall via ArrowUp.
  - Scrollback terminal buffer rendering with monospace glyphs.
  - 12 diagnostic built-in commands:
    1. `help`: Command documentation and usage.
    2. `uname` / `ver`: Kernel version, target architecture, and build date.
    3. `ls [path]`: VFS directory and file enumeration.
    4. `cat <path>`: VFS file inspection.
    5. `stat <path>`: POSIX metadata inspection.
    6. `lspci`: Enumerated PCI/PCIe device inventory.
    7. `lsblk`: AHCI disks and discovered MBR/GPT partitions.
    8. `smart [port]`: S.M.A.R.T. health telemetry for AHCI drives.
    9. `free` / `meminfo`: Real-time PMM and Kernel Heap usage statistics.
    10. `smbios`: BIOS/UEFI firmware and system motherboard hardware strings.
    11. `clear`: Scrollback terminal buffer reset.
    12. `uptime`: System uptime and timer tick counter.

### 6. Keyboard IRQ 1 Routing & Multi-Tab Dashboard
- Extended `KeyEvent` with `F5` (`0x3F`).
- Added Tab 5 (`DashboardTab::InteractiveShell`) to the multi-tab interface.
- Keyboard routing policy:
  - On Tabs 1..4: `1`..`5` and `F1`..`F5` switch tabs.
  - On Tab 5: `F1`..`F5` switch tabs; alphanumeric characters and symbols are routed exclusively to `opti-sh`.

## Verification & Results

- **Unit Testing:** 41 host unit tests across `optios-ustar`, `optios-ahci-proto`, and `optios-partition-table` pass (`./scripts/test.sh`).
- **Disk Generation:** `scripts/format_gpt_disk.py` generates a 64 MiB GPT disk image containing an EFI System Partition (16 MiB) and an OptiOS Customer Data Partition (46 MiB).
- **Automated QEMU Verification:** `./scripts/run.sh --test` verifies:
  - Protective MBR and GPT header verification with CRC32 matching.
  - Identification and partition listing for both GPT partitions.
  - Double buffer allocation and presentation.
  - Tab 5 switching and execution of `lsblk` and `cat /etc/motd` via `opti-sh`.
  - Screendumps captured: `screenshot_m5_tab4.png` and `screenshot_m6_tab5_shell.png`.
- **Selftests:** `./scripts/build.sh release --features selftest && ./scripts/run.sh --test` passes all selftests, including sector 0 MBR signature `0x55AA`, sector 1 GPT signature `EFI PART`, and S.M.A.R.T. data read.
