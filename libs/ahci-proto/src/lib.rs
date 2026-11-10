// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: AHCI Protocol Pure Data Structures & Builders
// Date: 2026-11-10

//! Pure, `#![no_std]` AHCI 1.0/1.3 protocol structures, bitfield builders,
//! ATA IDENTIFY DEVICE decoder, S.M.A.R.T. telemetry parser, and BlockDevice trait for OptiOS.

#![no_std]

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;

pub const FIS_TYPE_REG_H2D: u8 = 0x27;

/// Enumeration of permissible ATA commands.
/// OptiOS enforces storage read-only safety by construction:
/// only read and diagnostic commands can be represented.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum AllowedCommand {
    IdentifyDevice = 0xEC,
    ReadDmaExt = 0x25,
    SmartReadData = 0xB0,
}

/// Errors returned by AHCI protocol operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AhciError {
    Timeout,
    TaskFileError,
    NotPresent,
    UnsupportedController,
    InvalidBar,
    DmaAddressAbove4G,
    PortBusy,
    CommandAborted,
    DeviceError,
    BufferTooSmall,
    NoFreeSlot,
    TimerStalled,
    PortFailed,
}

/// Decoded ATA IDENTIFY metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentifiedDisk {
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub total_sectors: u64,
    pub capacity_mib: u64,
    pub smart_supported: bool,
    pub smart_enabled: bool,
}

/// Parsed S.M.A.R.T. single attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SmartAttribute {
    pub id: u8,
    pub current: u8,
    pub worst: u8,
    pub raw: u64,
}

/// Extracted vital S.M.A.R.T. telemetry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SmartTelemetry {
    pub revision: u16,
    pub temperature_c: Option<u32>,
    pub power_on_hours: Option<u64>,
    pub power_cycles: Option<u64>,
    pub reallocated_sectors: Option<u64>,
    pub attributes: Vec<SmartAttribute>,
}

/// Abstract block storage device reading interface.
pub trait BlockDevice {
    /// Read consecutive sectors starting at `lba` into `buf`.
    fn read_sectors(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), AhciError>;
    /// Logical sector size in bytes (e.g. 512).
    fn sector_size(&self) -> u32;
    /// Total addressable LBA sectors.
    fn total_sectors(&self) -> u64;
}

/// Builds a 20-byte Host-to-Device (H2D) Register FIS for ATA IDENTIFY DEVICE.
pub fn build_h2d_register_fis(cmd: AllowedCommand, out: &mut [u8]) -> Result<(), AhciError> {
    if out.len() < 20 {
        return Err(AhciError::BufferTooSmall);
    }
    out[..20].fill(0);
    out[0] = FIS_TYPE_REG_H2D;
    out[1] = 0x80; // C bit set (command update)
    out[2] = cmd as u8;
    out[7] = 0x00; // Device register
    Ok(())
}

/// Builds a 20-byte Host-to-Device (H2D) Register FIS for ATA READ DMA EXT (0x25, LBA48).
pub fn build_read_dma_ext_fis(
    lba: u64,
    sector_count: u16,
    out: &mut [u8],
) -> Result<(), AhciError> {
    if out.len() < 20 {
        return Err(AhciError::BufferTooSmall);
    }
    out[..20].fill(0);
    out[0] = FIS_TYPE_REG_H2D;
    out[1] = 0x80; // C bit set (command)
    out[2] = AllowedCommand::ReadDmaExt as u8;
    out[3] = 0x00; // Features Low
    out[4] = (lba & 0xFF) as u8;
    out[5] = ((lba >> 8) & 0xFF) as u8;
    out[6] = ((lba >> 16) & 0xFF) as u8;
    out[7] = 0x40; // Device: LBA bit 6 set
    out[8] = ((lba >> 24) & 0xFF) as u8;
    out[9] = ((lba >> 32) & 0xFF) as u8;
    out[10] = ((lba >> 40) & 0xFF) as u8;
    out[11] = 0x00; // Features Exp
    out[12] = (sector_count & 0xFF) as u8;
    out[13] = ((sector_count >> 8) & 0xFF) as u8;
    Ok(())
}

/// Builds a 20-byte Host-to-Device (H2D) Register FIS for ATA SMART READ DATA (0xB0 / 0xD0).
pub fn build_smart_read_data_fis(out: &mut [u8]) -> Result<(), AhciError> {
    if out.len() < 20 {
        return Err(AhciError::BufferTooSmall);
    }
    out[..20].fill(0);
    out[0] = FIS_TYPE_REG_H2D;
    out[1] = 0x80; // C bit set (command)
    out[2] = AllowedCommand::SmartReadData as u8; // 0xB0
    out[3] = 0xD0; // Feature: SMART READ DATA
    out[4] = 0x00; // LBA Low
    out[5] = 0x4F; // LBA Mid constant
    out[6] = 0xC2; // LBA High constant
    out[7] = 0x00; // Device register
    out[12] = 0x01; // Sector Count: 1
    Ok(())
}

/// Builds a 32-byte AHCI Command Header (dw0..dw3 representation) in Read-Only mode.
/// - `cfl`: Command FIS Length in 32-bit dwords (e.g. 5 for 20-byte H2D FIS)
/// - `prdtl`: Physical Region Descriptor Table Length (number of entries)
/// - `ctba`: Command Table Base Physical Address (128-byte aligned)
/// Note: Write bit (W) is strictly 0 (Read-Only) by construction.
pub fn build_command_header(
    cfl: u8,
    prdtl: u16,
    ctba: u64,
    out: &mut [u32],
) -> Result<(), AhciError> {
    if out.len() < 4 {
        return Err(AhciError::BufferTooSmall);
    }
    // W bit (bit 6) is strictly 0 for read-only safety
    let dw0 = ((prdtl as u32) << 16) | ((cfl as u32) & 0x1F);
    let dw1 = 0u32; // PRDBC = 0
    let dw2 = (ctba & 0xFFFF_FFFF) as u32;
    let dw3 = (ctba >> 32) as u32;

    out[0] = dw0;
    out[1] = dw1;
    out[2] = dw2;
    out[3] = dw3;
    Ok(())
}

