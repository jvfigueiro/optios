// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: POSIX.1-1988 USTAR Initramfs Archive Parser Adapter
// Date: 2026-11-09

//! USTAR in-memory archive parser adapter for OptiOS VFS.
//! Integrates the hardened `optios-ustar` library and maps `TarEntry` to `VfsNode`.

extern crate alloc;
use alloc::vec::Vec;
use optios_ustar::{parse_ustar_archive as lib_parse, TarEntryKind, TarError};
use crate::fs::vfs::{FileType, VfsNode};
use crate::posix::errno::Errno;
use crate::warn;

/// Maps a TarError into an Errno for the VFS layer per Milestone 5.1 POSIX specification.
pub fn tar_error_to_errno(err: TarError) -> Errno {
    match err {
        // Path length limit
        TarError::PathTooLong => Errno::ENAMETOOLONG,

        // Invalid path / syntax / magic
        TarError::InvalidPath
        | TarError::InvalidUtf8
        | TarError::BadMagic
        | TarError::BadVersion
        | TarError::NonOctalField
        | TarError::EmptyNumericField
        | TarError::JunkAfterTerminator
        | TarError::HighBitSet => Errno::EINVAL,

        // Arithmetic overflow in octal conversion
        TarError::NumericOverflow => Errno::EOVERFLOW,

        // Archive corruption, checksum mismatch, or boundary truncation
        TarError::BadChecksum
        | TarError::UnexpectedEof
        | TarError::TruncatedArchive
        | TarError::SizeOutOfBounds => Errno::EIO,

        // Unsupported tar entry types
        TarError::UnsupportedType => Errno::ENOSYS,
    }
}

/// Re-exports normalize_path from optios-ustar for path handling in VFS.
pub fn normalize_path(prefix: &str, name: &str) -> Result<alloc::string::String, Errno> {
    optios_ustar::normalize_path(prefix, name).map_err(tar_error_to_errno)
}

/// Parses a raw USTAR archive slice in memory and returns all discovered VFS nodes.
pub fn parse_ustar_archive(archive: &'static [u8]) -> Vec<VfsNode> {
    match lib_parse(archive) {
        Ok(entries) => {
            let mut nodes = Vec::with_capacity(entries.len());
            for entry in entries {
                let file_type = match entry.kind {
                    TarEntryKind::RegularFile => FileType::RegularFile,
                    TarEntryKind::Directory => FileType::Directory,
                };
                nodes.push(VfsNode {
                    path: entry.path,
                    file_type,
                    size: entry.size,
                    mode: entry.mode,
                    data: entry.data,
                });
            }
            nodes
        }
        Err(err) => {
            warn!("USTAR Initramfs parse failed: {:?} (mapped to {})", err, tar_error_to_errno(err));
            Vec::new()
        }
    }
}
