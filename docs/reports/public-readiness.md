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
4. Clarify `pci.ids` upstream triple-license (`GPL-2.0-or-later OR BSD-3-Clause`) and document `font_data.in` (Terminus 8x16 ASCII bitmap subset) in `THIRD_PARTY_NOTICES.md` and `docs/licensing-policy.md` (`H3`).
5. Create `docs/status.md`, `docs/known-issues.md`, `README.md`, `README.pt-BR.md`, `docs/experiment/README.md`, `docs/process/README.md`, `docs/ai-assisted-development.md`, `SECURITY.md`, `CONTRIBUTING.md`, `rust-toolchain.toml`, and `.github/workflows/ci.yml` (`H4`–`H8`).

---

## Per-Task Commit Log (`H0`–`H8`) on `pre-public-20261111`

| Task | Commit Hash | Commit Message & Files Touched |
| :---: | :---: | :--- |
| **H0** | `948c4d7` | `docs(reports): H0 — Preconditions, backup snapshot, and 10.1 audit inventory` (`docs/reports/public-readiness.md`) |
| **H1** | `1451e94` | `docs(reports): H1 — Repository and git history inventory (read-only)` (`docs/reports/public-readiness.md`) |
| **H2** | `1b6e9c5` | `docs(hygiene): H2 — Curate docs/images screenshots, sanitize markdown terminology, and harden .gitignore` (`docs/images/*.png`, `AGENTS.md`, `.gitignore`, `docs/DECISIONS/0003-vmm-heap.md`, `0004-irq-pit-keyboard-smbios.md`, `0010-milestone-9-mouse-window-compositor-desktop-gui.md`, `0012-driver-architecture.md`, `docs/architecture/owner-decisions.md`, `docs/posix-foundation.md`) |
| **H3** | `24a392d` | `docs(license): H3 — Clarify primary third-party notices and static assets policy` (`THIRD_PARTY_NOTICES.md`, `docs/licensing-policy.md`, `kernel/src/drivers/video/font_data.in`, `scripts/license-check.sh`) |
| **H4** | `3b76373` | `docs(status): H4 — Add factual docs/status.md matrix and docs/known-issues.md` (`docs/status.md`, `docs/known-issues.md`) |
| **H5** | `674555f` | `docs(readme): H5 — Write sober English README.md and Portuguese README.pt-BR.md` (`README.md`, `README.pt-BR.md`) |
| **H6** | `8d5c74a` | `docs(experiment): H6 — Add research questions, metrics, and factual counts in docs/experiment/README.md` (`docs/experiment/README.md`) |
| **H7** | `5eceea5` | `docs(process): H7 — Add docs/process/README.md and docs/ai-assisted-development.md` (`docs/process/README.md`, `docs/ai-assisted-development.md`) |
| **H8** | `4fdf9a2` | `ci(community): H8 — Add SECURITY.md, CONTRIBUTING.md, rust-toolchain.toml, and hardened GitHub Actions CI` (`SECURITY.md`, `CONTRIBUTING.md`, `rust-toolchain.toml`, `.github/workflows/ci.yml`) |

---

## H9 — Final Verification & Term Scrub (Raw Outputs)

### 1. License Check, Host Unit Tests (85/85), Release Build & Headless QEMU Verification
```text
$ ./scripts/license-check.sh && ./scripts/test.sh && ./scripts/build.sh release
==> [License Check] Scanning source files, static assets, and Cargo dependencies for policy compliance...
==> Checked 77 source/include files for SPDX headers.
==> Checked static assets: 0 font files (.ttf/.otf), 11 image files in docs/images/.
==> Inspecting licenses for 16 Cargo packages...
==> [License Check] All source headers, static assets, and dependencies COMPLY with OptiOS policy.
==================================================================
 OptiOS Host Test Suite
 Crates: optios-ustar, optios-ahci-proto, optios-partition-table, optios-elf-loader, optios-ansi-parser, optios-wm, optios-fat32, optios-xhci-proto
 Target: x86_64-unknown-linux-gnu
==================================================================
test result: optios_ahci_proto ...... ok. 11 passed; 0 failed
test result: optios_ansi_parser ..... ok. 10 passed; 0 failed
test result: optios_elf_loader ...... ok.  6 passed; 0 failed
test result: optios_fat32 ........... ok.  6 passed; 0 failed
test result: optios_partition_table . ok.  6 passed; 0 failed
test result: optios_ustar ........... ok. 24 passed; 0 failed
test result: optios_wm .............. ok.  5 passed; 0 failed
test result: optios_xhci_proto ...... ok. 17 passed; 0 failed
==================================================================
 All host unit tests passed successfully. (85 passed; 0 failed)
==================================================================
==> [OptiOS Build] Compilation mode: release
    Finished `release` profile [optimized] target(s) in 3.40s
==> [OptiOS Build] Kernel compiled: target/x86_64-unknown-none/release/optios-kernel
==> [OptiOS Build] Compiling Ring 3 userland utilities...
    Finished `release` profile [optimized] target(s) in 0.00s
==> [OptiOS Build] USTAR Initramfs created: build/iso_root/boot/initramfs.tar
==> [OptiOS Build] Bootable UEFI ISO created successfully:
    build/optios.iso
```

