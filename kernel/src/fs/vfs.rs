// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: POSIX Virtual File System (VFS) Core Interface
// Date: 2026-11-10

//! Virtual File System (VFS) interface providing POSIX-like `open`, `read`, `stat`, and `list_dir`
//! primitives over the in-memory USTAR Initramfs mounted at `/`.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::fs::proc::{self, ProcFileType};
use crate::fs::tarfs;
use crate::posix::errno::{Errno, KernelResult};
use crate::{info, warn};

pub const PROC_FD_BASE: usize = 0x8000_0000;

struct OpenProc {
    content: Vec<u8>,
}

struct ProcTable {
    procs: UnsafeCell<Vec<Option<OpenProc>>>,
    lock: AtomicBool,
}

unsafe impl Sync for ProcTable {}

static PROC_TABLE: ProcTable = ProcTable {
    procs: UnsafeCell::new(Vec::new()),
    lock: AtomicBool::new(false),
};

/// Type of a node in the Virtual File System.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileType {
    RegularFile,
    Directory,
}

impl FileType {
    pub const fn as_str(&self) -> &'static str {
        match self {
            FileType::RegularFile => "FILE",
            FileType::Directory => "DIR ",
        }
    }
}

/// Public metadata structure returned by `vfs::stat` and `vfs::list_dir`.
#[derive(Debug, Clone)]
pub struct VfsStat {
    pub path: String,
    pub file_type: FileType,
    pub size: usize,
    pub mode: u32,
}

/// Internal in-memory node backed by the USTAR initramfs archive.
pub struct VfsNode {
    pub path: String,
    pub file_type: FileType,
    pub size: usize,
    pub mode: u32,
    pub data: &'static [u8],
}

struct VfsTable {
    nodes: UnsafeCell<Vec<VfsNode>>,
    lock: AtomicBool,
}

unsafe impl Sync for VfsTable {}

static VFS_TABLE: VfsTable = VfsTable {
    nodes: UnsafeCell::new(Vec::new()),
    lock: AtomicBool::new(false),
};

fn with_vfs<F, R>(f: F) -> R
where
    F: FnOnce(&mut Vec<VfsNode>) -> R,
{
    while VFS_TABLE
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let nodes = unsafe { &mut *VFS_TABLE.nodes.get() };
    let result = f(nodes);
    VFS_TABLE.lock.store(false, Ordering::Release);
    result
}

/// Initializes the root VFS (`/`) from the USTAR initramfs archive provided by Limine.
pub fn init(archive_opt: Option<(&'static str, &'static [u8])>) -> usize {
    let Some((boot_path, archive)) = archive_opt else {
        warn!("VFS: No initramfs module provided by bootloader!");
        return 0;
    };

    let parsed = tarfs::parse_ustar_archive(archive);
    let count = parsed.len();

    with_vfs(|nodes| {
        *nodes = parsed;
    });

    info!(
        "VFS initialized: Mounted USTAR initramfs '{}' ({} bytes, {} nodes) at '/' (Read-Only).",
        boot_path,
        archive.len(),
        count
    );

    count
}

/// Opens a file by path in Read-Only mode and returns its descriptor index (`fd`).
pub fn open(path: &str) -> KernelResult<usize> {
    if path == "/proc" || path == "/proc/" {
        return Err(Errno::EISDIR);
    }

    if let Some(proc_file) = ProcFileType::from_path(path) {
        let content = proc::generate_proc_content(proc_file).into_bytes();
        while PROC_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let procs = unsafe { &mut *PROC_TABLE.procs.get() };
        let mut found_idx = None;
        for (i, slot) in procs.iter_mut().enumerate() {
            if slot.is_none() {
                *slot = Some(OpenProc { content: content.clone() });
                found_idx = Some(i);
                break;
            }
        }
        let idx = match found_idx {
            Some(i) => i,
            None => {
                procs.push(Some(OpenProc { content }));
                procs.len() - 1
            }
        };
        PROC_TABLE.lock.store(false, Ordering::Release);
        return Ok(PROC_FD_BASE + idx);
    }

    let canonical = tarfs::normalize_path("", path)?;
    with_vfs(|nodes| {
        for (idx, node) in nodes.iter().enumerate() {
            if node.path == canonical {
                if node.file_type == FileType::Directory {
                    return Err(Errno::EISDIR);
                }
                return Ok(idx);
            }
        }
        Err(Errno::ENOENT)
    })
}

/// Closes an open file descriptor.
pub fn close(fd: usize) -> KernelResult<()> {
    if fd >= PROC_FD_BASE {
        let idx = fd - PROC_FD_BASE;
        while PROC_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let procs = unsafe { &mut *PROC_TABLE.procs.get() };
        if let Some(slot) = procs.get_mut(idx) {
            *slot = None;
        }
        PROC_TABLE.lock.store(false, Ordering::Release);
        Ok(())
    } else {
        Ok(())
    }
}

