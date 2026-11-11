# 4. Hardware Interrupts (PIC/PIT), PS/2 Keyboard, SMBIOS Telemetry, and Interactive Dashboard

* **Date:** 2026-11-09
* **Status:** Accepted
* **Context:** Milestone 4 real-time hardware interrupts, system timer, keyboard input, SMBIOS inspection, and interactive multi-tab UI

## Context

To transition OptiOS from a static boot-time diagnostic screen to a live interactive environment, the kernel requires asynchronous hardware interrupt handling, a deterministic system clock, user keyboard input, and firmware DMI/SMBIOS inspection. Additionally, moving the test target to the modern QEMU Q35 chipset (`-machine q35`) exposes realistic PCIe/AHCI hardware topologies.

## Decisions

1. **8259 PIC Remapping & IRQ Dispatching:**
   - Remap the Dual 8259 Programmable Interrupt Controller to IDT vectors `32..47` (`0x20..0x2F`) to eliminate vector collisions with CPU architectural exceptions (`0..31`).
   - Unmask strictly IRQ 0 (System Timer, vector 32) and IRQ 1 (PS/2 Keyboard, vector 33), issuing End-of-Interrupt (`0x20`) upon completion.
   - Ensure the Kernel Heap Spinlock is `IrqSafe` by saving `RFLAGS` and disabling interrupts (`cli`) while holding the heap lock, preventing deadlocks if an interrupt occurs during allocation.
2. **Programmable Interval Timer (PIT 8253/8254):**
   - Configure PIT Channel 0 at **100 Hz** (divisor `11931`, 10 ms tick resolution).
   - Maintain a lock-free `AtomicU64` tick counter to track real-time system uptime.
3. **PS/2 Keyboard Driver & Lock-Free Ring Buffer:**
   - Process Scancode Set 1 from port `0x60` on IRQ 1.
   - Enqueue decoded key events (`F1`..`F3`, `1`..`3`, `Enter`, `Esc`, arrow keys, and ASCII characters) into an interrupt-safe static circular buffer consumed by the main event loop.
4. **SMBIOS / DMI Firmware Parser:**
   - Request SMBIOS 32-bit/64-bit entry points via Limine's `SmbiosRequest`.
   - Parse Type 0 (BIOS Information), Type 1 (System Information), and Type 2 (Baseboard Information) structures and string tables.
5. **Interactive Multi-Tab Framebuffer Dashboard:**
   - Replace the passive halt loop with an interrupt-driven event loop (`sti; hlt`) that updates the header uptime clock and switches diagnostic views (`[1/F1] System & SMBIOS`, `[2/F2] PCI Hardware Inventory`, `[3/F3] Memory & Heap Telemetry`).

## Consequences

- OptiOS becomes a responsive, interrupt-driven live diagnostic environment.
- Full PCI bus scanning (`0..=255` with multi-function detection and BAR0..BAR5 inspection) accurately inventories modern Q35/PCIe/AHCI controllers.
- Sets the foundation for future storage drivers (AHCI/SATA) and preemptive multitasking (not implemented / planned in R5).
