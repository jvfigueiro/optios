<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: AI-Assisted Systems Engineering Experiment & Metrics
// Date: 2026-11-11
-->

# OptiOS — Systems Engineering Experiment & Metrics

## 1. Purpose and Scope

OptiOS serves both as an educational x86-64 UEFI operating system project and as a structured empirical case study in AI-assisted systems engineering. Rather than evaluating isolated code snippets or synthetic benchmarks, the repository tracks how AI coding agents perform across a multi-milestone bare-metal kernel under explicit human architectural direction, verification gates, and post-hoc technical audits.

---

## 2. Research Questions

1. **What subsystems can an AI coding agent implement from public specifications under structured human constraints?**
2. **Which classes of architectural, hardware-protocol, or security defects require human or external audit intervention to catch?**
3. **Where do agent-authored engineering reports diverge from raw hardware/emulator evidence?**
4. **What failure modes remain hidden in QEMU and only manifest on physical bare-metal hardware?**

---

## 3. Methodology

1. **Human Direction & Gated Phases**:
   - The human maintainer defines the target architecture, phase scope, licensing rules ([`AGENTS.md`](../../AGENTS.md), [`docs/licensing-policy.md`](../licensing-policy.md)), and acceptance criteria.
   - Each milestone or phase is gated: the agent must inspect existing code, implement only the authorized scope, execute host unit tests and automated QEMU verification, paste raw command outputs, commit a snapshot, and stop for human review.
2. **Decoupled Host-Testable Protocol Crates**:
   - Complex binary parsers and hardware protocol structures (`ustar`, `ahci-proto`, `partition-table`, `elf-loader`, `ansi-parser`, `opti-wm`, `fat32`, `xhci-proto`) are isolated in pure `#![no_std]` library crates under `libs/` and tested on the host (`x86_64-unknown-linux-gnu`) via `./scripts/test.sh`.
3. **End-to-End QEMU / OVMF Verification**:
   - Full system integration is verified headlessly via `./scripts/run.sh --test`, which boots `build/optios.iso` under `qemu-system-x86_64` (`pc-q35`, OVMF UEFI, `-cpu qemu64,+smep,+smap`, attached SATA AHCI disk, and `qemu-xhci` USB mouse), injects keystrokes and mouse events over the QEMU monitor socket, and asserts against the captured UART serial log.
4. **Independent Verification Audits**:
   - Periodically (such as Phase 1 after Milestone 10), functional development is paused to perform a read-only verification audit ([`docs/reports/10.1-verification.md`](../reports/10.1-verification.md)) comparing prior milestone claims line-by-line against the actual source code and runtime behavior.

---

## 4. Tracked Metrics Definitions

- **Review rounds per functional slice**: Number of human-directed iterations or fix cycles required for a phase/slice to meet its acceptance gate.
- **Defects identified per audit by class**:
  - `P0` (Security / Data Safety): Privilege boundary flaws, unchecked user pointers, missing hardware protection bits, or unintended write paths.
  - `P1` (Correctness / Protocol): Hardware specification deviations, race conditions, resource leaks, or unhandled hostile disk inputs.
  - `P2` (Hygiene / Documentation): Missing negative unit tests, version string inconsistencies, or inaccurate documentation claims.
- **Unverified or overstated report claims later corrected during audit**: Statements in milestone summaries that exceeded what the code or tests actually proved.
- **Host tests modified after initial failure (with documented justification)**: Instances where an existing test was edited, recording whether the change fixed a genuine specification bug in the test or weakened an assertion (prohibited by [`AGENTS.md`](../../AGENTS.md) Rule A.4).
- **Scope or phase-gate violations**: Instances where an agent modified files outside the authorized phase scope or proceeded past a gate without approval.
- **Hardware-only defects discovered on physical machines**: Bugs that pass under QEMU/OVMF emulation but fail on physical x86-64 hardware (currently `0` — real hardware is untested).

---

## 5. Preliminary Factual Counts (Through Phase 6 & Public-Readiness Hygiene)

The following counts are extracted directly from the repository commit history, `./scripts/test.sh`, [`docs/reports/10.1-verification.md`](../reports/10.1-verification.md), and [`docs/reports/public-readiness.md`](../reports/public-readiness.md):

