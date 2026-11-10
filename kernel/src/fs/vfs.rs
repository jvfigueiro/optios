// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: POSIX Virtual File System (VFS) Core Interface & Mount Table
// Date: 2026-11-10

//! Virtual File System (VFS) interface providing POSIX-like `open`, `read`, `stat`, and `list_dir`
//! primitives over the in-memory USTAR Initramfs mounted at `/`, virtual pseudo-filesystems (`/proc`),
//! and block-device filesystems (FAT32 mounted at `/mnt/sda1`).

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};

use optios_ahci_proto::BlockDevice;
use optios_fat32::{Fat32Error, Fat32FileSystem, PartitionBlockDevice};

use crate::fs::proc::{self, ProcFileType};
use crate::fs::tarfs;
use crate::posix::errno::{Errno, KernelResult};
use crate::{info, warn};

pub const FAT_FD_BASE: usize = 0x4000_0000;
pub const PROC_FD_BASE: usize = 0x8000_0000;

pub const MNT_RDONLY: u32 = 1 << 0;
pub const MNT_NOEXEC: u32 = 1 << 1;

/// Returns the mount flags associated with the filesystem serving `path`.
pub fn get_mount_flags(path: &str) -> u32 {
    let clean = path.trim_start_matches('/');
    if clean.starts_with("mnt/") || clean == "mnt" {
        MNT_RDONLY | MNT_NOEXEC
    } else if clean.starts_with("proc/") || clean == "proc" {
        MNT_RDONLY | MNT_NOEXEC
    } else {
        // Root initramfs is read-only, but executable
        MNT_RDONLY
    }
}

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

// ---------------------------------------------------------------------------
// FAT32 Storage Mount Integration
// ---------------------------------------------------------------------------

pub struct KernelAhciDevice {
    pub port: &'static crate::drivers::ahci::AhciPort,
}

impl BlockDevice for KernelAhciDevice {
    fn read_sectors(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), optios_ahci_proto::AhciError> {
        unsafe { self.port.read_sectors(lba, count, buf) }
    }

    fn sector_size(&self) -> u32 {
        512
    }

    fn total_sectors(&self) -> u64 {
        self.port.total_sectors
    }
}

type MountedFat32 = Fat32FileSystem<PartitionBlockDevice<KernelAhciDevice>>;

struct FatFsMount {
    fs: UnsafeCell<Option<MountedFat32>>,
    lock: AtomicBool,
}

unsafe impl Sync for FatFsMount {}

static FAT_FS: FatFsMount = FatFsMount {
    fs: UnsafeCell::new(None),
    lock: AtomicBool::new(false),
};

fn with_fat_fs<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut MountedFat32) -> R,
{
    while FAT_FS
        .lock
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }
    let fs_opt = unsafe { &mut *FAT_FS.fs.get() };
    let res = fs_opt.as_mut().map(f);
    FAT_FS.lock.store(false, Ordering::Release);
    res
}

struct OpenFatFile {
    path: String,
    size: usize,
}

struct FatTable {
    files: UnsafeCell<Vec<Option<OpenFatFile>>>,
    lock: AtomicBool,
}

unsafe impl Sync for FatTable {}

static FAT_TABLE: FatTable = FatTable {
    files: UnsafeCell::new(Vec::new()),
    lock: AtomicBool::new(false),
};

/// Mounts a FAT32 partition from an AHCI disk port onto the VFS namespace at `/mnt/sda1`.
pub fn mount_fat32_sda1(
    port: &'static crate::drivers::ahci::AhciPort,
    start_lba: u64,
    total_sectors: u64,
) -> KernelResult<()> {
    let dev = KernelAhciDevice { port };
    let part = PartitionBlockDevice::new(dev, start_lba, total_sectors);
    match Fat32FileSystem::new(part) {
        Ok(fs) => {
            while FAT_FS
                .lock
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                core::hint::spin_loop();
            }
            unsafe {
                *FAT_FS.fs.get() = Some(fs);
            }
            FAT_FS.lock.store(false, Ordering::Release);
            info!(
                "VFS: Mounted FAT32 filesystem on /dev/sda1 (start LBA {}, {} sectors) at '/mnt/sda1' (Read-Only, noexec).",
                start_lba, total_sectors
            );
            Ok(())
        }
        Err(_) => {
            warn!("VFS: Failed to mount FAT32 filesystem on /dev/sda1.");
            Err(Errno::EIO)
        }
    }
}

