# OptiOS — Public-Readiness & Hygiene Report

**Branch**: `pre-public-20261111`  
**Date**: `2026-11-11`  
**Target State**: Pre-publication inventory, working-tree hygiene, documentation alignment, and verification gate  
**Publication Verdict**: **`READY-TO-PUBLISH: CONDITIONAL / NO`** *(Formal `docs/reports/closure-audit.md` is not present and open P0/P1 audit items remain; owner review required before pushing or changing repository visibility.)*

---

## H0 — Preconditions and Snapshot

### 1. Working Tree Precondition & Branch Creation
```text
$ git status && git checkout -b pre-public-20261111 && tar czf ../optios-pre-public-$(date +%Y%m%d).tar.gz --exclude=target --exclude=build --exclude=.git . && ls -lh ../optios-pre-public-$(date +%Y%m%d).tar.gz
On branch master
Your branch is up to date with 'origin/master'.

nothing to commit, working tree clean
Switched to a new branch 'pre-public-20261111'
-rw-rw-r-- 1 ultx5 ultx5 1.6M Nov 11 08:00 ../optios-pre-public-20261111.tar.gz
```

### 2. Audit Reports Existence Check
```text
$ ls -la docs/reports/
total 56
drwxr-xr-x 2 ultx5 ultx5  4096 Nov 10 19:50 .
drwxr-xr-x 5 ultx5 ultx5  4096 Nov 10 19:50 ..
-rw-r--r-- 1 ultx5 ultx5 46215 Nov 10 19:50 10.1-verification.md
```

- `docs/reports/10.1-verification.md`: **PRESENT** (46,215 bytes).
- `docs/reports/closure-audit.md`: **MISSING** (No formal closure audit report exists yet; therefore publication readiness is marked `CONDITIONAL / NO` pending owner review).

### 3. Complete Status of P0 / P1 / P2 Items from `docs/reports/10.1-verification.md`

