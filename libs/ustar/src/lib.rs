// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: USTAR In-Memory Archive Parser Library (Hardened)
// Date: 2026-11-09

//! Hardened USTAR (POSIX.1-1988) in-memory archive parser library for OptiOS.
//!
//! # Typeflag Behavior
//! The parser strictly handles:
//! - `b'0'` and `b'\0'`: Regular files (represented as `TarEntryKind::RegularFile`).
//! - `b'5'`: Directories (represented as `TarEntryKind::Directory`).
//!
//! All other typeflags are rejected or safely ignored without following:
//! - Hard links (`b'1'`) and Symbolic links (`b'2'`): Ignored without traversing or resolving targets.
//! - Character devices (`b'3'`), Block devices (`b'4'`), and FIFOs (`b'6'`): Ignored.
//!
//! No links are ever dereferenced or followed during archive parsing.

#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

pub const TAR_BLOCK_SIZE: usize = 512;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarEntryKind {
    RegularFile,
    Directory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TarEntry<'a> {
    pub path: String,
    pub kind: TarEntryKind,
    pub size: usize,
    pub mode: u32,
    pub data: &'a [u8],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TarError {
    BadChecksum,
    BadMagic,
    BadVersion,
    NonOctalField,
    EmptyNumericField,
    NumericOverflow,
    JunkAfterTerminator,
    HighBitSet,
    UnexpectedEof,
    TruncatedArchive,
    SizeOutOfBounds,
    InvalidPath,
    PathTooLong,
    InvalidUtf8,
    UnsupportedType,
}

/// A3: Parses an octal field with checked arithmetic, rejecting high-bit (base-256),
/// empty fields, non-octal characters, and trailing junk.
pub fn parse_octal(bytes: &[u8]) -> Result<usize, TarError> {
    if bytes.is_empty() {
        return Err(TarError::EmptyNumericField);
    }

    // Check for high-bit (GNU tar base-256 binary encoding)
    if (bytes[0] & 0x80) != 0 {
        return Err(TarError::HighBitSet);
    }

    let mut i = 0;
    // Skip leading spaces
    while i < bytes.len() && bytes[i] == b' ' {
        i += 1;
    }

    if i == bytes.len() {
        return Err(TarError::EmptyNumericField);
    }

    let mut val: usize = 0;
    let mut digits_found = false;

    while i < bytes.len() {
        let b = bytes[i];
        if b == 0 || b == b' ' {
            // Reached terminator. Ensure remainder has no junk.
            for &rem in &bytes[i..] {
                if rem != 0 && rem != b' ' {
                    return Err(TarError::JunkAfterTerminator);
                }
            }
            break;
        }

        if (b'0'..=b'7').contains(&b) {
            digits_found = true;
            let digit = (b - b'0') as usize;
            val = val.checked_mul(8).ok_or(TarError::NumericOverflow)?;
            val = val.checked_add(digit).ok_or(TarError::NumericOverflow)?;
        } else {
            return Err(TarError::NonOctalField);
        }
        i += 1;
    }

    if !digits_found {
        return Err(TarError::EmptyNumericField);
    }

    Ok(val)
}

/// Extracts a string from a byte slice up to the first NUL byte, rejecting invalid UTF-8.
fn parse_raw_str(bytes: &[u8]) -> Result<&str, TarError> {
    let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    core::str::from_utf8(&bytes[..end]).map_err(|_| TarError::InvalidUtf8)
}

/// A1: Computes the unsigned sum of all 512 header bytes treating chksum (148..156)
/// as ASCII spaces (0x20) and compares against the stored octal value.
pub fn verify_checksum(header: &[u8; 512]) -> Result<usize, TarError> {
    let stored_sum = parse_octal(&header[148..156])?;

    let mut calc_sum: usize = 0;
    for (i, &b) in header.iter().enumerate() {
        let byte_val = if (148..156).contains(&i) {
            0x20usize
        } else {
            b as usize
        };
        calc_sum = calc_sum.checked_add(byte_val).ok_or(TarError::NumericOverflow)?;
    }

    if calc_sum != stored_sum {
        return Err(TarError::BadChecksum);
    }

    Ok(stored_sum)
}

/// A2: Validates magic and version.
/// Requires exact match: header[257..263] == b"ustar\0" and header[263..265] == b"00".
pub fn validate_magic_version(header: &[u8; 512]) -> Result<(), TarError> {
    if &header[257..263] != b"ustar\0" {
        return Err(TarError::BadMagic);
    }
    if &header[263..265] != b"00" {
        return Err(TarError::BadVersion);
    }
    Ok(())
}

