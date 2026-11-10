// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Filesystem & VFS Subsystem Module
// Date: 2026-11-10

//! Virtual File System (VFS) and filesystem drivers (USTAR Initramfs, /proc).

pub mod proc;
pub mod tarfs;
pub mod vfs;