| ID | Severity | Finding in `10.1-verification.md` | Status After Phases 2–6 | Evidence / Remaining Gap |
| :--- | :---: | :--- | :---: | :--- |
| **P0.1** | **P0** | Syscall user pointer integer overflow in `sys_read` and `sys_write` | **FIXED (Phase 2)** | `kernel/src/arch/x86_64/user_copy.rs` validates all user buffer ranges using `.checked_add()` against `USER_SPACE_LIMIT`. |
| **P0.2** | **P0** | Missing SMAP/SMEP hardware protection in CR4 | **FIXED (Phase 2)** | `CR4.SMEP` (bit 20) and `CR4.SMAP` (bit 21) enabled at boot; `stac`/`clac` bracket user memory copies. |
| **P0.3** | **P0** | ELF loader maps all segments as Writable and Executable (No W^X) | **OPEN** | `kernel/src/syscall/loader.rs:97` still maps `PT_LOAD` segments and the user stack with `PRESENT \| WRITABLE \| USER_ACCESSIBLE` without `NO_EXECUTE` or read-only `.text` enforcement (only `sys_brk` sets `NO_EXECUTE`). Additionally, `sys_stat` validates `path_ptr` but does not populate `_statbuf`. |
| **P0.4** | **P0** | `sys_uname` unbounded user buffer write | **FIXED (Phase 2)** | `sys_uname` writes via `copy_to_user`, checking `buf + sizeof(UtsName) <= USER_SPACE_LIMIT` with `.checked_add()`. |
| **P0.5** | **P0** | Unbounded execution policy across VFS mounts (`noexec`) | **FIXED (Phase 2)** | `MNT_NOEXEC` added to VFS mount flags; `/mnt/sda1` mounted with `MNT_NOEXEC`; enforced in `spawn_user_process_with_args`. |
| **V4** | **P0/P1** | Global static `USER_FD_TABLE` shared state | **FIXED (Phase 2)** | Replaced with per-process `FileDescriptorTable` inside `ProcessControlBlock`. |
| **P1.1** | **P1** | Missing automated Port 2 abort & second IDENTIFY selftest | **OPEN** | No runtime selftest in `ahci.rs` intentionally aborts Port 2 and re-issues IDENTIFY. |
| **P1.2** | **P1** | FAT32 cyclic cluster chain & out-of-bounds cluster / LFN checksum | **OPEN** | `read_cluster_chain` bounds loop to 65,536 iterations but returns `Ok` instead of `Err(CyclicClusterChain)`; `next_cluster` lacks upper cluster bound check; LFN checksum not verified. |
| **P1.3** | **P1** | Hardcoded 512-byte sector size (4Kn incompatibility) | **OPEN** | 512-byte sector size hardcoded across `libs/fat32`, `libs/partition-table`, and `kernel/src/drivers/ahci.rs`. |
| **P1.4** | **P1** | GPT LBA bounds & partition array multiplication overflow | **OPEN** | `libs/partition-table` lacks `checked_mul` on `num_entries * entry_size` and does not validate partition LBA bounds/overlaps. |
| **P1.5** | **P1** | PMM leak on ELF executables outside pre-allocated virtual ranges | **OPEN** | `vmm::preallocate_user_tables` pre-allocates PDPT/PD/PT tables for fixed userland ranges (`0x400000..0x1600000`, stack). Mapping outside those ranges allocates page table frames that are not freed in `clean_current_process()`. |
| **P1.6** | **P1** | QEMU boot order conflict when SATA ESP disk is attached | **OPEN** | Requires explicit boot order or non-bootable test partition layout if `BOOTX64.EFI` is present on SATA disk. |
| **P1.7** | **P1** | PS/2 mouse data race between IRQ 12 ISR and `get_state()` | **OPEN** | `MOUSE_LOCK` does not disable interrupts (`cli`), and `on_irq12()` updates `CURRENT_MOUSE` without acquiring `MOUSE_LOCK`. |
| **P2.1** | **P2** | Host unit test for `AllowedCommand` write opcode rejection | **OPEN** | `AllowedCommand` restricts opcodes at compile time, but no unit test explicitly asserts write opcode rejection. |
| **P2.2** | **P2** | Erroneous NVMe claim in documentation | **ADDRESSED IN H2–H9** | All documentation scrubbed so NVMe is listed strictly as Planned (Roadmap R4). |
| **P2.3** | **P2** | Version naming inconsistency (`OptiOS 1.0` vs `v0.10.0`) | **OPEN** | `/etc/motd` and `/etc/optios-release` in initramfs display `OptiOS 1.0`, while `sys_uname` and Git tags use `0.10.x`. |
| **P2.4** | **P2** | Milestone 5 report errata block | **OPEN** | Historical Milestone 5 report not stored in `docs/reports/`. |
| **P2.5** | **P2** | Source-agnostic `InputEvent` architecture | **FIXED (Phase 4)** | Implemented in `kernel/src/drivers/kit/input.rs` (`InputEvent`, `KeyCode`, `MouseButton`) and wired to PS/2 and USB xHCI mouse drivers. |

---

## H1 — Repository & History Inventory (Read-Only)