// ---------------------------------------------------------------------------
// VFS Core Types and Table
// ---------------------------------------------------------------------------

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
    if path == "/proc" || path == "/proc/" || path == "/mnt" || path == "/mnt/" || path == "/mnt/sda1" || path == "/mnt/sda1/" {
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

    if path.starts_with("/mnt/sda1/") {
        let rel = &path["/mnt/sda1/".len()..];
        let stat_res = with_fat_fs(|fs| fs.stat(rel));
        match stat_res {
            Some(Ok(entry)) => {
                if entry.is_dir {
                    return Err(Errno::EISDIR);
                }
                while FAT_TABLE
                    .lock
                    .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                    .is_err()
                {
                    core::hint::spin_loop();
                }
                let files = unsafe { &mut *FAT_TABLE.files.get() };
                let mut found_idx = None;
                for (i, slot) in files.iter_mut().enumerate() {
                    if slot.is_none() {
                        *slot = Some(OpenFatFile {
                            path: String::from(rel),
                            size: entry.size as usize,
                        });
                        found_idx = Some(i);
                        break;
                    }
                }
                let idx = match found_idx {
                    Some(i) => i,
                    None => {
                        files.push(Some(OpenFatFile {
                            path: String::from(rel),
                            size: entry.size as usize,
                        }));
                        files.len() - 1
                    }
                };
                FAT_TABLE.lock.store(false, Ordering::Release);
                return Ok(FAT_FD_BASE + idx);
            }
            Some(Err(Fat32Error::NotFound)) => return Err(Errno::ENOENT),
            Some(Err(Fat32Error::NotAFile)) => return Err(Errno::EISDIR),
            Some(Err(_)) => return Err(Errno::EIO),
            None => return Err(Errno::ENOENT),
        }
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
    } else if fd >= FAT_FD_BASE {
        let idx = fd - FAT_FD_BASE;
        while FAT_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let files = unsafe { &mut *FAT_TABLE.files.get() };
        if let Some(slot) = files.get_mut(idx) {
            *slot = None;
        }
        FAT_TABLE.lock.store(false, Ordering::Release);
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

    if fd >= FAT_FD_BASE {
        let idx = fd - FAT_FD_BASE;
        while FAT_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let files = unsafe { &*FAT_TABLE.files.get() };
        let file_path = files.get(idx).and_then(|opt| opt.as_ref()).map(|f| f.path.clone());
        FAT_TABLE.lock.store(false, Ordering::Release);

        let Some(rel_path) = file_path else {
            return Err(Errno::EBADF);
        };

        let read_res = with_fat_fs(|fs| fs.read_file(&rel_path, offset, buf));
        return match read_res {
            Some(Ok(bytes_read)) => Ok(bytes_read),
            Some(Err(Fat32Error::NotFound)) => Err(Errno::ENOENT),
            Some(Err(_)) => Err(Errno::EIO),
            None => Err(Errno::EIO),
        };
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

    if path == "/mnt" || path == "/mnt/" {
        return Ok(VfsStat {
            path: String::from("/mnt"),
            file_type: FileType::Directory,
            size: 0,
            mode: 0o755,
        });
    }

    if path == "/mnt/sda1" || path == "/mnt/sda1/" {
        return Ok(VfsStat {
            path: String::from("/mnt/sda1"),
            file_type: FileType::Directory,
            size: 0,
            mode: 0o755,
        });
    }

    if path.starts_with("/mnt/sda1/") {
        let rel = &path["/mnt/sda1/".len()..];
        let stat_res = with_fat_fs(|fs| fs.stat(rel));
        return match stat_res {
            Some(Ok(entry)) => Ok(VfsStat {
                path: String::from(path),
                file_type: if entry.is_dir {
                    FileType::Directory
                } else {
                    FileType::RegularFile
                },
                size: entry.size as usize,
                mode: if entry.is_dir { 0o755 } else { 0o644 },
            }),
            Some(Err(Fat32Error::NotFound)) => Err(Errno::ENOENT),
            Some(Err(_)) => Err(Errno::EIO),
            None => Err(Errno::ENOENT),
        };
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

    if dir_path == "/mnt" || dir_path == "/mnt/" {
        return Ok(alloc::vec![VfsStat {
            path: String::from("/mnt/sda1"),
            file_type: FileType::Directory,
            size: 0,
            mode: 0o755,
        }]);
    }

    if dir_path == "/mnt/sda1" || dir_path == "/mnt/sda1/" {
        let res = with_fat_fs(|fs| fs.list_dir("/"));
        return match res {
            Some(Ok(entries)) => {
                let mut list = Vec::new();
                for entry in entries {
                    list.push(VfsStat {
                        path: format!("/mnt/sda1/{}", entry.name),
                        file_type: if entry.is_dir {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        },
                        size: entry.size as usize,
                        mode: if entry.is_dir { 0o755 } else { 0o644 },
                    });
                }
                Ok(list)
            }
            Some(Err(_)) => Err(Errno::EIO),
            None => Err(Errno::ENOENT),
        };
    }

    if dir_path.starts_with("/mnt/sda1/") {
        let rel = &dir_path["/mnt/sda1/".len()..];
        let res = with_fat_fs(|fs| fs.list_dir(rel));
        return match res {
            Some(Ok(entries)) => {
                let mut list = Vec::new();
                for entry in entries {
                    list.push(VfsStat {
                        path: format!("{}/{}", dir_path.trim_end_matches('/'), entry.name),
                        file_type: if entry.is_dir {
                            FileType::Directory
                        } else {
                            FileType::RegularFile
                        },
                        size: entry.size as usize,
                        mode: if entry.is_dir { 0o755 } else { 0o644 },
                    });
                }
                Ok(list)
            }
            Some(Err(Fat32Error::NotFound)) => Err(Errno::ENOENT),
            Some(Err(_)) => Err(Errno::EIO),
            None => Err(Errno::ENOENT),
        };
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
            results.push(VfsStat {
                path: String::from("/mnt"),
                file_type: FileType::Directory,
                size: 0,
                mode: 0o755,
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
    } else if fd >= FAT_FD_BASE {
        let idx = fd - FAT_FD_BASE;
        while FAT_TABLE
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
        let files = unsafe { &*FAT_TABLE.files.get() };
        let sz = files.get(idx).and_then(|opt| opt.as_ref()).map(|f| f.size);
        FAT_TABLE.lock.store(false, Ordering::Release);
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
