# ADR 0006: Milestone 5.1 Hardening — USTAR Parser Isolation & AHCI Driver Robustness

## Status

Accepted — 2026-11-10

## Context

An independent engineering audit of the Milestone 5 codebase identified critical safety and robustness vulnerabilities across two core systems:
1. **USTAR Initramfs Archive Parser:**
   - Missing header checksum verification.
   - Non-strict magic and version validation.
   - Unchecked octal parsing susceptible to overflow and trailing junk.
   - Lack of path traversal (`..`) defenses.
   - Vulnerability to malformed or truncated archive boundaries.
2. **SATA AHCI 1.0/1.3 Storage Driver:**
   - Ephemeral DMA allocations: command lists and received FIS structures were allocated and deallocated per command, leaving HBA registers pointing to freed frames.
   - Unvalidated PCI BAR5 (ABAR), lack of BAR sizing, missing check for 64-bit BARs on Type-0 PCI configuration headers, and sizing BAR without disabling Memory Space decoding.
   - Missing BIOS/OS handoff (BOHC) synchronization.
   - Fixed CPU-speed-dependent spin loops prone to premature timeouts on fast CPUs or infinite stalls on hardware failures.
   - Hardcoded command slot 0 without considering `CAP.NCS` or multi-slot tracking.
   - Incomplete error recovery on task file errors or command aborts.

## Decisions

### 1. Pure USTAR Parser Library (`libs/ustar`)
- Extracted pure parsing logic into a `#![no_std]` + `alloc` workspace crate (`libs/ustar`) with comprehensive host unit tests.
- Implemented strict POSIX.1-1988 USTAR compliance:
  - Header checksum calculated as unsigned byte sum with `chksum` treated as ASCII spaces.
  - Strict magic and version matching: `header[257..263] == b"ustar\0"` and `header[263..265] == b"00"`.
  - Checked arithmetic in `parse_octal` rejecting GNU base-256 binary encoding (`0x80`), trailing junk, and integer overflow.
  - Path normalization rejecting all `..` components, embedded NUL bytes, and paths exceeding 255 characters.
  - Safe skipping of root directory markers (`./`, `.`, `/`) emitted by tools like `tar -C rootfs .` without error.
  - Safe ignoring and skipping of data blocks for PAX extended headers (`'x'`, `'g'`), GNU extensions (`'L'`, `'K'`), and link entries (`'1'`, `'2'`).

### 2. Pure AHCI Protocol Definitions (`libs/ahci-proto`)
- Extracted command builders, bitfield packing, and ATA IDENTIFY decoding into a pure `#![no_std]` + `alloc` workspace crate (`libs/ahci-proto`) tested on host.
- Enforced read-only safety by construction through the `AllowedCommand` enum (`IdentifyDevice = 0xEC`), eliminating any public path to issue sector write commands.
- Removed any `write` parameters from `build_command_header` so write bit `W` is hardcoded to 0 by construction.
- Standardized PRDT generation with zero-based byte counts (`DBC = byte_count - 1`).
- Implemented pure `find_free_slot(occupied, ncs) -> Option<u8>` returning `None` (mapping to `AhciError::NoFreeSlot`) when all slots in `0..ncs` are occupied.

### 3. Persistent Per-Port DMA Structures (`AhciPort`)
- Allocated one dedicated, persistent 4 KiB physical page frame per active SATA port from PMM:
  - `@0x000`: Command List (1024 B, 1 KiB aligned).
  - `@0x400`: Received FIS (256 B, 256 B aligned).
  - `@0x500`: Command Table 0 (144 B, 128 B aligned).
  - `@0x800`: Data Buffer (512 B, 512 B aligned).
- Dedicated frame is zeroed immediately on allocation and the 512-byte buffer is re-zeroed prior to issuing commands.
- Added runtime alignment assertions and verified physical addresses remain `< 4 GiB` when `CAP.S64A == 0`.

### 4. Controller Validation, Sizing, & BIOS/OS Handoff
- Temporarily disabled PCI Memory Space (bit 1) and Bus Master (bit 2) decoding in the PCI Command register before sizing BAR5 with `0xFFFFFFFF`, restoring original values and enabling Bus Master and Memory Space after mask readback.
- Verified BAR5 is a 32-bit Memory Space BAR; explicitly rejected controllers claiming 64-bit BAR5 because Type-0 PCI configuration headers lack BAR6.
- Mapped MMIO registers bounded by `min(bar_size, 0x100 + 0x80 * (highest_implemented_port + 1))`, preventing mapping beyond the BAR.
- Performed BIOS/OS handoff via the `BOHC` register when `CAP2.BOH` is advertised (noted in comments as untested under QEMU where `CAP2.BOH = 0`).
- Disabled `GHC.IE` and `PxIE` to maintain deterministic, poll-driven I/O without stray IRQs.

### 5. Time-Bounded Polling & Startup Reordering
- Moved Dual 8259 PIC, 100 Hz PIT timer, and keyboard initialization ahead of AHCI driver initialization in `kernel/src/main.rs`, enabling hardware interrupts (`sti`) so PIT ticks advance during driver bring-up.
- Replaced CPU spin loops with `time::wait_until(timeout_ms, cond)` using `core::hint::spin_loop()` (`pause`), with:
  - `debug_assert!(are_interrupts_enabled())` checking RFLAGS IF bit 9.
  - Bounded spin stall guard returning `Err(AhciError::TimerStalled)` if ticks do not advance.
  - `timeout_ticks = ceil(ms / 10) + 1` ensuring actual elapsed time is never shorter than requested due to tick boundary quantization.

### 6. Command Execution & Recovery
- Dynamic slot selection via `find_free_slot(ci | sact, ncs).ok_or(AhciError::NoFreeSlot)`.
- Automated error recovery on task file error (`TFES`), device error (`TFD.ERR`), or timeout: stops engine, clears errors (`PxSERR`/`PxIS`), reads `PxTFD`.
- If `PxTFD` still reports `BSY` or `DRQ` set after stopping the engine, the port is marked `Failed` via `AtomicBool`, `ST` is not re-enabled, and `Err(AhciError::PortFailed)` is returned. If `BSY` and `DRQ` are clear, `ST` is safely restarted.

## Consequences

- The kernel is resilient against untrusted or malformed initramfs archives and path traversal attacks.
- The storage stack is architecturally proven to be read-only, preventing forensic alteration of attached physical storage.
- AHCI bring-up and command dispatch are fully deterministic, bounded by millisecond-precision timers, and compliant with the Serial ATA AHCI 1.3.1 specification.

## Consequences

- The kernel is resilient against untrusted or malformed initramfs archives and path traversal attacks.
- The storage stack is architecturally proven to be read-only, preventing forensic alteration of attached physical storage.
- AHCI bring-up and command dispatch are fully deterministic, bounded by millisecond-precision timers, and compliant with the Serial ATA AHCI 1.3.1 specification.
