# OptiOS — Engineering Rules

## A. Working Rules

1. **Evidence before fixes**: Never change what you cannot first demonstrate.
2. **Paste raw output**: Paste raw output (cargo, QEMU serial, grep, diff). No summaries in place of output.
3. **Strict scope**: Touch only what the phase names; list every file touched.
4. **Integrity and safety**: No test edits just to force a pass (state the reason for any test edit); no git revert/reset/checkout; no disk-write code paths without explicit owner authorization; read-only must stay provable.
5. **Language and terminology**: English for identifiers, comments, logs and docs. Say "POSIX-oriented", never "POSIX-compliant". Never document a feature that is not implemented.
6. **Snapshot before each phase**: Git commit if a repo exists, otherwise `tar czf ../optios-<phase>-$(date +%Y%m%d).tar.gz --exclude=target --exclude=build .`.
7. **Gated phases**: Finish, report, STOP, wait for owner approval.

## 1. Project Identity

OptiOS is an independent operating system developed from scratch.

Its initial purpose is to provide a UEFI-bootable, x86-64 live
environment for computer diagnostics and recovery.

The project is not a Linux distribution, a BSD derivative,
or a modified Windows PE environment.

## 2. Architectural Principles

- Target architecture: x86-64.
- Firmware: UEFI exclusively.
- Initial boot medium: removable USB storage.
- Kernel: custom implementation in Rust.
- Kernel environment: no_std.
- Bootloader: Limine, initially.
- Development and testing: Debian, QEMU and OVMF.
- Unix-like architectural principles.
- POSIX compatibility as a long-term objective.
- Native graphical environment developed by the project.

## 3. Development Philosophy

- Prefer simple, modular and maintainable implementations.
- Do not implement unnecessary functionality.
- Do not introduce external dependencies without justification.
- Do not imitate Linux internals without understanding their purpose.
- Do not assume Linux kernel APIs are available.
- Do not introduce a dependency on the Linux kernel.
- Reuse mature third-party components when architecturally appropriate.
- Keep the kernel independent from user applications.

## 4. Rust Requirements

- Use stable Rust unless a specific technical reason
  requires another toolchain.
- Use no_std for kernel code.
- Avoid accidental dependencies on std.
- Document unsafe blocks and their safety invariants.
- Prefer explicit interfaces and well-defined ownership.
- Keep target-specific code isolated.
- Do not introduce unnecessary nightly features.

## 4.1 Language and Documentation Standard

- All source code, comments, identifiers, variable names, module names,
  and technical documentation (docs/) MUST be exclusively in English.
- All system output (serial console logs, framebuffer UI, error messages,
  panic screens, exception dumps) MUST be exclusively in English.
- Chat communication with the project maintainer may occur in Portuguese,
  but any repository artifact or code committed MUST be in English.

## 4.2 Mandatory File Copyright Header

Every source code file, linker script, and build/test script (`.rs`, `.ld`, `.sh`, `.py`)
in the repository MUST begin with the standardized OptiOS copyright header:

```text
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: <Component Name / File Purpose>
// Date: <YYYY-MM-DD>
```

(Using `#` for shell/Python scripts after the shebang line, or `/* ... */` / `//` as appropriate for linker scripts.)
The `Date:` field is date only (`YYYY-MM-DD`), with no hour or minute. The date is the date of the last edit to that file, and it is only updated when the file's content actually changes (never by tooling that merely touches the file).
Do not include "All rights reserved".

## 4.3 Licensing & Provenance

Development is private; nothing is published yet.
Owner to verify licensing statements with qualified counsel before any distribution.

- **OptiOS-original code**: SPDX `GPL-2.0-or-later`.
- **Imported code**: Keeps its own copyright and license, lives in clearly separated directories and is recorded in `THIRD_PARTY_NOTICES.md` (URL, commit/version, SPDX, local modifications).
- **ALLOWED in the kernel image**: `GPL-2.0-only`, `GPL-2.0-or-later`, `LGPL-2.1-only`, `LGPL-2.1-or-later`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `MIT`, `MIT-0`, `Zlib`. For "A OR B" dual licenses, record the option used (the compatible one).
- **REVIEW (owner decides case by case)**: `MPL-2.0`, `CC0-1.0`, `Unlicense`, others.
- **DENIED in the kernel image or anything linked with it**: `GPL-3.0-only`, `GPL-3.0-or-later`, `AGPL-*`, `Apache-2.0` alone, `BSD-4-Clause`, `CDDL-*`, `EPL-*`, proprietary, unknown, or missing SPDX.
- **Firmware blobs**: Never committed without recorded redistribution terms; kept apart from source; flagged in notices.
- **Process-isolated foreign drivers**: Whether process-isolated foreign drivers change the legal picture is an OPEN question: draw no legal conclusions anywhere.
- **AI-assisted code**: Part of the code is AI-assisted: record that fact in docs; draw no conclusion about copyright.
- **Third-party contributions**: No third-party contributions until the owner decides on DCO/CLA.
- **Porting policy**: Never copy or paraphrase foreign code from memory. A port happens only with explicit owner approval per driver, from a named source, and is recorded before any code lands.

## 5. Architecture and Decisions

- Never change the project architecture without presenting
  the technical justification first.
- Do not replace Rust with another language without approval.
- Do not introduce BIOS Legacy support.
- Do not introduce x86 32-bit support.
- Do not implement unrelated features prematurely.
- Record significant architectural decisions in docs/DECISIONS/.
- Maintain consistency with the project documentation.

## 6. Development Workflow

Before implementing a significant feature:

1. Inspect the existing code.
2. Understand the current architecture.
3. Explain the proposed implementation.
4. Identify dependencies and risks.
5. Define how the result will be tested.
6. Implement only the approved scope.

Never overwrite existing work without inspecting it.

Never remove functionality simply to make a build succeed.

Never claim that something works without executing
the appropriate verification.

## 7. Testing and Verification

- Compile after meaningful changes.
- Run available automated tests.
- Verify generated binaries and their formats.
- Use QEMU/OVMF for boot verification.
- Preserve build logs when troubleshooting.
- Distinguish between compilation success and actual
  runtime correctness.

## 8. Documentation

Document:
- Architectural decisions.
- Public interfaces.
- Important data structures.
- Memory management assumptions.
- Hardware dependencies.
- Build and execution procedures.
- Known limitations.

Documentation must reflect the actual implementation,
not merely the intended design.

## 9. Security and Safety

- Do not execute destructive commands without approval.
- Do not format or modify physical storage devices.
- Do not overwrite host firmware or bootloaders.
- Do not modify the VPS host boot configuration.
- Use virtual machines for operating system testing.
- Never assume a hardware operation is safe.

## 10. Agent Behavior

Act as a senior systems engineering assistant.

Challenge questionable decisions.
Explain trade-offs.
Prefer technical correctness over convenience.
Ask questions when requirements are ambiguous.

Do not silently make major architectural decisions.

The human maintainer retains final authority over
architecture, implementation strategy and acceptance.
