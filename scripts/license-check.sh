#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (C) 2026 João Figueiró
# Item: License Compliance & SPDX Verification Script
# Date: 2026-11-10

set -euo pipefail

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${PROJECT_ROOT}"

echo "==> [License Check] Scanning source files for SPDX headers and policy compliance..."

python3 - << 'EOF'
import os
import sys
import json
import subprocess

DENIED_SPDX = [
    "GPL-3.0-only", "GPL-3.0-or-later", "GPL-3.0",
    "AGPL-1.0", "AGPL-3.0", "AGPL-3.0-only", "AGPL-3.0-or-later",
    "BSD-4-Clause", "CDDL-1.0", "CDDL-1.1", "EPL-1.0", "EPL-2.0",
    "Proprietary", "unknown"
]

ALLOWED_SPDX = [
    "GPL-2.0-only", "GPL-2.0-or-later", "GPL-2.0",
    "LGPL-2.1-only", "LGPL-2.1-or-later", "LGPL-2.1",
    "BSD-2-Clause", "BSD-3-Clause",
    "ISC", "MIT", "MIT-0", "Zlib"
]

REVIEW_SPDX = [
    "MPL-2.0", "CC0-1.0", "Unlicense"
]

errors = 0
checked_files = 0

# 1. Check tracked source files (.rs, .ld, .sh, .py)
for root, dirs, files in os.walk("."):
    dirs[:] = [d for d in dirs if d not in ("target", "build", "vendor", ".git", ".system_generated")]
    for fname in files:
        ext = os.path.splitext(fname)[1]
        if ext in (".rs", ".ld", ".sh", ".py"):
            fpath = os.path.join(root, fname)
            checked_files += 1
            with open(fpath, "r", encoding="utf-8", errors="replace") as f:
                content = f.read(1024)
            
            # Find SPDX-License-Identifier line
            spdx_line = None
            for line in content.splitlines():
                if "SPDX-License-Identifier:" in line:
                    spdx_line = line.split("SPDX-License-Identifier:", 1)[1].strip()
                    break
            
            if not spdx_line:
                print(f"ERROR: Missing SPDX-License-Identifier in {fpath}", file=sys.stderr)
                errors += 1
            else:
                for denied in DENIED_SPDX:
                    if denied.lower() in spdx_line.lower():
                        print(f"ERROR: DENIED license '{spdx_line}' found in {fpath}", file=sys.stderr)
                        errors += 1

print(f"==> Checked {checked_files} source files for SPDX headers.")

# 2. Check Cargo metadata dependencies
try:
    meta_json = subprocess.check_output(["cargo", "metadata", "--format-version", "1"], stderr=subprocess.PIPE)
    meta = json.loads(meta_json)
    packages = meta.get("packages", [])
    print(f"==> Inspecting licenses for {len(packages)} Cargo packages...")

    for pkg in packages:
        pname = pkg["name"]
        pver = pkg["version"]
        license_str = pkg.get("license") or "unknown"
        
        # Check against denied
        is_denied = False
        for denied in DENIED_SPDX:
            # Check standalone denied identifier (e.g. Apache-2.0 alone)
            if license_str.strip() == "Apache-2.0" or denied.lower() == license_str.lower():
                is_denied = True
                break
        
        if is_denied:
            print(f"ERROR: Package {pname} {pver} has DENIED license: '{license_str}'", file=sys.stderr)
            errors += 1
            continue

        # Check if license contains allowed permissive components for dual licenses
        # e.g., "MIT OR Apache-2.0" allows MIT
        parts = [p.strip() for p in license_str.replace("(", "").replace(")", "").split(" OR ")]
        has_allowed = any(p in ALLOWED_SPDX for p in parts)
        has_review = any(p in REVIEW_SPDX for p in parts)

        if has_allowed:
            # Satisfied by an allowed option
            pass
        elif has_review:
            print(f"WARNING [REVIEW]: Package {pname} {pver} has license requiring owner review: '{license_str}'")
        else:
            print(f"ERROR: Package {pname} {pver} has unapproved or unknown license: '{license_str}'", file=sys.stderr)
            errors += 1

except Exception as ex:
    print(f"ERROR: Failed to run cargo metadata: {ex}", file=sys.stderr)
    errors += 1

if errors > 0:
    print(f"\nFAILURE: License check failed with {errors} violation(s).", file=sys.stderr)
    sys.exit(1)
else:
    print("==> [License Check] All source headers and dependencies COMPLY with OptiOS policy.")
EOF
