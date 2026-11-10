// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: SMBIOS / DMI Firmware Table Parser (Types 0, 1, 2)
// Date: 2026-11-09

//! SMBIOS / DMI Firmware Table Parser for OptiOS.
//! Extracts BIOS (Type 0), System (Type 1), and Baseboard (Type 2) hardware strings.

extern crate alloc;
use alloc::string::String;
use crate::mm::vmm::{self, flags};
use crate::{info, warn};

/// Parsed SMBIOS / DMI firmware and system identification strings.
#[derive(Debug, Clone)]
pub struct SmbiosInfo {
    pub major_version: u8,
    pub minor_version: u8,
    pub bios_vendor: String,
    pub bios_version: String,
    pub bios_release_date: String,
    pub sys_manufacturer: String,
    pub sys_product_name: String,
    pub sys_version: String,
    pub board_manufacturer: String,
    pub board_product: String,
    pub structure_count: usize,
}

impl Default for SmbiosInfo {
    fn default() -> Self {
        Self {
            major_version: 0,
            minor_version: 0,
            bios_vendor: String::from("N/A"),
            bios_version: String::from("N/A"),
            bios_release_date: String::from("N/A"),
            sys_manufacturer: String::from("N/A"),
            sys_product_name: String::from("N/A"),
            sys_version: String::from("N/A"),
            board_manufacturer: String::from("N/A"),
            board_product: String::from("N/A"),
            structure_count: 0,
        }
    }
}

/// Converts a physical or HHDM address into a valid virtual pointer, ensuring that
/// all 4 KiB pages covering `[addr .. addr + size]` are mapped in the active page tables.
/// (Under Limine BaseRevision 3, Reserved/ACPI firmware regions are not pre-mapped in HHDM.)
fn ensure_mapped(addr: u64, size: usize, hhdm_offset: u64) -> *const u8 {
    let (phys_addr, virt_addr) = if addr < hhdm_offset {
        (addr, addr + hhdm_offset)
    } else {
        (addr - hhdm_offset, addr)
    };

    let start_page = phys_addr & !0xFFF;
    let end_page = (phys_addr.saturating_add(size as u64).saturating_add(0xFFF)) & !0xFFF;

    let mut page_phys = start_page;
    while page_phys < end_page {
        let page_virt = page_phys + hhdm_offset;
        if vmm::virt_to_phys(page_virt).is_none() {
            let _ = vmm::map_page(
                page_virt,
                page_phys,
                flags::PRESENT | flags::NO_EXECUTE,
            );
        }
        page_phys += 4096;
    }

    virt_addr as *const u8
}

/// Reads a 1-based string index from the unformatted string-set following a formatted SMBIOS header.
unsafe fn read_smbios_string(str_area_start: *const u8, max_end: *const u8, index: u8) -> String {
    if index == 0 {
        return String::from("Not Specified");
    }

    let mut curr = str_area_start;
    let mut current_idx: u8 = 1;

    while curr < max_end {
        // An empty string at the very start or double null means end of string table
        if *curr == 0 {
            break;
        }

        // Find length of current null-terminated string
        let mut len = 0usize;
        while curr.add(len) < max_end && *curr.add(len) != 0 {
            len += 1;
        }

        if current_idx == index {
            let slice = core::slice::from_raw_parts(curr, len);
            if let Ok(s) = core::str::from_utf8(slice) {
                let trimmed = s.trim();
                if !trimmed.is_empty() {
                    return String::from(trimmed);
                }
            }
            return String::from("Unknown");
        }

        // Advance past the null terminator of this string
        curr = curr.add(len + 1);
        current_idx = current_idx.saturating_add(1);
    }

    String::from("N/A")
}