```text
$ ./scripts/run.sh --test
==> [OptiOS Test] Executing automated verification on QEMU (Q35 + AHCI SSD, TCG mode, no KVM)...
[INFO ] OptiOS Kernel — Milestone 10: FAT32 Storage VFS Mounts & Ring 3 Text Inspector
[INFO ] Limine Protocol: Base revision handshake successful.
[INFO ] CPU Security: SMEP=true, SMAP=true
[INFO ] VFS: Mounted FAT32 filesystem on /dev/sda1 (start LBA 2048, 32768 sectors) at '/mnt/sda1' (Read-Only, noexec).
[INFO ] xHCI: Command Ring DMA verified! CommandCompletionEvent received: Code=1 (Success), Slot=0.
[INFO ] xHCI Slot 1: Activated HID Boot Protocol (SET_CONFIGURATION 0x01).
[INFO ] xHCI Slot 1: Configured Endpoint 0x81 (DCI 3) for HID Interrupt IN.
[INFO ] xHCI Slot 1: Live HID Boot report received (len=4)!
DISCOVERED USB DEVICES:
SLOT  PORT  SPEED  VID:PID    EP (DCI)  STATUS      REPORTS  DETAILS
----  ----  -----  -------    --------  ------      -------  -------
1     5     High-Speed (480 Mbps)  0627:0001  0x81 (3)  CONFIGURED  5        HID Boot Mouse
[INFO ] [opti-sh] '/bin/cat' exited status 0: PMM free frames before=127107, after=127107 (zero leak: true)
[INFO ] [opti-sh] '/bin/edit' exited status 0: PMM free frames before=127107, after=127107 (zero leak: true)
==> [OptiOS Test] Milestone 10 Automated Verification: PASSED SUCCESSFULLY!
```

### 2. Relative Markdown Link Resolution Check
```text
Checked 180 relative Markdown links across 32 tracked *.md files.
ALL relative Markdown links resolved to existing files/directories.
```

### 3. Mandatory Forbidden & Overclaimed String Grep Check
Checked across all tracked files (`git grep`, excluding `docs/reports/public-readiness.md`):
- `"All rights reserved"` (case-insensitive)
- `"/home/"`
- `"ultx5"`
- `".gemini"`
- `"antigravity/brain"`
- `"POSIX-compliant"`
- `"zero leak guarantee"` / `"Zero Memory Leak Guarantees"`
- `"multi-tasking"` / `"multitasking"` (unless explicitly qualified as `"not implemented / planned in R5"`)
- `"hardware-isolated"`
- `"Monolithic Micro-Modular"`
- Unqualified claims of `"NVMe"` driver support (must only appear as Planned in R4)