/// Reads up to `buf.len()` bytes from an open file descriptor `fd` starting at `offset`.
pub fn read(fd: usize, offset: usize, buf: &mut [u8]) -> KernelResult<usize> {
    if fd >= PROC_FD_BASE {
        let idx = fd - PROC_FD_BASE;
        while PROC_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let procs = unsafe { &*PROC_TABLE.procs.get() };
        let res = match procs.get(idx).and_then(|opt| opt.as_ref()) {
            Some(open_proc) => {
                if offset >= open_proc.content.len() {
                    Ok(0)
                } else {
                    let available = open_proc.content.len() - offset;
                    let to_copy = available.min(buf.len());
                    buf[..to_copy].copy_from_slice(&open_proc.content[offset..offset + to_copy]);
                    Ok(to_copy)
                }
            }
            None => Err(Errno::EBADF),
        };
        PROC_TABLE.lock.store(false, Ordering::Release);
        return res;
    }

    with_vfs(|nodes| {
        let node = nodes.get(fd).ok_or(Errno::EBADF)?;
        if node.file_type == FileType::Directory {
            return Err(Errno::EISDIR);
        }
        if offset >= node.data.len() {
            return Ok(0);
        }
        let available = node.data.len() - offset;
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&node.data[offset..offset + to_copy]);
        Ok(to_copy)
    })
}

/// Retrieves metadata (`VfsStat`) for the given path.
pub fn stat(path: &str) -> KernelResult<VfsStat> {
    if path == "/proc" || path == "/proc/" {
        return Ok(VfsStat {
            path: String::from("/proc"),
            file_type: FileType::Directory,
            size: 0,
            mode: 0o555,
        });
    }

    if let Some(proc_file) = ProcFileType::from_path(path) {
        let content = proc::generate_proc_content(proc_file);
        return Ok(VfsStat {
            path: String::from(proc_file.path()),
            file_type: FileType::RegularFile,
            size: content.len(),
            mode: 0o444,
        });
    }

    let canonical = if path == "/" {
        String::from("/")
    } else {
        tarfs::normalize_path("", path)?
    };
    if canonical == "/" {
        return Ok(VfsStat {
            path: String::from("/"),
            file_type: FileType::Directory,
            size: 0,
            mode: 0o755,
        });
    }

    with_vfs(|nodes| {
        for node in nodes.iter() {
            if node.path == canonical {
                return Ok(VfsStat {
                    path: node.path.clone(),
                    file_type: node.file_type,
                    size: node.size,
                    mode: node.mode,
                });
            }
        }
        Err(Errno::ENOENT)
    })
}

/// Lists entries mounted in the VFS under `dir_path` (use `"/"` to list all mounted initramfs nodes).
pub fn list_dir(dir_path: &str) -> KernelResult<Vec<VfsStat>> {
    if dir_path == "/proc" || dir_path == "/proc/" {
        let mut results = Vec::new();
        for (p, _) in proc::get_proc_file_list() {
            if let Some(proc_file) = ProcFileType::from_path(p) {
                let sz = proc::generate_proc_content(proc_file).len();
                results.push(VfsStat {
                    path: String::from(p),
                    file_type: FileType::RegularFile,
                    size: sz,
                    mode: 0o444,
                });
            }
        }
        return Ok(results);
    }

    let canonical = if dir_path == "/" {
        String::from("/")
    } else {
        tarfs::normalize_path("", dir_path)?
    };
    with_vfs(|nodes| {
        let mut results = Vec::new();
        for node in nodes.iter() {
            let matches = if canonical == "/" {
                true
            } else {
                node.path.starts_with(&canonical) && node.path != canonical
            };
            if matches {
                results.push(VfsStat {
                    path: node.path.clone(),
                    file_type: node.file_type,
                    size: node.size,
                    mode: node.mode,
                });
            }
        }
        if canonical == "/" {
            results.push(VfsStat {
                path: String::from("/proc"),
                file_type: FileType::Directory,
                size: 0,
                mode: 0o555,
            });
        }
        if results.is_empty() && canonical != "/" {
            return Err(Errno::ENOENT);
        }
        Ok(results)
    })
}

/// Returns the total size of an open file descriptor.
pub fn file_size(fd: usize) -> Option<usize> {
    if fd >= PROC_FD_BASE {
        let idx = fd - PROC_FD_BASE;
        while PROC_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let procs = unsafe { &*PROC_TABLE.procs.get() };
        let sz = procs.get(idx).and_then(|opt| opt.as_ref()).map(|p| p.content.len());
        PROC_TABLE.lock.store(false, Ordering::Release);
        sz
    } else {
        with_vfs(|nodes| nodes.get(fd).map(|n| n.size))
    }
}

/// Helper that opens, stats, and reads an entire text file into a `String` using the POSIX VFS API.
#[allow(dead_code)]
pub fn read_to_string(path: &str) -> KernelResult<String> {
    let fd = open(path)?;
    let st = stat(path)?;
    let mut buf = alloc::vec![0u8; st.size];
    let bytes_read = read(fd, 0, &mut buf)?;
    buf.truncate(bytes_read);
    let s = core::str::from_utf8(&buf).map_err(|_| Errno::EINVAL)?;
    Ok(String::from(s))
}
