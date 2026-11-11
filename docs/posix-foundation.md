<!--
// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: POSIX Architectural Foundation Design Note
// Date: 2026-11-11
-->

# OptiOS POSIX Foundation — Architectural Design Note

## 1. Scope & Terminology

This document establishes the architectural roadmap for transitioning from OptiOS's current internal VFS API to a true POSIX-style I/O subsystem.

The current implementation is strictly **POSIX-oriented** in its concepts, naming, and error number assignments, and is not certified for full POSIX compliance. Full POSIX compatibility requires user-space processes, address space isolation, file descriptor inheritance, and standard system call interfaces, which are scheduled for subsequent milestones.

---

## 2. File Descriptors vs. Open File Descriptions

In POSIX (IEEE Std 1003.1), file I/O involves three distinct layers:

```
[ User Process ]
       |
       v (File Descriptor: 0, 1, 2...)
+------------------------------------------+
|      Per-Process File Descriptor Table   |  (Local to Process / Thread Group)
|   - fd flags (e.g., FD_CLOEXEC)          |
|   - Pointer to Open File Description     |
+------------------------------------------+
       |
       | (Shared via dup(), fcntl(), fork())
       v
+------------------------------------------+
|         Open File Description            |  (System-Wide Table / Reference Counted)
|   - Current file offset (cursor)         |
|   - File status flags (O_RDONLY, etc.)   |
|   - Access mode and lock state           |
|   - Reference count                      |
|   - Pointer to VFS Node (vnode / inode)  |
+------------------------------------------+
       |
       v
+------------------------------------------+
|             VFS Inode / Node             |  (Underlying Storage or Memory Entity)
|   - File type and permissions            |
|   - Size, timestamps, backing data       |
+------------------------------------------+
```

### 2.1 The Role of Open File Descriptions

The separation between the per-process file descriptor table and system-wide open file descriptions is fundamental:
- **Shared Offsets:** When a process calls `dup()` or forks a child, the new file descriptor points to the *same* open file description. As a result, subsequent `read()` calls in either context advance the shared file cursor.
- **Independent Open Calls:** When two separate calls to `open()` target the same path, they create two *distinct* open file descriptions pointing to the same underlying inode. Each maintains its own independent offset.

### 2.2 Why Current OptiOS Cannot Have Real `read()` or `lseek()`

In the current milestone:
1. No processes or threads exist; execution runs within a single kernel context.
2. File descriptors returned by `vfs::open()` are direct indices into a static array of initramfs nodes.
3. Because no open file descriptions exist, there is no entity where an active file offset could be stored. Implementing a global cursor per file node would break concurrency and violate POSIX semantics (multiple opens of the same file would inadvertently share an offset).
4. Consequently, OptiOS currently implements a `pread`-like signature (`vfs::read(fd, offset, buf)`), deferring sequential `read(fd, buf, count)` and `lseek(fd, offset, whence)` until open file descriptions are introduced.

---

## 3. Pre-Requisites for Real File I/O (`read`, `close`, `lseek`)

Before OptiOS can provide standard stream-based POSIX file descriptors, the kernel must introduce:

1. **Open File Description Structure (`FileDescription`):**
   - Reference counter (`AtomicUsize`).
   - File status flags (`u32`).
   - Monotonic 64-bit seek position / offset with atomic or spinlock protection.
   - Reference or pointer to the underlying filesystem node.
2. **Per-Process File Descriptor Table (`FdTable`):**
   - Array or dynamic vector of descriptor entries (`Option<Arc<FileDescription>>`).
   - Per-descriptor flags (`FD_CLOEXEC`).
   - Allocation policy (always selecting the lowest available integer $\ge 0$).
3. **Descriptor Lifecycle Primitives:**
   - Real `close(fd)` decrementing the reference count and releasing the open file description when count reaches zero.
   - `dup()` and `dup2()` duplicating descriptor table entries while preserving the referenced description.

---

## 4. Deferrals to Process & Syscall Milestones

The following capabilities are explicitly deferred until user processes and system calls are implemented:

- **System Call ABI:** Registers, calling convention, and return semantics (e.g. negative errno vs. carry flag vs. separate error register) for the x86-64 `syscall` / `sysretq` instruction.
- **Process Memory Isolation:** Copying buffers between user virtual space and kernel space (`copy_to_user`, `copy_from_user`) with page fault verification and pointer sanitization.
- **File Descriptor Inheritance:** Duplicating descriptor tables on `fork()` and closing descriptors with `FD_CLOEXEC` on `execve()`.
- **Standard Streams:** Automatic instantiation of descriptors `0` (stdin), `1` (stdout), and `2` (stderr) bound to terminal / console devices.