```text
$ git grep -i -n -E 'all rights reserved|/home/|ultx5|\.gemini|antigravity/brain|posix-compliant|zero leak guarantee|zero memory leak guarantees|hardware-isolated|monolithic micro-modular' -- ':!docs/reports/public-readiness.md' || echo "ZERO MATCHES (PASS)"
ZERO MATCHES (PASS)

$ git grep -i -n -E 'multi-tasking|multitasking' -- ':!docs/reports/public-readiness.md'
README.md:62:- **R5 Process model**: Preemptive scheduler (multitasking is not implemented / planned in R5) with IRQ-safe locks, per-process fd table, pipes, TTY line discipline, `posix_spawn`, libc/runtime decision.
README.pt-BR.md:50:- **R5 Process model**: Escalonador preemptivo (multitasking is not implemented / planned in R5) com locks seguros contra IRQ, tabela de descritores por processo, pipes, disciplina de linha TTY, `posix_spawn`, decisão sobre libc/runtime.
docs/DECISIONS/0003-vmm-heap.md:30:- Establishes the prerequisite memory foundation for future multitasking (not implemented / planned in R5), process isolation, and dynamic device drivers.
docs/DECISIONS/0004-irq-pit-keyboard-smbios.md:33:- Sets the foundation for future storage drivers (AHCI/SATA) and preemptive multitasking (not implemented / planned in R5).
docs/DECISIONS/0010-milestone-9-mouse-window-compositor-desktop-gui.md:56:- OptiOS now possesses a dual interface: a classical, high-contrast POSIX-oriented text console by default, and a multi-window graphical desktop on demand (multitasking is not implemented / planned in R5).
docs/experiment/README.md:66:| **Overstated / Inaccurate Documentation Claims Corrected in H2** | `3` | 1. ADR 0010 (`multitasking windowed graphical desktop` corrected to `multi-window graphical desktop; multitasking not implemented / planned in R5`).<br>2. ADR 0012 (`BlockDevice` listing NVMe as implemented corrected to `Planned in R4`).<br>3. `docs/posix-foundation.md` terminology aligned with `AGENTS.md`. |
docs/known-issues.md:23:   - Preemptive multitasking, per-process `CR3` page tables, `fork()`, `wait4()`, and IPC pipes are **not implemented / planned in R5**.
docs/status.md:39:| **Preemptive Multitasking, Scheduler, Signals, Pipes & `fork`/`exec`/`wait`** | Planned (not implemented / planned in R5) | No | No | [`roadmap.md`](roadmap.md) (R5) |

$ git grep -i -n 'nvme' -- '*.md' ':!docs/reports/public-readiness.md'
README.md:61:- **R4 Storage/FS**: NVMe (native — Planned in R4), block cache, FAT32 hardening, 4Kn sector-size correctness, ext4/NTFS read-only.
README.pt-BR.md:49:- **R4 Storage/FS**: NVMe (nativo — Planned in R4), cache de blocos, endurecimento do FAT32, suporte a setores 4Kn, ext4/NTFS somente leitura.
docs/DECISIONS/0012-driver-architecture.md:72:  - Implemented by AHCI SATA drives (`AhciPort`), and Planned in R4 for NVMe controllers and RAM disks.
docs/architecture/owner-decisions.md:15:- **Native vs port**: Write natively against public specs (AHCI, xHCI, HID, and Planned in R4/R7: NVMe, HDA); port vendor-specific complex hardware (Ethernet NICs, Wi-Fi); GPU acceleration out of scope (GOP, later `virtio-gpu`).
docs/experiment/README.md:62:| **Milestone 10 Claims Audited in `10.1-verification.md`** | `12` | `6` Verified without qualification, `2` Verified with architectural caveats (#1, #6), `1` Misleading wording (#9), `1` False (#10 NVMe claim), `1` Inconsistent version string (#11), `1` Omission (#12 WM). |
docs/experiment/README.md:66:| **Overstated / Inaccurate Documentation Claims Corrected in H2** | `3` | 1. ADR 0010 (`multitasking windowed graphical desktop` corrected to `multi-window graphical desktop; multitasking not implemented / planned in R5`).<br>2. ADR 0012 (`BlockDevice` listing NVMe as implemented corrected to `Planned in R4`).<br>3. `docs/posix-foundation.md` terminology aligned with `AGENTS.md`. |
docs/known-issues.md:24:4. **No NVMe Storage Driver**:
docs/known-issues.md:25:   - PCI enumeration recognizes NVMe class codes (`01:08`), but only SATA AHCI (`01:06:01`) has an active driver. Native NVMe support is **Planned in R4**.
docs/reports/10.1-verification.md:28:| 10 | **Kernel scans SATA/NVMe storage controllers** | **FALSE** | There is **NO NVMe driver** in OptiOS. The kernel only initializes the Intel ICH9 SATA AHCI controller (`8086:2922`). NVMe is planned for Roadmap Milestone R4. |
docs/reports/10.1-verification.md:352:#### 3. NVMe Support Claims Audit
docs/reports/10.1-verification.md:353:- Milestone 10 Report (line 189): Claims the boot process `"scans the SATA/NVMe storage controllers"`.
docs/reports/10.1-verification.md:355:  - `kernel/src/drivers/pci.rs:58-59`: Class code string mapping only (`(0x01, 0x08) => "Non-Volatile Memory (NVMe)"`).
docs/reports/10.1-verification.md:356:  - **There is NO NVMe driver in OptiOS.** No PCIe NVMe queue registers, doorbells, or admin commands exist. NVMe support is scheduled for Roadmap Milestone R4.
docs/reports/10.1-verification.md:634:#### P2.2: Erroneous NVMe Claim Correction
docs/reports/10.1-verification.md:635:- **Problem**: Milestone 10 report states that OptiOS "scans SATA/NVMe storage controllers", but no NVMe driver exists.
docs/reports/10.1-verification.md:636:- **Minimal Fix**: Correct report and documentation to state "SATA AHCI storage controllers (NVMe planned for Milestone R4)".
docs/roadmap.md:11:- **R4 Storage/FS**: NVMe (native), block cache, FAT32 hardening, 4Kn sector-size correctness, ext4/NTFS read-only.
docs/status.md:38:| **NVMe Storage Driver, Block Cache, ext4/NTFS Read-Only** | Planned (Planned in R4) | No | No | [`roadmap.md`](roadmap.md) (R4) |
```

