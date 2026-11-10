<!--
// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Third-Party Notices & Component Disclosures
// Date: 2026-11-09
-->

# Third-Party Notices & Component Disclosures

This document lists third-party software components bundled, vendored, or distributed with OptiOS, along with their respective licensing notices.

---

## 1. OptiOS Project Licensing Notice

> **Important Notice:**
> This repository and its distributed binaries contain proprietary or reserved components of OptiOS.
> Every source code file, script, and linker descriptor contains the header:
> ```text
> Copyright (C) 2026 João Figueiró. All rights reserved.
> ```
> A copyright header indicates ownership and reservation of rights; **it does not constitute an open-source or permissive license grant**.
> This document does **NOT** establish or define a software license for OptiOS itself; licensing and distribution terms for OptiOS remain the exclusive prerogative of the project maintainer.

---

## 2. Third-Party Components

### 2.1 Limine Bootloader (`vendor/limine`)

- **Component:** Limine Bootloader (Binary EFI payloads, BIOS stages, and C header/installer utilities).
- **Origin / Source:** [https://github.com/limine-bootloader/limine](https://github.com/limine-bootloader/limine)
- **License:** 2-Clause BSD License ("Simplified BSD License" / "FreeBSD License")
- **Copyright:** Copyright (C) 2019-2025 mintsuki and contributors.

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

### 2.2 PCI ID Database Subset (`/usr/share/hwdata/pci.ids`)

- **Component:** PCI Hardware Identification Database (curated minimal subset embedded within the boot initramfs image).
- **Origin / Source:** The PCI ID Repository ([https://pci-ids.ucw.cz/](https://pci-ids.ucw.cz/))
- **Maintainer:** Martin Mares and contributors.
- **License:** Dual-licensed under the 3-Clause BSD License and the GNU General Public License (GPL) version 2 or later.
- **Notice Summary:**
  The subset shipped in `initramfs.tar` includes vendor and device identification strings for common virtualized and physical chipset devices (Intel Q35 DRAM controller, ICH9 LPC/SATA/SMBus, Intel 82574L NIC, QEMU standard VGA).