| Metric Category | Factual Count | Breakdown & Source Reference |
| :--- | :---: | :--- |
| **Feature Milestones Completed** | `11` | Milestones `1` through `10` plus Milestone `5.1` hardening ([`docs/DECISIONS/`](../DECISIONS/)). |
| **Post-M10 Engineering Phases Completed** | `7` | `Phase 0` (Governance), `Phase 1` (10.1 Audit), `Phases 2–3` (P0 Fixes & Driver Kit), `Phase 4` (xHCI U1), `Phase 5` (xHCI U2), `Phase 6` (xHCI U3). |
| **Host-Testable `#![no_std]` Crates** | `8` | `ahci-proto` (11), `ansi-parser` (10), `elf-loader` (6), `fat32` (6), `partition-table` (6), `ustar` (24), `opti-wm` (5), `xhci-proto` (17) — **85 host unit tests total**. |
| **Milestone 10 Claims Audited in `10.1-verification.md`** | `12` | `6` Verified without qualification, `2` Verified with architectural caveats (#1, #6), `1` Misleading wording (#9), `1` False (#10 NVMe claim), `1` Inconsistent version string (#11), `1` Omission (#12 WM). |
| **Prioritized Defects Identified in `10.1-verification.md`** | `17` | **`P0`**: `5` items (`4` fixed in Phase 2; `1` open: `P0.3` ELF W^X).<br>**`P1`**: `7` items (`7` open; tracked in [`docs/known-issues.md`](../known-issues.md)).<br>**`P2`**: `5` items (`2` fixed in Phase 4 / H2; `3` open). |
| **Protocol Bugs Caught by QEMU Live Integration in Phases 4–6** | `4` | 1. `HCIVERSION` unaligned 16-bit MMIO read returning `v0.00` instead of `v1.00` (fixed in Phase 5 via 32-bit read at offset `0x00`).<br>2. 32-byte vs 34-byte USB Configuration Descriptor truncation omitting the 7-byte Endpoint Descriptor (fixed in Phase 6).<br>3. xHCI Input Context endpoint offset off-by-one (`ctx_size * dci` instead of `ctx_size * (dci + 1)` due to prepended Input Control Context; fixed in Phase 6).<br>4. QEMU monitor routing `mouse_move` to PS/2 Mouse `#2` instead of USB HID Mouse `#3` without `mouse_set 3` (fixed in Phase 6). |
| **Host Unit Tests Modified With Documented Justification** | `1` | `test_format_configure_ep_input_context` in [`libs/xhci-proto/src/lib.rs`](../../libs/xhci-proto/src/lib.rs): updated expected Endpoint Context 3 offset from `96` (`32 * 3`) to `128` (`32 * 4`) to match xHCI Specification Section 6.2.5 after QEMU live transfer testing demonstrated the off-by-one offset. |
| **Overstated / Inaccurate Documentation Claims Corrected in H2** | `3` | 1. ADR 0010 (`multitasking windowed graphical desktop` corrected to `multi-window graphical desktop; multitasking not implemented / planned in R5`).<br>2. ADR 0012 (`BlockDevice` listing NVMe as implemented corrected to `Planned in R4`).<br>3. `docs/posix-foundation.md` terminology aligned with `AGENTS.md`. |
| **Hardware-Only Defects on Physical Machines** | `0 (Untested)` | Zero physical hardware runs executed to date. |

---

## 6. Methodological Limitations

No sweeping generalizations about AI software engineering should be drawn from this dataset alone:

1. **Single-Project Sample Size ($N = 1$)**: All observations come from a single repository directed by a single human maintainer.
2. **Human-in-the-Loop Selection Bias**: Architectural decomposition, phase scoping, and audit prompts were authored by a human reviewer who actively steered the agent away from dead ends and specified which invariants to check.
3. **Evolving Models and Tooling**: Development occurred across changing AI model versions and agent harness configurations, meaning raw defect rates are not attributable to a single frozen model checkpoint.
4. **Emulator-Only Verification**: Passing tests in QEMU (`pc-q35` + OVMF) do not prove timing, cache-coherency, SMM/BIOS handoff, or electrical compatibility on real physical motherboards.
