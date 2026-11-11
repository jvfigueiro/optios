<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Security Policy & Vulnerability Reporting
// Date: 2026-11-11
-->

# Security Policy

## Project Status Disclaimer

**OptiOS is an experimental early-alpha research and learning operating system.**
- It is verified only in QEMU (`pc-q35` + Tianocore EDK II OVMF) and has **not** been tested on physical hardware.
- It should **never** be deployed in production environments or booted on physical machines containing important data.
- Known security and architectural gaps (such as missing W^X enforcement on ELF `PT_LOAD` segments in [`docs/known-issues.md`](docs/known-issues.md) item `P0.3`) are documented openly as part of the engineering study.

## Reporting a Vulnerability

If you discover a security vulnerability, memory-safety defect, or read-only storage bypass that is not already listed in [`docs/known-issues.md`](docs/known-issues.md) or [`docs/reports/10.1-verification.md`](docs/reports/10.1-verification.md):

1. **Do not open a public GitHub Issue** if the finding could affect upstream dependencies or host build/test scripts.
2. **Use GitHub Private Vulnerability Reporting (Security Advisories)**:
   - Navigate to the **Security** tab of this repository on GitHub and click **"Report a vulnerability"** to submit a private advisory directly to the maintainer.
3. Include a minimal reproduction step, the affected file/line range, and raw QEMU or `cargo test` output demonstrating the issue.

As an individual learning and research project, response times are best-effort.
