// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: POSIX Virtual File System (VFS) Core Interface
// Date: 2026-11-09

//! Virtual File System (VFS) interface providing POSIX-like `open`, `read`, `stat`, and `list_dir`
//! primitives over the in-memory USTAR Initramfs mounted at `/`.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::fs::tarfs;
use crate::posix::errno::{Errno, KernelResult};
use crate::{info, warn};

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

/// Reads up to `buf.len()` bytes from an open file descriptor `fd` starting at `offset`.
pub fn read(fd: usize, offset: usize, buf: &mut [u8]) -> KernelResult<usize> {
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
        if results.is_empty() && canonical != "/" {
            return Err(Errno::ENOENT);
        }
        Ok(results)
    })
}

/// Helper that opens, stats, and reads an entire text file into a `String` using the POSIX VFS API.
pub fn read_to_string(path: &str) -> KernelResult<String> {
    let fd = open(path)?;
    let st = stat(path)?;
    let mut buf = alloc::vec![0u8; st.size];
    let bytes_read = read(fd, 0, &mut buf)?;
    buf.truncate(bytes_read);
    let s = core::str::from_utf8(&buf).map_err(|_| Errno::EINVAL)?;
    Ok(String::from(s))
}
