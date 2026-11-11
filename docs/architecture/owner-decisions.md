# OptiOS Architecture — Owner Decisions

Owner to verify licensing statements with qualified counsel before any distribution.

## B1. Strategic Goal

OptiOS starts as a portable maintenance/diagnostic live system and must be able to scale into an installable general-purpose OS. Reusing drivers from other systems with little or no adaptation is a strategic requirement, so licensing and architecture are chosen for it.

## B3. Driver Architecture Principles (Design Target)

- **Driver Kit**: Narrow typed interface (`PciDevice`, `MmioRegion`, `DmaBuffer`, `IrqHandle`, timers, log/telemetry). Native drivers use ONLY this, never `pmm`/`vmm` directly.
- **Device model**: Buses (PCI, later USB/ACPI), `Driver` trait (match table, probe/attach/detach/suspend/resume/shutdown), a registry carrying each driver's SPDX and origin (`Native` | `Imported(source)`).
- **Class interfaces**: `BlockDevice` read-only in the type system, with a SEPARATE write trait granted only by an "installed" profile; `InputDevice` (source-agnostic `InputEvent`); `NetDevice`; `Display`; a `Diagnosable` trait (counters, register dump, health) implemented by every driver.
- **Foreign Driver Environment for reuse with minimal change**: A stable C ABI ("kapi") exported by the kernel plus compat headers. `bsd_compat` FIRST (`bus_space`, `bus_dma`, `mutex`, `callout`, `softintr`, `malloc`); `linuxkpi-lite` only if justified later. C drivers statically linked first, loadable modules later.
- **Native vs port**: Write natively against public specs (AHCI, xHCI, HID, and Planned in R4/R7: NVMe, HDA); port vendor-specific complex hardware (Ethernet NICs, Wi-Fi); GPU acceleration out of scope (GOP, later `virtio-gpu`).
- **Safety**: Bounded waits only; IRQ-safe locks (no ISR may take a lock that is not IRQ-safe); whitelisted request types for untrusted devices; hostile-input handling in every parser; DMA constraints and an IOMMU hook.
- **Testability**: Pure `libs/*-proto` crates and an `Mmio` trait with a simulated register file for host tests.
- **Driver placement**: Where drivers run (monolithic vs microkernel) stays open: the Driver Kit must be capability-style so it can later be exported to userland.