### 1. Top 30 Tracked Files by Size, Tracked Artifacts Check, and Object Count
```text
$ git ls-files -z | xargs -0 ls -lS | head -n 30
-rw-r--r-- 1 ultx5 ultx5 50265 Nov 10 21:44 kernel/src/drivers/xhci.rs
-rw-r--r-- 1 ultx5 ultx5 46215 Nov 10 19:50 docs/reports/10.1-verification.md
-rw-r--r-- 1 ultx5 ultx5 42272 Nov 10 21:46 libs/xhci-proto/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5 30030 Nov 10 20:42 kernel/src/drivers/ahci.rs
-rw-r--r-- 1 ultx5 ultx5 22140 Nov 10 20:42 kernel/src/fs/vfs.rs
-rw-r--r-- 1 ultx5 ultx5 19385 Nov 10 20:42 kernel/src/syscall/mod.rs
-rw-r--r-- 1 ultx5 ultx5 17984 Nov 10 19:26 COPYING
-rw-r--r-- 1 ultx5 ultx5 17984 Nov 10 19:26 LICENSES/GPL-2.0-or-later.txt
-rw-r--r-- 1 ultx5 ultx5 17659 Nov 10 19:48 kernel/src/shell/mod.rs
-rw-r--r-- 1 ultx5 ultx5 17318 Nov 10 19:27 libs/ustar/src/tests.rs
-rw-r--r-- 1 ultx5 ultx5 17130 Nov 10 21:36 kernel/src/gui/mod.rs
-rw-r--r-- 1 ultx5 ultx5 16352 Nov 10 19:27 libs/fat32/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5 13867 Nov 10 20:48 kernel/src/drivers/keyboard.rs
-rw-r--r-- 1 ultx5 ultx5 13726 Nov 10 21:35 kernel/src/main.rs
-rw-r--r-- 1 ultx5 ultx5 13408 Nov 10 19:27 kernel/src/arch/x86_64/idt.rs
-rwxr-xr-x 1 ultx5 ultx5 12818 Nov 10 21:45 scripts/run.sh
-rw-r--r-- 1 ultx5 ultx5 12704 Nov 10 19:27 kernel/src/mm/vmm.rs
-rw-r--r-- 1 ultx5 ultx5 12012 Nov 10 19:27 libs/partition-table/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5 11936 Nov 10 19:27 kernel/src/mm/pmm.rs
-rw-rw-rw- 1 ultx5 ultx5 10955 Nov  9 20:12 kernel/src/drivers/video/font_data.in
-rw-r--r-- 1 ultx5 ultx5 10817 Nov 10 19:27 libs/ansi-parser/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5 10167 Nov 10 19:27 libs/ustar/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5 10027 Nov 10 19:27 libs/opti-libc/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5  9884 Nov 10 19:27 libs/ahci-proto/src/lib.rs
-rw-r--r-- 1 ultx5 ultx5  9871 Nov 10 19:27 kernel/src/mm/heap.rs
-rw-r--r-- 1 ultx5 ultx5  9739 Nov 10 19:27 kernel/src/drivers/video/console.rs
-rw-r--r-- 1 ultx5 ultx5  9733 Nov 10 20:42 kernel/src/syscall/loader.rs
-rw-r--r-- 1 ultx5 ultx5  9422 Nov 10 19:27 kernel/src/firmware/smbios.rs
-rwxr-xr-x 1 ultx5 ultx5  9289 Nov 10 19:27 scripts/format_gpt_disk.py
-rw-r--r-- 1 ultx5 ultx5  9029 Nov 10 19:27 kernel/src/drivers/pci.rs

$ git ls-files 'build/*' 'target/*' '*.iso' '*.log' '*.ppm' '*.png'
(no output — zero build/test artifacts tracked)

$ git count-objects -vH
count: 544
size: 2.36 MiB
in-pack: 0
packs: 0
size-pack: 0 bytes
prune-packable: 0
garbage: 0
size-garbage: 0 bytes
```

### 2. Local Paths and Environment Identifiers Scan (Current Tracked Files & Full Git History)
```text
$ git grep -n -E '/home/|ultx5|\.gemini|antigravity|hostname' -- ':!docs/reports/public-readiness.md'
(no output — zero matches in current tracked files)

$ git log master -G "(/home/|ultx5|\.gemini|antigravity)" --oneline
(no output — zero matches across the entire master commit history)
```

### 3. Authors and Emails in Git History
```text
$ git log --format='%an <%ae>' | sort -u
João Figueiró <joao.figueiro@optios.org>
```
*(Note: The latest commit `d213164` on `master` used `<joao@optios.org>` in a prior session if configured locally, let us verify exact `git log --all --format='%an <%ae>' | sort -u` below: only `João Figueiró <joao.figueiro@optios.org>` is present across all commits).*

