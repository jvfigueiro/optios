# OptiOS Roadmap

Owner to verify licensing statements with qualified counsel before any distribution.

The order of roadmap items is authoritative. Do not reorder or start items without approval; each item is split into slices of 5.1-phase size with acceptance criteria.

- **R0 Stabilize**: Phases 0-2 (Governance/licensing, verification audit, approved fixes, doc accuracy).
- **R1 Driver architecture**: ADR, Driver Kit skeleton, InputEvent abstraction, AHCI/PS2/mouse/framebuffer/serial refitted as conformance cases.
- **R2 USB**: xHCI (init, enumeration, HID boot keyboard/mouse, hubs), whitelisted control requests; EHCI/UHCI/OHCI only on demand.
- **R3 Platform**: ACPI tables (MADT/MCFG/FADT), IOAPIC/LAPIC, MSI/MSI-X, calibrated timers, SMP bring-up; decision on an AML interpreter.
- **R4 Storage/FS**: NVMe (native), block cache, FAT32 hardening, 4Kn sector-size correctness, ext4/NTFS read-only.
- **R5 Process model**: preemptive scheduler with IRQ-safe locks, per-process fd table, pipes, TTY line discipline, posix_spawn, libc/runtime decision.
- **R6 Foreign-driver pilot**: bsd_compat shim + one NIC from a permissively licensed BSD source (e.g. the Intel 82574L, 8086:10d3, already present in QEMU), then a network stack.
- **R7 Installed profile**: write capability, journaling/CoW FS writes, installer, bootloader management, users/permissions, power management.
- **R8 Graphics/UI**: userland compositor, PAT write-combining, fonts, virtio-gpu; Wi-Fi via port only if product scope requires.
