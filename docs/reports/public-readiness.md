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
