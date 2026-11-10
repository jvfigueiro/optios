// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Pure ELF64 Executable Parser & Segment Validator
// Date: 2026-11-10

//! Pure, `#![no_std]` ELF64 binary loader and program header validator for OptiOS.
//! Validates ELF magic, target architecture (x86-64), endianness, and enforces strict
//! isolation by rejecting any segment overlapping kernel virtual address space.

#![no_std]

extern crate alloc;
use alloc::vec::Vec;

pub const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
pub const ELF_CLASS_64: u8 = 2;
pub const ELF_DATA_2LSB: u8 = 1;
pub const ELF_VERSION_CURRENT: u8 = 1;

pub const ET_EXEC: u16 = 2;
pub const ET_DYN: u16 = 3;
pub const EM_X86_64: u16 = 62;

pub const PT_LOAD: u32 = 1;

pub const PF_X: u32 = 1 << 0;
pub const PF_W: u32 = 1 << 1;
pub const PF_R: u32 = 1 << 2;

/// Upper bound for userland virtual addresses in canonical 48-bit x86-64 paging.
/// Any mapping at or above this threshold is reserved for the kernel.
pub const USER_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ElfError {
    BufferTooSmall,
    InvalidMagic,
    Not64Bit,
    NotLittleEndian,
    InvalidVersion,
    InvalidArchitecture,
    UnsupportedType,
    InvalidPhdrCount,
    CorruptedSegmentOffset,
    SegmentZeroMemSize,
    FileSizeExceedsMemSize,
    AddressOverflow,
    KernelAddressOverlap,
    InvalidEntryPoint,
}

#[derive(Debug, Clone)]
pub struct LoadedSegment<'a> {
    pub vaddr: u64,
    pub mem_size: u64,
    pub file_data: &'a [u8],
    pub flags: u32,
    pub align: u64,
}

#[derive(Debug, Clone)]
pub struct ElfBinary<'a> {
    pub entry_point: u64,
    pub segments: Vec<LoadedSegment<'a>>,
}

/// Parses and strictly validates an ELF64 executable binary buffer.
pub fn parse_elf<'a>(data: &'a [u8]) -> Result<ElfBinary<'a>, ElfError> {
    if data.len() < 64 {
        return Err(ElfError::BufferTooSmall);
    }

    if data[0..4] != ELF_MAGIC {
        return Err(ElfError::InvalidMagic);
    }

    if data[4] != ELF_CLASS_64 {
        return Err(ElfError::Not64Bit);
    }

    if data[5] != ELF_DATA_2LSB {
        return Err(ElfError::NotLittleEndian);
    }

    if data[6] != ELF_VERSION_CURRENT {
        return Err(ElfError::InvalidVersion);
    }

    let e_type = u16::from_le_bytes(data[16..18].try_into().unwrap());
    if e_type != ET_EXEC && e_type != ET_DYN {
        return Err(ElfError::UnsupportedType);
    }

    let e_machine = u16::from_le_bytes(data[18..20].try_into().unwrap());
    if e_machine != EM_X86_64 {
        return Err(ElfError::InvalidArchitecture);
    }

    let e_entry = u64::from_le_bytes(data[24..32].try_into().unwrap());
    let e_phoff = u64::from_le_bytes(data[32..40].try_into().unwrap()) as usize;
    let e_phentsize = u16::from_le_bytes(data[54..56].try_into().unwrap()) as usize;
    let e_phnum = u16::from_le_bytes(data[56..58].try_into().unwrap()) as usize;

    if e_phentsize < 56 || e_phnum == 0 {
        return Err(ElfError::InvalidPhdrCount);
    }

    let phdr_table_end = e_phoff
        .checked_add(e_phnum.checked_mul(e_phentsize).ok_or(ElfError::AddressOverflow)?)
        .ok_or(ElfError::AddressOverflow)?;

    if phdr_table_end > data.len() {
        return Err(ElfError::InvalidPhdrCount);
    }

    let mut segments = Vec::new();
    let mut has_executable_segment = false;

    for i in 0..e_phnum {
        let offset = e_phoff + i * e_phentsize;
        let p_type = u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap());
        let p_flags = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().unwrap());
        let p_offset = u64::from_le_bytes(data[offset + 8..offset + 16].try_into().unwrap()) as usize;
        let p_vaddr = u64::from_le_bytes(data[offset + 16..offset + 24].try_into().unwrap());
        let _p_paddr = u64::from_le_bytes(data[offset + 24..offset + 32].try_into().unwrap());
        let p_filesz = u64::from_le_bytes(data[offset + 32..offset + 40].try_into().unwrap()) as usize;
        let p_memsz = u64::from_le_bytes(data[offset + 40..offset + 48].try_into().unwrap());
        let p_align = u64::from_le_bytes(data[offset + 48..offset + 56].try_into().unwrap());

        if p_type != PT_LOAD {
            continue;
        }

        if p_memsz == 0 {
            return Err(ElfError::SegmentZeroMemSize);
        }

        if (p_filesz as u64) > p_memsz {
            return Err(ElfError::FileSizeExceedsMemSize);
        }

        let file_end = p_offset.checked_add(p_filesz).ok_or(ElfError::AddressOverflow)?;
        if file_end > data.len() {
            return Err(ElfError::CorruptedSegmentOffset);
        }

        let vaddr_end = p_vaddr.checked_add(p_memsz).ok_or(ElfError::AddressOverflow)?;
        if vaddr_end > USER_SPACE_LIMIT {
            return Err(ElfError::KernelAddressOverlap);
        }

        if (p_flags & PF_X) != 0 {
            has_executable_segment = true;
        }

        segments.push(LoadedSegment {
            vaddr: p_vaddr,
            mem_size: p_memsz,
            file_data: &data[p_offset..file_end],
            flags: p_flags,
            align: p_align,
        });
    }

    if segments.is_empty() || !has_executable_segment {
        return Err(ElfError::InvalidEntryPoint);
    }

    // Check entry point is within one of the PT_LOAD segments
    let mut entry_valid = false;
    for seg in &segments {
        if e_entry >= seg.vaddr && e_entry < (seg.vaddr + seg.mem_size) && (seg.flags & PF_X) != 0 {
            entry_valid = true;
            break;
        }
    }

    if !entry_valid {
        return Err(ElfError::InvalidEntryPoint);
    }

    Ok(ElfBinary {
        entry_point: e_entry,
        segments,
    })
}

#[cfg(test)]
mod tests;
