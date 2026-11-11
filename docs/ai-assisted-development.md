<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: AI-Assisted Development Disclosure & Provenance Rules
// Date: 2026-11-11
-->

# AI-Assisted Development & Code Provenance

Owner to verify licensing statements with qualified counsel before any distribution.

---

## 1. Disclosure of AI Assistance

OptiOS is built using an AI-assisted systems engineering workflow:
- **Human Maintainer Role**: Defines project goals, system architecture, roadmap sequencing ([`docs/roadmap.md`](roadmap.md)), licensing rules ([`docs/licensing-policy.md`](licensing-policy.md)), engineering constraints ([`AGENTS.md`](../AGENTS.md)), phase scopes, and verification gates, and reviews all outputs before acceptance.
- **AI Coding Agent Role**: Writes kernel, driver, library, userland, test, and documentation code within the boundaries of each gated phase, executes automated host and QEMU verification suites, and produces raw diagnostic reports ([`docs/process/README.md`](process/README.md)).
- **Agent Environment**: Google Antigravity IDE / autonomous coding agent harness.
- **AI Models Used**: Google Gemini and Anthropic Claude (used for architectural planning, specification breakdown, code generation, and verification auditing under human supervision).

*(Note: Historical user prompts are not reconstructed from memory in this repository. The authoritative engineering constraints and phase outcomes are recorded in [`AGENTS.md`](../AGENTS.md), [`docs/DECISIONS/`](DECISIONS/), and [`docs/reports/`](reports/).)*

---

## 2. Meaning and Limits of "Written from Scratch"

When documentation describes OptiOS as an operating system written from scratch in Rust (`no_std`), that phrase means specifically:
1. It is not a fork or derivative distribution of the Linux kernel, a BSD kernel, or Windows PE.
2. The kernel (`kernel/`), protocol libraries (`libs/`), and Ring 3 utilities (`userland/`) were written as a new codebase for this repository.

At the same time, "from scratch" has clear technical boundaries:
- **Third-Party Bootloader & Crates**: OptiOS boots via the prebuilt **Limine** UEFI bootloader (`BSD-2-Clause`) and links the `limine` and `bitflags` Rust crates (`MIT`), plus a minimal 10-line PCI ID excerpt (`pci.ids`) and an 8x16 ASCII bitmap glyph table (`font_data.in`), all disclosed in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
- **Public Hardware & OS Specifications**: Register layouts, bitfields, descriptors, and calling conventions follow standard public specifications (UEFI, Intel 64 and IA-32 SDM, SATA AHCI 1.3, USB 3.0 xHCI 1.2, USB HID 1.11, System V AMD64 ABI, and POSIX conventions).
- **AI Model Training Data**: Large language models used to assist development were trained by their respective providers on broad corpora of existing public code and technical documentation.

---

## 3. Factual Provenance & Porting Rules

To maintain clean provenance across the repository, [`AGENTS.md`](../AGENTS.md) Section 4.3 and [`docs/licensing-policy.md`](licensing-policy.md) enforce the following factual rules:

1. **No Unrecorded Copying or Memory Paraphrasing**:
   - Agents are prohibited from copying or paraphrasing third-party driver implementations from memory.
   - Native drivers (`ahci`, `xhci`, `ps2-kbd`, `ps2-mouse`, `uart-16550`, `uefi-gop`) are written against hardware register specifications and registered in the central Driver Registry (`/proc/drivers`) as `Origin: Native`.
2. **Explicit Porting Gate (Planned in Roadmap R6)**:
   - When a foreign driver is ported in the future (such as the planned BSD NIC driver pilot in Roadmap R6), it requires explicit prior owner approval per driver, must be taken from a named upstream repository and commit with an `ALLOWED` license, must reside in a separated directory preserving its upstream copyright and license header, and must be recorded in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) before any code lands. No foreign driver has been ported yet.
3. **No Legal Conclusions Drawn**:
   - In accordance with [`AGENTS.md`](../AGENTS.md) Section 4.3, this repository records the factual use of AI assistance and third-party components and draws **no legal conclusions** regarding AI-generated copyright or process-isolated driver licensing.
