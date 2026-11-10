<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Virtual File System (VFS) Operational Semantics & Error Specification
// Date: 2026-11-09
-->

# OptiOS VFS Semantics & Operational Specification

## 1. Overview

The OptiOS Virtual File System (VFS) provides an in-memory, read-only namespace mounted at the root (`/`), backed directly by the USTAR initramfs archive provided at boot by Limine.

The VFS currently exposes an internal kernel API (`vfs::open`, `vfs::read`, `vfs::stat`, `vfs::list_dir`). This interface is **POSIX-oriented** in its naming and error conventions, but differs fundamentally from a full POSIX userspace file descriptor runtime.

---

## 2. Fundamental Architectural Differences from POSIX

| Dimension | POSIX Standard (`open`/`read`/`close`) | Current OptiOS VFS Implementation |
|---|---|---|
| **File Descriptor (`fd`)** | Small integer indexing a per-process descriptor table, pointing to an open file description. | Direct `usize` index into the global kernel node array (`VFS_TABLE.nodes`). |
| **Open Flags** | Bitmask (`O_RDONLY`, `O_WRONLY`, `O_NONBLOCK`, `O_CLOEXEC`, etc.). | None. All files are implicitly opened in read-only mode. |
| **File Offset State** | Maintained per open file description; updated sequentially by `read()` and `write()`. | **Stateless.** No offset is stored inside the kernel. |
| **Read Semantics** | Stream-oriented: `read(fd, buf, count)` advances implicit file offset. | **Pread-like:** `read(fd, offset, buf)` requires explicit `offset` parameter on each invocation. |
| **Resource Lifetime** | Explicit `close(fd)` deallocates the descriptor and decrements reference count. | **Static.** Nodes and data slices are backed by static physical memory (`&'static [u8]`). No `close()` exists. |
| **File Positioning** | Arbitrary positioning via `lseek(fd, offset, whence)`. | **None.** No `lseek()` function exists; callers provide offsets directly to `read()`. |
| **File Modification** | Creation (`creat`, `O_CREAT`), writes (`write`), truncation, deletion (`unlink`). | **Read-Only by construction.** No mutation entry points exist. |

---

## 3. Function Specifications & Error Tables

### 3.1 `vfs::open`

```rust
pub fn open(path: &str) -> KernelResult<usize>
```

Resolves `path` against the mounted initramfs archive and returns the global node index (`fd`).

#### Error Conditions

| `Errno` | Condition |
|---|---|
| `EINVAL` | `path` is empty (`""`), contains embedded NUL bytes, or attempts path traversal using `..` components (e.g. `/etc/../motd`). |
| `ENAMETOOLONG` | Canonical path exceeds the maximum supported path length of 255 bytes. |
| `EISDIR` | Target `path` is a directory (directories cannot be opened via `vfs::open`; use `vfs::list_dir`). |
| `ENOENT` | File path does not match any entry in the mounted initramfs archive. |

---

### 3.2 `vfs::read`

```rust
pub fn read(fd: usize, offset: usize, buf: &mut [u8]) -> KernelResult<usize>
```

Performs a pread-like read of up to `buf.len()` bytes from the node identified by `fd` starting at byte `offset`. Returns the number of bytes successfully copied into `buf`.

#### Semantics
- If `offset >= node.data.len()`, returns `Ok(0)` (standard EOF indication).
- If `offset < node.data.len()`, copies `min(node.data.len() - offset, buf.len())` bytes.
- Does not modify any kernel state or internal offset.

#### Error Conditions

| `Errno` | Condition |
|---|---|
| `EBADF` | `fd` does not correspond to a valid node index (`fd >= VFS_TABLE.nodes.len()`). |
| `EISDIR` | `fd` corresponds to a directory node rather than a regular file. |

---

### 3.3 `vfs::stat`

```rust
pub fn stat(path: &str) -> KernelResult<VfsStat>
```

Queries metadata for `path`. Returns `VfsStat { path, file_type, size, mode }`.

#### Semantics
- `stat("/")` always succeeds, returning a synthetic root directory entry (`FileType::Directory`, `size: 0`, `mode: 0o755`).
- For any other path, normalizes the path and searches the mounted initramfs node table.

#### Error Conditions

| `Errno` | Condition |
|---|---|
| `EINVAL` | `path` is empty, contains embedded NUL bytes, or attempts path traversal using `..` components. |
| `ENAMETOOLONG` | Canonical path exceeds 255 bytes. |
| `ENOENT` | Path does not match the root directory or any mounted entry. |

---

### 3.4 `vfs::list_dir`

```rust
pub fn list_dir(dir_path: &str) -> KernelResult<Vec<VfsStat>>
```

Enumerates entries located under `dir_path`.

#### Semantics
- When called with `"/"`, returns metadata for all nodes mounted in the initramfs.
- When called with a directory path (e.g. `"/etc"`), returns all nodes whose canonical path begins with that directory prefix.

#### Error Conditions

| `Errno` | Condition |
|---|---|
| `EINVAL` | `dir_path` is empty, contains embedded NUL bytes, or attempts path traversal using `..`. |
| `ENAMETOOLONG` | Directory path exceeds 255 bytes. |
| `ENOENT` | No nodes match the specified directory prefix (for paths other than `"/"`). |

---

## 4. Mount Failure Semantics

When `vfs::init(archive_opt)` is called during boot:
1. If the bootloader provides no module or `tarfs::parse_ustar_archive` fails (due to invalid magic, bad checksum, truncation, etc.), the parser logs a warning via `warn!` detailing the failure and mapped `Errno`.
2. The parser returns an empty node list (`Vec::new()`).
3. The VFS initializes with **0 nodes** and logs an informational warning.
4. **The kernel does NOT panic.** The system continues booting into the diagnostic environment. Subsequent lookups for files (such as `/etc/optios-release`) gracefully return `Err(Errno::ENOENT)`, and dashboard tabs display appropriate fallback placeholders.