/// Finds the first available command slot in `0..ncs` given the bitmask of currently
/// occupied slots (`ci | sact`).
/// Returns `Some(slot)` if a slot in `0..ncs` is free (bit is 0), or `None` if all slots
/// in `0..ncs` are occupied.
#[inline]
pub fn find_free_slot(occupied: u32, ncs: u8) -> Option<u8> {
    if ncs == 0 {
        return None;
    }
    let max_slots = ncs.min(32);
    let mask = if max_slots == 32 {
        0xFFFF_FFFF
    } else {
        (1u32 << max_slots) - 1
    };
    let available = (!occupied) & mask;
    if available == 0 {
        None
    } else {
        Some(available.trailing_zeros() as u8)
    }
}

/// Builds a 16-byte PRDT (Physical Region Descriptor Table) entry.
/// - `buf_phys`: Physical base address of the data buffer
/// - `byte_count`: Data length in bytes (1..=4 MiB)
/// - `interrupt_on_completion`: Set bit 31 to trigger interrupt on completion
pub fn build_prdt_entry(
    buf_phys: u64,
    byte_count: u32,
    interrupt_on_completion: bool,
    out: &mut [u32],
) -> Result<(), AhciError> {
    if out.len() < 4 || byte_count == 0 || byte_count > 4 * 1024 * 1024 {
        return Err(AhciError::BufferTooSmall);
    }
    let dba = (buf_phys & 0xFFFF_FFFF) as u32;
    let dbau = (buf_phys >> 32) as u32;
    let ioc_bit = if interrupt_on_completion { 1u32 << 31 } else { 0 };
    let dbc = (byte_count - 1) & 0x3F_FFFF; // 0-based byte count (len - 1)
    let dw3 = ioc_bit | dbc;

    out[0] = dba;
    out[1] = dbau;
    out[2] = 0;
    out[3] = dw3;
    Ok(())
}

/// Extracts an ATA byte-swapped ASCII string from a slice of 16-bit IDENTIFY words.
pub fn parse_ata_string(words: &[u16]) -> String {
    let mut bytes = Vec::with_capacity(words.len() * 2);
    for &w in words {
        let hi = (w >> 8) as u8;
        let lo = (w & 0xFF) as u8;
        if (0x20..=0x7E).contains(&hi) {
            bytes.push(hi);
        }
        if (0x20..=0x7E).contains(&lo) {
            bytes.push(lo);
        }
    }
    let s = core::str::from_utf8(&bytes).unwrap_or("Unknown").trim();
    String::from(s)
}

/// Decodes the 256-word (512-byte) response from ATA `IDENTIFY DEVICE` (`0xEC`).
pub fn decode_identify_data(words: &[u16; 256]) -> IdentifiedDisk {
    let serial = parse_ata_string(&words[10..20]);
    let firmware = parse_ata_string(&words[23..27]);
    let model = parse_ata_string(&words[27..47]);

    // Words 100..=103 hold the 64-bit LBA48 sector count
    let lba48 = (words[100] as u64)
        | ((words[101] as u64) << 16)
        | ((words[102] as u64) << 32)
        | ((words[103] as u64) << 48);
    let lba28 = (words[60] as u64) | ((words[61] as u64) << 16);
    let total_sectors = if lba48 != 0 { lba48 } else { lba28 };
    let capacity_mib = (total_sectors * 512) / (1024 * 1024);

    // Word 82 Bit 0: SMART feature set supported; Word 85 Bit 0: SMART enabled
    let smart_supported = (words[82] & 0x0001) != 0 && words[82] != 0xFFFF;
    let smart_enabled = (words[85] & 0x0001) != 0 && words[85] != 0xFFFF;

    IdentifiedDisk {
        model,
        serial,
        firmware,
        total_sectors,
        capacity_mib,
        smart_supported,
        smart_enabled,
    }
}

/// Decodes the 512-byte payload returned by ATA `SMART READ DATA` (`0xB0` / `0xD0`).
pub fn parse_smart_data(data: &[u8; 512]) -> SmartTelemetry {
    let revision = (data[0] as u16) | ((data[1] as u16) << 8);
    let mut temp = None;
    let mut poh = None;
    let mut cycles = None;
    let mut realloc = None;
    let mut attrs = Vec::new();

    for i in 0..30 {
        let off = 2 + i * 12;
        let id = data[off];
        if id == 0 {
            continue;
        }
        let current = data[off + 3];
        let worst = data[off + 4];
        let raw = (data[off + 5] as u64)
            | ((data[off + 6] as u64) << 8)
            | ((data[off + 7] as u64) << 16)
            | ((data[off + 8] as u64) << 24)
            | ((data[off + 9] as u64) << 32)
            | ((data[off + 10] as u64) << 40);

        attrs.push(SmartAttribute {
            id,
            current,
            worst,
            raw,
        });

        match id {
            5 => realloc = Some(raw),
            9 => poh = Some(raw),
            12 => cycles = Some(raw),
            190 | 194 => {
                let t = (raw & 0xFF) as u32;
                if t > 0 && t < 150 {
                    temp = Some(t);
                }
            }
            _ => {}
        }
    }

    SmartTelemetry {
        revision,
        temperature_c: temp,
        power_on_hours: poh,
        power_cycles: cycles,
        reallocated_sectors: realloc,
        attributes: attrs,
    }
}

#[cfg(test)]
mod tests;