---

## Final GO / NO-GO Publication Checklist

| # | Gate Criterion | Status | Evidence / Notes |
| :---: | :--- | :---: | :--- |
| 1 | Working tree clean and isolated on `pre-public-20261111` | **PASS** | `git status` clean; `origin/master` untouched (no `git push` executed). |
| 2 | Pre-hygiene backup tarball created outside repo root | **PASS** | `../optios-pre-public-20261111.tar.gz` (`1.6 MiB`). |
| 3 | Zero secrets, tokens, or private keys in full git history | **PASS** | Verified via regex scan across `git log --all -p`. |
| 4 | Zero local paths or host usernames in tracked files or commit history | **PASS** | Zero matches for `/home/`, `ultx5`, `.gemini`, or `antigravity/brain` outside this report. |
| 5 | All 77 source/include files carry valid `SPDX-License-Identifier` | **PASS** | Verified via `./scripts/license-check.sh`. |
| 6 | Zero `.ttf`/`.otf` third-party font files embedded | **PASS** | Verified via `./scripts/license-check.sh` (`0 font files`). |
| 7 | `THIRD_PARTY_NOTICES.md` & `docs/licensing-policy.md` complete | **PASS** | Limine (`BSD-2-Clause`), `pci.ids` (`GPL-2.0-or-later OR BSD-3-Clause`), `limine`/`bitflags` (`MIT`), and `font_data.in` disclosed. |
| 8 | Sober `README.md` (EN) and `README.pt-BR.md` (PT-BR) aligned with `docs/status.md` | **PASS** | Created from Appendix A template; zero marketing claims. |
| 9 | All 180 relative Markdown links across 32 `*.md` files resolve | **PASS** | Verified via Python link resolver. |
| 10 | All 85 host unit tests and headless QEMU verification pass | **PASS** | `./scripts/test.sh` (85 passed) and `./scripts/run.sh --test` (`PASSED SUCCESSFULLY!`). |
| 11 | Formal `docs/reports/closure-audit.md` present & all P0 items closed | **CONDITIONAL / NO** | `closure-audit.md` was not present prior to H0, and item `P0.3` (ELF W^X segment permissions) remains open (documented transparently in `docs/known-issues.md`). |

**Overall Verdict**: **`READY-TO-PUBLISH: CONDITIONAL / NO`** *(All hygiene, licensing, documentation, CI, and verification gates pass; publication is conditional on the Owner Decisions listed below).*

---

## Summary of Owner Decisions Required Before Publishing

1. **Fill AI Models & Tools Placeholder in [`docs/ai-assisted-development.md`](../ai-assisted-development.md)**:
   - Replace `<owner to fill: AI models and agent tools used>` in Section 1 of `docs/ai-assisted-development.md` with the exact AI model names and agent harness descriptions you wish to disclose publicly.
2. **Review `kernel/src/drivers/video/font_data.in` Provenance**:
   - Confirm whether keeping the 95-glyph ASCII 8x16 bitmap array (`font_data.in`, derived from Terminus Font 8x16 and disclosed in `THIRD_PARTY_NOTICES.md` Section 1.3) is acceptable, or whether you want it replaced with a public-domain IBM VGA 8x16 bitmap table before flipping repository visibility.
3. **Accept Open `P0.3` / `P1` Audit Items as Documented Known Issues (or Schedule a Code Fix Phase First)**:
   - `docs/known-issues.md` and `SECURITY.md` openly document the remaining items from `10.1-verification.md` (including `P0.3` ELF segment W^X and `P2.3` `OptiOS 1.0` vs `0.10.0-alpha` MOTD string). Decide whether to publish with these transparently documented as early-alpha known issues or resolve `P0.3` / `P2.3` in a code phase before making the repository public.
4. **Merge and Push (Owner-Only Action)**:
   - Review the commits on `pre-public-20261111` (`git log master..pre-public-20261111`), merge into `master` when satisfied, push to `origin`, and enable GitHub Private Vulnerability Reporting in the repository Security settings.