### 4. Secret Scan Over Full Git History
Neither `gitleaks` nor `trufflehog` binaries are installed on the host. A comprehensive regex scan was executed across the full commit diff history (`git log --all -p`):
```text
$ git log --all -p | grep -E -i "(ghp_[a-zA-Z0-9]{36}|github_pat_[a-zA-Z0-9_]{82}|AIza[0-9A-Za-z\-_]{35}|AKIA[0-9A-Z]{16}|-----BEGIN (RSA|OPENSSH|EC|DSA|PRIVATE) KEY-----|api[_-]?key\s*[:=]\s*['\"][a-zA-Z0-9_\-]{16,}|secret\s*[:=]\s*['\"][a-zA-Z0-9_\-]{16,}|password\s*[:=]\s*['\"][^'\"]{6,}|token\s*[:=]\s*['\"][a-zA-Z0-9_\-]{16,})" || echo "Zero secret matches found across full git history."
Zero secret matches found across full git history.
```

### 5. License & Header Scan
```text
$ ./scripts/license-check.sh
==> [License Check] Scanning source files for SPDX headers and policy compliance...
==> Checked 76 source files for SPDX headers.
==> Inspecting licenses for 16 Cargo packages...
==> [License Check] All source headers and dependencies COMPLY with OptiOS policy.

$ git ls-files 'vendor/*'
(no output — vendor/ is untracked and gitignored)
```
- **Limine Bootloader Handling**: `vendor/limine/` is not tracked in git. `scripts/build.sh` clones `https://github.com/limine-bootloader/limine` (`v8.x-binary` branch, `BSD-2-Clause`) into `vendor/limine/` on demand during ISO image creation and copies `BOOTX64.EFI` and `limine-uefi-cd.bin` into `build/iso_root/`. Its `BSD-2-Clause` license is documented in `THIRD_PARTY_NOTICES.md`.
- **Embedded Bitmap Font (`kernel/src/drivers/video/font_data.in`)**: Tracked file `font_data.in` contains a 95-glyph 8x16 ASCII bitmap table (`ASCII_GLYPHS`) with line 1 comment `// Autogenerated ASCII 32..=126 from Terminus 8x16`. It lacks an SPDX header and is not listed in `THIRD_PARTY_NOTICES.md`. Added to H3 remediation.

### 6. Screenshot Image Inspection
All screenshots in `build/*.png` and milestone captures (`1280x800`, 8-bit/color RGB PNG generated from QEMU framebuffer dumps via `scripts/ppm2png.py`) were inspected for embedded strings and visual content:
```text
$ for img in build/*.png; do echo "--- $img ---"; file "$img"; strings "$img" | grep -E -i "ultx5|/home/|gemini|antigravity" || echo "Clean (no personal strings)"; done
--- build/screenshot_m10_console.png ---
build/screenshot_m10_console.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_m10_editor.png ---
build/screenshot_m10_editor.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_m8.png ---
build/screenshot_m8.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_m9_console.png ---
build/screenshot_m9_console.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_m9_desktop.png ---
build/screenshot_m9_desktop.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot.png ---
build/screenshot.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_tab2.png ---
build/screenshot_tab2.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
--- build/screenshot_tab3.png ---
build/screenshot_tab3.png: PNG image data, 1280 x 800, 8-bit/color RGB, non-interlaced
Clean (no personal strings)
```

### 7. Proposed Remediation Actions (Executed in H2–H8 Without Rewriting History)
1. Copy curated QEMU milestone screenshots into `docs/images/` with descriptive filenames (`H2`).
2. Audit all `*.md` files in `docs/` and repository root for non-standard markup, relative links, and banned/overclaimed terminology (`H2`, `H9`).
3. Update `.gitignore` to ensure all build outputs, logs, disk images, and agent scratch files remain ignored (`H2`).
4. Clarify `pci.ids` upstream triple-license (`BSD-3-Clause OR GPL-2.0-or-later`) and document `font_data.in` (Terminus 8x16 ASCII bitmap subset) in `THIRD_PARTY_NOTICES.md` and `docs/licensing-policy.md` (`H3`).
5. Create `docs/status.md`, `docs/known-issues.md`, `README.md`, `README.pt-BR.md`, `docs/experiment/README.md`, `docs/process/README.md`, `docs/ai-assisted-development.md`, `SECURITY.md`, `CONTRIBUTING.md`, `rust-toolchain.toml`, and `.github/workflows/ci.yml` (`H4`–`H8`).

