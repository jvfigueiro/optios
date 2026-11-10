# OptiOS Licensing Policy

Owner to verify licensing statements with qualified counsel before any distribution.

This policy replaces the earlier "GPLv3 planned" and the placeholder "All rights reserved". Development is private; nothing is published yet.

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
