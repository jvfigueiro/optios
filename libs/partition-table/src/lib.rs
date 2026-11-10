// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Partition Table Pure Parsers (MBR & GPT)
// Date: 2026-11-10

//! Pure `#![no_std]` partition table parsers for MBR and GPT with CRC32 validation,
//! UTF-16LE partition name decoding, and standard GUID resolution.

#![no_std]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

pub const MBR_SIGNATURE: u16 = 0xAA55;
pub const MBR_PARTITION_TABLE_OFFSET: usize = 446;
pub const MBR_ENTRY_SIZE: usize = 16;
pub const MBR_TYPE_GPT_PROTECTIVE: u8 = 0xEE;

pub const GPT_SIGNATURE: &[u8; 8] = b"EFI PART";
pub const GPT_HEADER_MIN_SIZE: usize = 92;

/// Known GPT Partition Type GUIDs (in raw mixed-endian binary format per UEFI spec).
pub const GUID_EFI_SYSTEM: [u8; 16] = [
    0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11, 0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9,
    0x3B,
];
pub const GUID_MICROSOFT_BASIC_DATA: [u8; 16] = [
    0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44, 0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99,
    0xC7,
];
pub const GUID_LINUX_FILESYSTEM: [u8; 16] = [
    0xAF, 0x3D, 0xC6, 0x0F, 0x83, 0x84, 0x72, 0x47, 0x8E, 0x79, 0x3D, 0x69, 0xD8, 0x47, 0x7D,
    0xE4,
];
pub const GUID_LINUX_SWAP: [u8; 16] = [
    0x6D, 0xFD, 0x57, 0x06, 0xAB, 0xA4, 0xC4, 0x43, 0x84, 0xE5, 0x09, 0x33, 0xC8, 0x4B, 0x4F,
    0x4F,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PartitionError {
    InvalidMbrSignature,
    InvalidGptSignature,
    InvalidGptRevision,
    InvalidHeaderSize,
    HeaderCrcMismatch { expected: u32, calculated: u32 },
    ArrayCrcMismatch { expected: u32, calculated: u32 },
    BufferTooSmall,
    InvalidData,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MbrPartition {
    pub index: usize,
    pub bootable: bool,
    pub part_type: u8,
    pub start_lba: u32,
    pub sector_count: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MbrTable {
    pub is_protective: bool,
    pub partitions: Vec<MbrPartition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptPartition {
    pub index: usize,
    pub name: String,
    pub type_name: String,
    pub type_guid: [u8; 16],
    pub unique_guid: [u8; 16],
    pub start_lba: u64,
    pub end_lba: u64,
    pub sector_count: u64,
    pub size_mib: u64,
    pub attributes: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GptTable {
    pub disk_guid: [u8; 16],
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub partitions: Vec<GptPartition>,
}

/// Computes the IEEE 802.3 CRC-32 (standard Ethernet / POSIX / ZIP / GPT polynomial 0xEDB88320).
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFFu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = -(crc as i32 & 1) as u32;
            crc = (crc >> 1) ^ (0xEDB88320 & mask);
        }
    }
    !crc
}

/// Decodes UTF-16LE code units (e.g. from GPT 72-byte name field) to a trimmed ASCII/UTF-8 String.
pub fn decode_utf16le_string(bytes: &[u8]) -> String {
    let mut s = String::new();
    let num_units = bytes.len() / 2;
    for i in 0..num_units {
        let lo = bytes[i * 2] as u16;
        let hi = bytes[i * 2 + 1] as u16;
        let code = lo | (hi << 8);
        if code == 0 {
            break;
        }
        if (0x20..=0x7E).contains(&code) {
            s.push(code as u8 as char);
        } else {
            // Replace non-ASCII printable or surrogate characters safely
            s.push('?');
        }
    }
    String::from(s.trim())
}

/// Formats a raw 16-byte mixed-endian GUID into standard canonical string `XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX`.
pub fn format_guid(guid: &[u8; 16]) -> String {
    let d1 = (guid[0] as u32)
        | ((guid[1] as u32) << 8)
        | ((guid[2] as u32) << 16)
        | ((guid[3] as u32) << 24);
    let d2 = (guid[4] as u16) | ((guid[5] as u16) << 8);
    let d3 = (guid[6] as u16) | ((guid[7] as u16) << 8);
    format!(
        "{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}",
        d1, d2, d3, guid[8], guid[9], guid[10], guid[11], guid[12], guid[13], guid[14], guid[15]
    )
}

/// Translates a known GPT Partition Type GUID to a human-readable description.
pub fn resolve_gpt_type_name(guid: &[u8; 16]) -> String {
    if *guid == GUID_EFI_SYSTEM {
        String::from("EFI System Partition")
    } else if *guid == GUID_MICROSOFT_BASIC_DATA {
        String::from("Microsoft Basic Data")
    } else if *guid == GUID_LINUX_FILESYSTEM {
        String::from("Linux Filesystem Data")
    } else if *guid == GUID_LINUX_SWAP {
        String::from("Linux Swap")
    } else {
        format_guid(guid)
    }
}

/// Parses an MBR (Sector 0, 512 bytes).
pub fn parse_mbr(sector0: &[u8; 512]) -> Result<MbrTable, PartitionError> {
    let sig = (sector0[510] as u16) | ((sector0[511] as u16) << 8);
    if sig != MBR_SIGNATURE {
        return Err(PartitionError::InvalidMbrSignature);
    }

    let mut is_protective = false;
    let mut partitions = Vec::new();

    for i in 0..4 {
        let off = MBR_PARTITION_TABLE_OFFSET + i * MBR_ENTRY_SIZE;
        let status = sector0[off];
        let part_type = sector0[off + 4];
        let start_lba = (sector0[off + 8] as u32)
            | ((sector0[off + 9] as u32) << 8)
            | ((sector0[off + 10] as u32) << 16)
            | ((sector0[off + 11] as u32) << 24);
        let sector_count = (sector0[off + 12] as u32)
            | ((sector0[off + 13] as u32) << 8)
            | ((sector0[off + 14] as u32) << 16)
            | ((sector0[off + 15] as u32) << 24);

        if part_type == MBR_TYPE_GPT_PROTECTIVE {
            is_protective = true;
        }

        if part_type != 0 && sector_count > 0 {
            partitions.push(MbrPartition {
                index: i + 1,
                bootable: status == 0x80,
                part_type,
                start_lba,
                sector_count,
            });
        }
    }

    Ok(MbrTable {
        is_protective,
        partitions,
    })
}

/// Parses a GPT Header (LBA 1, 512 bytes) and Partition Entries Array (LBAs 2..=33).
pub fn parse_gpt(
    header_sector: &[u8; 512],
    entries_data: &[u8],
) -> Result<GptTable, PartitionError> {
    if &header_sector[..8] != GPT_SIGNATURE {
        return Err(PartitionError::InvalidGptSignature);
    }

    let revision = (header_sector[8] as u32)
        | ((header_sector[9] as u32) << 8)
        | ((header_sector[10] as u32) << 16)
        | ((header_sector[11] as u32) << 24);
    if revision != 0x0001_0000 {
        return Err(PartitionError::InvalidGptRevision);
    }

    let header_size = ((header_sector[12] as u32)
        | ((header_sector[13] as u32) << 8)
        | ((header_sector[14] as u32) << 16)
        | ((header_sector[15] as u32) << 24)) as usize;

    if header_size < GPT_HEADER_MIN_SIZE || header_size > 512 {
        return Err(PartitionError::InvalidHeaderSize);
    }

    // Header CRC32 validation: CRC calculated with CRC field (bytes 16..20) treated as 0
    let recorded_crc = (header_sector[16] as u32)
        | ((header_sector[17] as u32) << 8)
        | ((header_sector[18] as u32) << 16)
        | ((header_sector[19] as u32) << 24);

    let mut header_copy = [0u8; 512];
    header_copy[..header_size].copy_from_slice(&header_sector[..header_size]);
    header_copy[16..20].fill(0);
    let calculated_crc = crc32(&header_copy[..header_size]);

    if calculated_crc != recorded_crc {
        return Err(PartitionError::HeaderCrcMismatch {
            expected: recorded_crc,
            calculated: calculated_crc,
        });
    }

    let first_usable_lba = (header_sector[40] as u64)
        | ((header_sector[41] as u64) << 8)
        | ((header_sector[42] as u64) << 16)
        | ((header_sector[43] as u64) << 24)
        | ((header_sector[44] as u64) << 32)
        | ((header_sector[45] as u64) << 40)
        | ((header_sector[46] as u64) << 48)
        | ((header_sector[47] as u64) << 56);

    let last_usable_lba = (header_sector[48] as u64)
        | ((header_sector[49] as u64) << 8)
        | ((header_sector[50] as u64) << 16)
        | ((header_sector[51] as u64) << 24)
        | ((header_sector[52] as u64) << 32)
        | ((header_sector[53] as u64) << 40)
        | ((header_sector[54] as u64) << 48)
        | ((header_sector[55] as u64) << 56);

    let mut disk_guid = [0u8; 16];
    disk_guid.copy_from_slice(&header_sector[56..72]);

    let num_entries = ((header_sector[80] as u32)
        | ((header_sector[81] as u32) << 8)
        | ((header_sector[82] as u32) << 16)
        | ((header_sector[83] as u32) << 24)) as usize;

    let entry_size = ((header_sector[84] as u32)
        | ((header_sector[85] as u32) << 8)
        | ((header_sector[86] as u32) << 16)
        | ((header_sector[87] as u32) << 24)) as usize;

    let recorded_array_crc = (header_sector[88] as u32)
        | ((header_sector[89] as u32) << 8)
        | ((header_sector[90] as u32) << 16)
        | ((header_sector[91] as u32) << 24);

    let total_array_bytes = num_entries * entry_size;
    if entries_data.len() < total_array_bytes {
        return Err(PartitionError::BufferTooSmall);
    }

    let calculated_array_crc = crc32(&entries_data[..total_array_bytes]);
    if calculated_array_crc != recorded_array_crc {
        return Err(PartitionError::ArrayCrcMismatch {
            expected: recorded_array_crc,
            calculated: calculated_array_crc,
        });
    }

    let mut partitions = Vec::new();
    for i in 0..num_entries {
        let off = i * entry_size;
        let mut type_guid = [0u8; 16];
        type_guid.copy_from_slice(&entries_data[off..off + 16]);

        // Unused partition entry has all-zero Type GUID
        if type_guid == [0u8; 16] {
            continue;
        }

        let mut unique_guid = [0u8; 16];
        unique_guid.copy_from_slice(&entries_data[off + 16..off + 32]);

        let start_lba = (entries_data[off + 32] as u64)
            | ((entries_data[off + 33] as u64) << 8)
            | ((entries_data[off + 34] as u64) << 16)
            | ((entries_data[off + 35] as u64) << 24)
            | ((entries_data[off + 36] as u64) << 32)
            | ((entries_data[off + 37] as u64) << 40)
            | ((entries_data[off + 38] as u64) << 48)
            | ((entries_data[off + 39] as u64) << 56);

        let end_lba = (entries_data[off + 40] as u64)
            | ((entries_data[off + 41] as u64) << 8)
            | ((entries_data[off + 42] as u64) << 16)
            | ((entries_data[off + 43] as u64) << 24)
            | ((entries_data[off + 44] as u64) << 32)
            | ((entries_data[off + 45] as u64) << 40)
            | ((entries_data[off + 46] as u64) << 48)
            | ((entries_data[off + 47] as u64) << 56);

        let attributes = (entries_data[off + 48] as u64)
            | ((entries_data[off + 49] as u64) << 8)
            | ((entries_data[off + 50] as u64) << 16)
            | ((entries_data[off + 51] as u64) << 24)
            | ((entries_data[off + 52] as u64) << 32)
            | ((entries_data[off + 53] as u64) << 40)
            | ((entries_data[off + 54] as u64) << 48)
            | ((entries_data[off + 55] as u64) << 56);

        let name = decode_utf16le_string(&entries_data[off + 56..off + 128]);
        let type_name = resolve_gpt_type_name(&type_guid);
        let sector_count = if end_lba >= start_lba {
            end_lba - start_lba + 1
        } else {
            0
        };
        let size_mib = (sector_count * 512) / (1024 * 1024);

        partitions.push(GptPartition {
            index: i + 1,
            name,
            type_name,
            type_guid,
            unique_guid,
            start_lba,
            end_lba,
            sector_count,
            size_mib,
            attributes,
        });
    }

    Ok(GptTable {
        disk_guid,
        first_usable_lba,
        last_usable_lba,
        partitions,
    })
}

#[cfg(test)]
mod tests;
