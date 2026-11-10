# ADR 0010: Milestone 9 — PS/2 Mouse, Window Compositor & Desktop Environment

- **Status**: Accepted
- **Date**: 2026-11-10
- **Author**: João Figueiró / OptiOS Core Team

## Context

Following Milestone 8's establishment of the POSIX userland environment, dynamic `sys_brk` heap, `/proc` telemetry, and ANSI terminal emulation, OptiOS required a clean, independent graphical windowing environment that can be launched on-demand from the shell (`gui` / `desktop`), analogous to Unix `startx`.

A robust GUI foundation requires:
1. Low-level interrupt-driven pointer input via the Intel 8042 PS/2 controller auxiliary port (IRQ 12).
2. A decoupled, pure `#![no_std]` window geometry, z-order, and hit-testing engine (`libs/opti-wm`) verifiable on host.
3. Tear-free double-buffered compositing on the linear GOP framebuffer (1280x800x32bpp) with a high-contrast cursor sprite.
4. Seamless round-trip transitions between the fullscreen text console and the desktop environment without memory leaks or state corruption.
5. Absolute cleanliness of the terminal console (logs routed to COM1/dmesg only after boot) and authentic UTF-8 decoding for proper display of `João Figueiró`.

## Decisions

### 1. Terminal Cleanliness & UTF-8 Decoder
- Separated kernel logging: Once `opti-sh` starts, internal logs (`info!`, `warn!`) route exclusively to COM1 serial and the in-memory `dmesg` ring buffer, preventing terminal screen pollution.
- Implemented a 2-byte UTF-8 decoding state machine in `console.rs` that maps `\xC3\xA3` -> `ã` (`GLYPH_A_TILDE`) and `\xC3\xB3` -> `ó` (`GLYPH_O_ACUTE`), accurately rendering `João Figueiró` in the MOTD and terminal prompts.

### 2. Pre-Allocation of Userland Intermediate Paging Structures
- Added `vmm::preallocate_user_tables` called during `syscall::init()`.
- Intermediate page directory tables (PDPT, PD, PT) covering user code (`0x0040_0000`), heap (`0x0080_0000..0x0180_0000`), and stack (`0x0000_7FFF_FFFF_0000`) are allocated at boot.
- Result: Runtime process execution achieves exact physical frame parity (`free_before == free_after`, delta = 0) from the very first invocation (`zero leak: true`).

### 3. PS/2 Mouse Driver (IRQ 12 / Vector 44)
- Configured Intel 8042 controller auxiliary device via command `0xA8`, updated the Compaq Status Byte (enabling IRQ 12 bit 1), and issued `0xF4` (Enable Data Reporting).
- Unmasked IRQ 12 on the Slave PIC and IRQ 2 on the Master PIC.
- Registered vector 44 in the IDT, dispatching to `mouse::on_irq12()` and properly acknowledging End-Of-Interrupt to both PIC controllers.
- Processed 3-byte packets with 9-bit signed coordinate delta computation, inverted Y-axis alignment, and screen boundary clamping (0..1280, 0..800).

### 4. Pure Window Geometry & Hit-Testing Manager (`libs/opti-wm`)
- Created `#![no_std]` crate `libs/opti-wm` with zero kernel dependencies.
- Implemented `Window` and `WindowManager` tracking positions, dimensions, z-order, minimize/focus states, and dragging.
- Implemented hierarchical hit-testing (`HitTarget::TitleBar`, `HitTarget::CloseButton`, `HitTarget::Body`, `HitTarget::Desktop`).
- Clamps dragged windows to ensure titlebars always remain on-screen and accessible.
- Validated with 5 pure host unit tests (totaling 62 host tests in `./scripts/test.sh`).

### 5. Desktop Environment (`gui` / `desktop`) & Compositor
- Integrated `gui` command in `opti-sh`.
- Top System Bar: Displays OptiOS logo, live Uptime, RAM usage, and `[ESC / Exit to Console]` button.
- Bottom Dock / Taskbar: Launch and focus buttons for native diagnostic Mini Apps.
- 4 Native Diagnostic Windows:
  1. **Diagnostic Terminal (`opti-sh`):** Interactive shell viewport inside GUI.
  2. **Hardware & SMBIOS Inspector:** CPUID brand, CPU flags, motherboard, BIOS, and PCI devices.
  3. **Storage & S.M.A.R.T. Manager:** SATA AHCI drives, GPT partition tables, and S.M.A.R.T. health metrics.
  4. **Memory & PMM Monitor:** Visual graphical progress bars for physical PMM frame allocation and kernel virtual heap.
- High-Contrast Cursor: 12x19 pixel arrow sprite with 1px black contrast border and bright white interior, rendered on top of the composition.
- Smooth Exit: Pressing ESC or clicking `[ESC / Exit to Console]` clears the backbuffer and restores the fullscreen text console.

## Consequences

- OptiOS now possesses a dual interface: a classical, high-contrast POSIX text console by default, and a full multitasking windowed graphical desktop on demand.
- Host test coverage expanded to 62 unit tests across 6 independent library crates.
- Verified end-to-end under QEMU with automated keyboard and mouse injection.