/// A5: Normalizes a USTAR path, rejecting any `..` component, empty results,
/// NUL bytes, invalid UTF-8, and combined lengths > 255.
pub fn normalize_path(prefix: &str, name: &str) -> Result<String, TarError> {
    if prefix.contains('\0') || name.contains('\0') {
        return Err(TarError::InvalidPath);
    }

    let mut combined = String::new();
    if !prefix.is_empty() {
        combined.push_str(prefix);
        if !combined.ends_with('/') {
            combined.push('/');
        }
    }
    combined.push_str(name);

    if combined.len() > 255 {
        return Err(TarError::PathTooLong);
    }

    let mut normalized_parts: Vec<&str> = Vec::new();
    for part in combined.split('/') {
        if part.is_empty() || part == "." {
            continue;
        }
        if part == ".." {
            return Err(TarError::InvalidPath);
        }
        normalized_parts.push(part);
    }

    if normalized_parts.is_empty() {
        if !combined.is_empty() {
            return Ok(String::from("/"));
        }
        return Err(TarError::InvalidPath);
    }

    let mut canonical = String::with_capacity(combined.len() + 1);
    for part in normalized_parts {
        canonical.push('/');
        canonical.push_str(part);
    }

    if canonical.len() > 255 {
        return Err(TarError::PathTooLong);
    }

    Ok(canonical)
}

/// A4: Parses a USTAR archive from memory with checked arithmetic and range validation.
/// Requires valid end-of-archive blocks (two 512-byte zero blocks).
pub fn parse_ustar_archive<'a>(archive: &'a [u8]) -> Result<Vec<TarEntry<'a>>, TarError> {
    let mut nodes = Vec::new();
    let mut offset = 0usize;
    let mut end_of_archive_found = false;

    while let Some(next_header_end) = offset.checked_add(TAR_BLOCK_SIZE) {
        if next_header_end > archive.len() {
            return Err(TarError::UnexpectedEof);
        }

        let header_slice = &archive[offset..next_header_end];
        let mut header = [0u8; TAR_BLOCK_SIZE];
        header.copy_from_slice(header_slice);

        // Check for end-of-archive marker (first byte is 0 per POSIX tar)
        if header[0] == 0 {
            // Validate that this block is all zeros
            if !header.iter().all(|&b| b == 0) {
                return Err(TarError::UnexpectedEof);
            }
            // Need two consecutive zero blocks per POSIX standard
            let second_zero_end = next_header_end.checked_add(TAR_BLOCK_SIZE)
                .ok_or(TarError::UnexpectedEof)?;
            if second_zero_end > archive.len() {
                return Err(TarError::UnexpectedEof);
            }
            if archive[next_header_end..second_zero_end].iter().all(|&b| b == 0) {
                end_of_archive_found = true;
                break;
            } else {
                return Err(TarError::UnexpectedEof);
            }
        }

        // A1: Verify header checksum
        verify_checksum(&header)?;

        // A2: Validate magic and version
        validate_magic_version(&header)?;

        let name = parse_raw_str(&header[0..100])?;
        let mode = parse_octal(&header[100..108])? as u32;
        let size = parse_octal(&header[124..136])?;
        let typeflag = header[156];
        let prefix = parse_raw_str(&header[345..500])?;

        // A4: Validate data range BEFORE computing offset
        let data_start = next_header_end;
        let data_end = data_start.checked_add(size).ok_or(TarError::NumericOverflow)?;
        if data_end > archive.len() {
            return Err(TarError::SizeOutOfBounds);
        }
        let data_slice = &archive[data_start..data_end];

        // A5: Path normalization
        let path = normalize_path(prefix, name)?;

        // Root entry handling (e.g. "./", ".", "/" emitted by tools like `tar -C rootfs .`):
        // Skip without creating a node and without error.
        if path == "/" {
            let data_blocks = (size.checked_add(TAR_BLOCK_SIZE - 1).ok_or(TarError::NumericOverflow)?)
                / TAR_BLOCK_SIZE;
            let data_len_aligned = data_blocks.checked_mul(TAR_BLOCK_SIZE).ok_or(TarError::NumericOverflow)?;
            let next_offset = data_start.checked_add(data_len_aligned).ok_or(TarError::NumericOverflow)?;
            if next_offset > archive.len() {
                return Err(TarError::TruncatedArchive);
            }
            offset = next_offset;
            continue;
        }

        // Typeflag handling: 
        // '0' | '\0' => Regular file
        // '5' => Directory
        // Links ('1', '2') and special devices ('3', '4', '6') are ignored and never followed
        let file_type = match typeflag {
            b'5' => Some(TarEntryKind::Directory),
            b'0' | 0 => Some(TarEntryKind::RegularFile),
            _ => None,
        };

        if let Some(ft) = file_type {
            nodes.push(TarEntry {
                path,
                kind: ft,
                size,
                mode,
                data: data_slice,
            });
        }

        // Advance offset: TAR_BLOCK_SIZE + ceiling(size / 512) * 512
        let data_blocks = (size.checked_add(TAR_BLOCK_SIZE - 1).ok_or(TarError::NumericOverflow)?)
            / TAR_BLOCK_SIZE;
        let data_len_aligned = data_blocks.checked_mul(TAR_BLOCK_SIZE).ok_or(TarError::NumericOverflow)?;
        let next_offset = data_start.checked_add(data_len_aligned).ok_or(TarError::NumericOverflow)?;
        
        if next_offset > archive.len() {
            return Err(TarError::TruncatedArchive);
        }
        offset = next_offset;
    }

    if !end_of_archive_found {
        return Err(TarError::UnexpectedEof);
    }

    Ok(nodes)
}

#[cfg(test)]
mod tests;