/// Parses SMBIOS structures from a virtual structure table pointer and length.
unsafe fn parse_structure_table(table_ptr: *const u8, table_len: usize, info: &mut SmbiosInfo) {
    let max_end = table_ptr.add(table_len);
    let mut curr = table_ptr;

    while curr.add(4) <= max_end {
        let type_id = *curr;
        let formatted_len = *curr.add(1) as usize;

        if formatted_len < 4 || curr.add(formatted_len) > max_end {
            break;
        }

        info.structure_count += 1;
        let str_area = curr.add(formatted_len);

        match type_id {
            0 => {
                // Type 0: BIOS Information
                if formatted_len >= 9 {
                    let vendor_idx = *curr.add(0x04);
                    let version_idx = *curr.add(0x05);
                    let date_idx = *curr.add(0x08);
                    info.bios_vendor = read_smbios_string(str_area, max_end, vendor_idx);
                    info.bios_version = read_smbios_string(str_area, max_end, version_idx);
                    info.bios_release_date = read_smbios_string(str_area, max_end, date_idx);
                }
            }
            1 => {
                // Type 1: System Information
                if formatted_len >= 7 {
                    let mfg_idx = *curr.add(0x04);
                    let prod_idx = *curr.add(0x05);
                    let ver_idx = *curr.add(0x06);
                    info.sys_manufacturer = read_smbios_string(str_area, max_end, mfg_idx);
                    info.sys_product_name = read_smbios_string(str_area, max_end, prod_idx);
                    info.sys_version = read_smbios_string(str_area, max_end, ver_idx);
                }
            }
            2 => {
                // Type 2: Baseboard / Motherboard Information
                if formatted_len >= 7 {
                    let mfg_idx = *curr.add(0x04);
                    let prod_idx = *curr.add(0x05);
                    info.board_manufacturer = read_smbios_string(str_area, max_end, mfg_idx);
                    info.board_product = read_smbios_string(str_area, max_end, prod_idx);
                }
            }
            127 => {
                // Type 127: End-of-Table marker
                break;
            }
            _ => {}
        }

        // Advance `curr` past the unformatted string-set (terminated by double null `\0\0`)
        let mut scan = str_area;
        while scan.add(1) < max_end {
            if *scan == 0 && *scan.add(1) == 0 {
                scan = scan.add(2);
                break;
            }
            scan = scan.add(1);
        }
        if scan <= curr {
            break;
        }
        curr = scan;
    }
}

/// Initializes and parses the SMBIOS / DMI tables provided by Limine.
pub fn init(entry_32: Option<u64>, entry_64: Option<u64>, hhdm_offset: u64) -> SmbiosInfo {
    let mut smbios = SmbiosInfo::default();

    unsafe {
        if let Some(e64_addr) = entry_64 {
            let ep = ensure_mapped(e64_addr, 64, hhdm_offset);
            let anchor = core::slice::from_raw_parts(ep, 5);
            if anchor == b"_SM3_" {
                smbios.major_version = *ep.add(0x07);
                smbios.minor_version = *ep.add(0x08);
                let max_size = (u32::from_le_bytes([
                    *ep.add(0x0C),
                    *ep.add(0x0D),
                    *ep.add(0x0E),
                    *ep.add(0x0F),
                ]) as usize)
                    .min(65536);
                let table_phys = u64::from_le_bytes([
                    *ep.add(0x10),
                    *ep.add(0x11),
                    *ep.add(0x12),
                    *ep.add(0x13),
                    *ep.add(0x14),
                    *ep.add(0x15),
                    *ep.add(0x16),
                    *ep.add(0x17),
                ]);

                let table_virt = ensure_mapped(table_phys, max_size, hhdm_offset);
                parse_structure_table(table_virt, max_size, &mut smbios);
            }
        } else if let Some(e32_addr) = entry_32 {
            let ep = ensure_mapped(e32_addr, 64, hhdm_offset);
            let anchor = core::slice::from_raw_parts(ep, 4);
            if anchor == b"_SM_" {
                smbios.major_version = *ep.add(0x06);
                smbios.minor_version = *ep.add(0x07);
                let table_len = (u16::from_le_bytes([*ep.add(0x16), *ep.add(0x17)]) as usize).min(65536);
                let table_phys = u32::from_le_bytes([
                    *ep.add(0x18),
                    *ep.add(0x19),
                    *ep.add(0x1A),
                    *ep.add(0x1B),
                ]) as u64;

                let table_virt = ensure_mapped(table_phys, table_len, hhdm_offset);
                parse_structure_table(table_virt, table_len, &mut smbios);
            }
        } else {
            warn!("SMBIOS: No 32-bit or 64-bit SMBIOS entry point provided by bootloader.");
            return smbios;
        }
    }

    info!(
        "SMBIOS {}.{} parsed ({} structures): BIOS={} ({}, {}), System={} {} ({})",
        smbios.major_version,
        smbios.minor_version,
        smbios.structure_count,
        smbios.bios_vendor,
        smbios.bios_version,
        smbios.bios_release_date,
        smbios.sys_manufacturer,
        smbios.sys_product_name,
        smbios.sys_version
    );

    smbios
}
