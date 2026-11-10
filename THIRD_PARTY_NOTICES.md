<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Third-Party Notices & Component Disclosures
// Date: 2026-11-10
-->

# Third-Party Notices & Component Disclosures

Owner to verify licensing statements with qualified counsel before any distribution.

This document records third-party software components bundled, vendored, compiled into the kernel image, or used as host development tools with OptiOS, in compliance with the OptiOS Licensing Policy (`docs/licensing-policy.md`).

---

## 1. Shipped / Bundled Components

### 1.1 Limine Bootloader Binary Payloads (`vendor/limine/`)
- **Component**: Limine Bootloader (Binary EFI payloads `BOOTX64.EFI`, `limine-uefi-cd.bin`, BIOS stages)
- **Origin / Source**: [https://github.com/limine-bootloader/limine](https://github.com/limine-bootloader/limine)
- **Version**: v8.x-binary branch
- **SPDX-License-Identifier**: `BSD-2-Clause`
- **Copyright**: Copyright (C) 2019-2025 mintsuki and contributors
- **Notice**:
```text
Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions are met:

1. Redistributions of source code must retain the above copyright notice, this
   list of conditions and the following disclaimer.

2. Redistributions in binary form must reproduce the above copyright notice,
   this list of conditions and the following disclaimer in the documentation
   and/or other materials provided with the distribution.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.
```

---

### 1.2 PCI ID Database Subset (`initramfs:/usr/share/hwdata/pci.ids`)
- **Component**: PCI ID Database (curated minimal subset of vendor and device IDs for Q35/ICH9 chipset and virtual hardware)
- **Origin / Source**: [https://pci-ids.ucw.cz/](https://pci-ids.ucw.cz/)
- **Maintainer**: Martin Mares, Albert Pool, and contributors
- **SPDX-License-Identifier**: `BSD-3-Clause OR GPL-2.0-or-later`
- **Selected Option**: `GPL-2.0-or-later` (compatible with OptiOS kernel image)
- **Modifications**: Minimal 10-line text subset containing only devices enumerated during QEMU/Q35 live boot.

---

### 1.3 Rust Crate Dependencies (via `cargo metadata`)

#### `limine` (crate version 0.5.0)
- **Component**: Limine boot protocol Rust interface structures
- **Source**: [https://github.com/limine-bootloader/limine-rs](https://github.com/limine-bootloader/limine-rs)
- **SPDX-License-Identifier**: `MIT OR Apache-2.0`
- **Selected Option**: `MIT` (permissive, compatible with GPL-2.0-or-later)
- **Local Modifications**: None (compiled via Cargo)

#### `bitflags` (crate version 2.13.2)
- **Component**: Bitmask flags macro and utility types (transitive dependency of limine)
- **Source**: [https://github.com/bitflags/bitflags](https://github.com/bitflags/bitflags)
- **SPDX-License-Identifier**: `MIT OR Apache-2.0`
- **Selected Option**: `MIT` (permissive, compatible with GPL-2.0-or-later)
- **Local Modifications**: None (compiled via Cargo)

---

## 2. Host Build & Verification Tools (NOT Shipped)

The following tools are used strictly in the host development/test environment and are **not shipped** in the OptiOS binary kernel image or distribution medium:

### 2.1 QEMU (`qemu-system-x86_64`)
- **Purpose**: Virtual machine hardware emulation for Q35/OVMF automated testing.
- **SPDX-License-Identifier**: `GPL-2.0-only` / `GPL-2.0-or-later`
- **Distribution Status**: **Not shipped** (host development and CI tool only).

### 2.2 OVMF (`OVMF.fd` / EDK II)
- **Purpose**: Open-source UEFI firmware binary for QEMU testing.
- **Origin**: Debian distribution of Tianocore EDK II.
- **SPDX-License-Identifier**: `BSD-2-Clause-Patent`
- **Distribution Status**: **Not shipped** (host development and test firmware only).

### 2.3 xorriso
- **Purpose**: Host utility to generate ISO 9660 filesystem images (`scripts/build.sh`).
- **SPDX-License-Identifier**: `GPL-3.0-or-later`
- **Distribution Status**: **Not shipped** (external host build tool only; never linked or included in kernel).
