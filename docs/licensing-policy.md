<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: OptiOS Licensing & Static Assets Policy
// Date: 2026-11-11
-->

# OptiOS Licensing Policy

Owner to verify licensing statements with qualified counsel before any distribution.

This policy replaces the earlier "GPLv3 planned" notes and legacy proprietary reservation placeholders.

## 1. Source Code & Kernel Image Licensing

- **OptiOS-original code**: SPDX `GPL-2.0-or-later`.
- **Imported code**: Keeps its own copyright and license, lives in clearly separated directories, and is recorded in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md) (URL, commit/version, SPDX, local modifications).
- **ALLOWED in the kernel image**: `GPL-2.0-only`, `GPL-2.0-or-later`, `LGPL-2.1-only`, `LGPL-2.1-or-later`, `BSD-2-Clause`, `BSD-3-Clause`, `ISC`, `MIT`, `MIT-0`, `Zlib`. For "A OR B" dual licenses, record the option used (the compatible one).
- **REVIEW (owner decides case by case)**: `MPL-2.0`, `CC0-1.0`, `Unlicense`, `OFL-1.1`, others.
- **DENIED in the kernel image or anything linked with it**: `GPL-3.0-only`, `GPL-3.0-or-later`, `AGPL-*`, `Apache-2.0` alone, `BSD-4-Clause`, `CDDL-*`, `EPL-*`, proprietary, unknown, or missing SPDX.
- **Firmware blobs**: Never committed without recorded redistribution terms; kept apart from source; flagged in notices.
- **Process-isolated foreign drivers**: Whether process-isolated foreign drivers change the legal picture is an OPEN question: draw no legal conclusions anywhere.
- **AI-assisted code**: Part of the code is AI-assisted: record that fact in docs; draw no conclusion about copyright.
- **Third-party contributions**: No third-party contributions until the owner decides on DCO/CLA.
- **Porting policy**: Never copy or paraphrase foreign code from memory. A port happens only with explicit owner approval per driver, from a named source, and is recorded before any code lands.

## 2. Static Assets Policy (Fonts, Images, and Data Files)

- **Font Files (`.ttf`, `.otf`, `.woff`, `.woff2`, `.psf`, `.bdf`, `.fon`)**:
  - No third-party scalable or bitmap font files (`.ttf`, `.otf`, etc.) may be committed to the repository or embedded into `initramfs` or the kernel image without explicit owner approval and full license recording in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
  - Currently, **zero** `.ttf`/`.otf` font files are present in the repository (enforced automatically by [`scripts/license-check.sh`](../scripts/license-check.sh)).
  - Embedded source-level glyph tables (such as `kernel/src/drivers/video/font_data.in`) must carry an SPDX header and be disclosed in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
- **Images & Screenshots (`docs/images/*.png`)**:
  - Repository images must be first-party captures generated from OptiOS execution in QEMU or original diagrams authored for the project under `GPL-2.0-or-later`.
  - Screenshots must not contain host personal identifiers, local filesystem paths, or third-party proprietary UI elements.
- **Hardware Data Files (`pci.ids`, etc.)**:
  - Any hardware ID table or static dataset bundled into `initramfs` must come from an upstream source with an `ALLOWED` license (e.g., `pci.ids` under `GPL-2.0-or-later OR BSD-3-Clause`, selecting `GPL-2.0-or-later`) and be documented in [`THIRD_PARTY_NOTICES.md`](../THIRD_PARTY_NOTICES.md).
