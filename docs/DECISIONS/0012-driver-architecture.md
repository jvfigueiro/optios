# ADR 0012: OptiOS Driver Kit Architecture & Foreign Driver Compatibility

- **Status**: Accepted
- **Date**: 2026-11-10
- **Author**: João Figueiró / OptiOS Core Team

## Context

As OptiOS matures from a diagnostic live environment toward an installable, general-purpose operating system, its driver ecosystem must scale cleanly without introducing fragile ad-hoc hardware access.

Key architectural challenges:
1. **Memory Safety in Hardware I/O**: Direct dereferencing of raw MMIO pointers can cause out-of-bounds access, invalid alignments, or compiler optimization elisions. MMIO must enforce range checking and volatile access by construction.
2. **DMA Coherency and Lifetime Safety**: Direct Memory Access requires physically contiguous pages below device addressing limits, uncached (PWT|PCD) page table attributes, and guaranteed deallocation on drop.
3. **Hardware Device Model**: A unified abstraction is required to represent buses (PCI, USB, ISA/Platform), driver life cycles (`probe`, `attach`, `detach`), and device discovery registries.
4. **Driver Reuse Strategy**: OptiOS requires drivers for modern peripherals (such as xHCI USB 3.0 controllers, Wi-Fi, Ethernet, and storage). Writing all drivers from scratch in Rust is impractical; however, importing Linux drivers introduces heavy GPL-3.0/Linux-internal coupling and potential license friction. FreeBSD drivers, licensed under BSD-2-Clause / BSD-3-Clause, are fully compatible with OptiOS's `GPL-2.0-or-later` license and present cleaner, stable bus/driver interfaces.

## Decisions

### 1. Strongly-Typed Hardware Abstractions (`kernel/src/drivers/kit/`)

The OptiOS Driver Kit provides three core primitives:

- **`MmioRegion`**:
  - Safe encapsulation of a memory-mapped register window (`base: u64, size: usize`).
  - Strict bounds-checking on every read/write operation (`read_u8`, `read_u16`, `read_u32`, `read_u64`, `write_*`).
  - Strict alignment validation (e.g. 4-byte alignment for 32-bit registers).
  - Volatile memory semantics (`read_volatile`, `write_volatile`) to prevent compiler reordering or dead-store elimination.
  - Sub-region derivation (`sub_region(offset, sub_size)`) allowing individual functional register blocks (such as per-port AHCI registers) to be partitioned with verified parent bounds.

- **`DmaBuffer`**:
  - Contiguous physical page allocation backed by the Physical Memory Manager (`pmm::alloc_frame`).
  - Uncached, write-through virtual memory mapping (`vmm`) with PWT (Page Write-Through) and PCD (Page Cache Disable) caching flags.
  - Physical address retrieval (`phys_addr()`) for hardware programming and virtual address retrieval (`virt_addr()`, `as_slice()`, `as_mut_slice()`) for CPU DMA buffer manipulation.
  - RAII automatic cleanup on drop.

- **`IrqHandle`**:
  - Explicit binding between hardware interrupt vectors (APIC/MSI/Legacy PIC) and driver interrupt service routines (ISRs).
  - Masking, unmasking, and acknowledgment (EOI) handled through a unified interface.

### 2. Device Model & Driver Life Cycle

Devices and drivers conform to a uniform model:

- **Bus Enumeration (`enum BusType`)**:
  - `Pci`: Peripheral Component Interconnect (configuration space access, BAR allocation, MSI/MSI-X).
  - `Usb`: Universal Serial Bus (xHCI host controllers, pipes, endpoints, descriptors).
  - `IsaPlatform`: Legacy/System-on-Chip platform devices (UART COM1, PS/2 8042 controller, ACPI/PIT).

- **Driver Trait**:
  ```rust
  pub trait Driver: Send + Sync {
      fn name(&self) -> &str;
      fn probe(&self, device: &DeviceInfo) -> ProbeResult;
      fn attach(&mut self, device: &mut DeviceInfo) -> Result<(), DriverError>;
      fn detach(&mut self, device: &mut DeviceInfo) -> Result<(), DriverError>;
      fn suspend(&mut self) -> Result<(), DriverError>;
      fn resume(&mut self) -> Result<(), DriverError>;
  }
  ```

- **Device Registry (`DeviceRegistry`)**:
  - Central inventory of discovered hardware devices and bound drivers.
  - Prevents multiple drivers from concurrently binding to conflicting MMIO/I/O port ranges.

### 3. Device Class Interfaces

High-level kernel subsystems interact with devices strictly through class traits:

- **`BlockDevice`**:
  - Sector-granular storage interface (`read_sectors`, `sector_size`, `total_sectors`).
  - Read-Only guarantee enforced by construction: write commands are excluded from `AllowedCommand`.
  - Implemented by AHCI SATA drives (`AhciPort`), NVMe controllers, and RAM disks.

- **`InputDevice`**:
  - Unified event model for human interface peripherals (`Mouse`, `Keyboard`, `Touchscreen`).
  - Emits standard `InputEvent` packets (scancodes, button clicks, relative coordinate deltas, absolute position).
  - Decoupled from physical transport (PS/2 vs USB HID vs VirtIO Input).

### 4. Foreign Driver Environment (`bsd_compat`)

To support complex hardware (such as xHCI USB 3.0 controllers and network chipsets) without compromising license compatibility or kernel safety:

1. **License Compliance**:
   - OptiOS original code is licensed under `GPL-2.0-or-later`.
   - Imported foreign drivers are selected strictly from FreeBSD (`BSD-2-Clause` / `BSD-3-Clause`), which is legally permissible under GPL-2.0.
   - All imported files retain their original author copyright headers and are documented in `THIRD_PARTY_NOTICES.md`.

2. **C Compatibility Shim (`bsd_compat`)**:
   - OptiOS provides a lightweight `#![no_std]` C-compatible shim providing the minimal FreeBSD kernel subsystem APIs:
     - `bus_alloc_resource_any`, `bus_release_resource`, `bus_space_read_*`, `bus_space_write_*` (mapped to `MmioRegion`).
     - `bus_dma_tag_create`, `bus_dmamem_alloc`, `bus_dmamap_load` (mapped to `DmaBuffer`).
     - `device_printf`, `malloc`/`free` (mapped to kernel allocator), and mutex/lock primitives.
   - Prevents foreign code from making direct unchecked pointer accesses or assumptions about host OS internals.

3. **Driver Isolation**:
   - Foreign drivers are compiled as standalone crates or static libraries linked against the `bsd_compat` shim.
   - Faults or illegal access within the driver are trapped by MMIO bounds checks and page fault handlers.

## Consequences

- All AHCI register access is refactored from raw pointers (`mmio_read32`/`mmio_write32`) to bounds-checked `MmioRegion`.
- The Driver Kit foundation is established, preparing OptiOS for USB xHCI controllers and future network drivers.
- Complete adherence to licensing rules, strict English standard, and zero-warning build criteria.
