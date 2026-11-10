// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: UEFI / Limine Physical Memory Map Parser
// Date: 2026-11-09

//! Physical memory map parsing and accounting for OptiOS.

use limine::memory_map::{Entry, EntryType};

/// Aggregated statistics of installed physical RAM.
#[derive(Debug, Clone, Copy, Default)]
pub struct MemoryStats {
    /// Total usable physical RAM available to the operating system (bytes).
    pub usable_bytes: u64,
    /// Total physical RAM installed in the system (bytes).
    /// Sum of Usable, BootloaderReclaimable, Kernel/Modules, and ACPI memory.
    pub total_ram_bytes: u64,
    /// Memory reserved by firmware/hardware MMIO (bytes).
    pub reserved_bytes: u64,
    /// Total number of memory regions provided by the bootloader.
    pub entry_count: usize,
}

impl MemoryStats {
    /// Usable physical RAM in MiB.
    pub fn usable_mib(&self) -> u64 {
        self.usable_bytes / (1024 * 1024)
    }

    /// Total physical RAM installed in MiB.
    pub fn total_ram_mib(&self) -> u64 {
        self.total_ram_bytes / (1024 * 1024)
    }

    /// Usable physical RAM in KiB.
    #[allow(dead_code)]
    pub fn usable_kib(&self) -> u64 {
        self.usable_bytes / 1024
    }

    /// Total physical RAM installed in KiB.
    #[allow(dead_code)]
    pub fn total_ram_kib(&self) -> u64 {
        self.total_ram_bytes / 1024
    }
}

/// Parses memory map entries from Limine and calculates accurate physical RAM statistics.
///
/// Strictly separates real system RAM (Usable, Bootloader Reclaimable, Kernel/Modules,
/// and ACPI) from hardware MMIO, PCI aperture holes, and Reserved regions.
pub fn parse_memory_map(entries: &[&Entry]) -> MemoryStats {
    let mut stats = MemoryStats {
        entry_count: entries.len(),
        ..Default::default()
    };

    for entry in entries {
        let length = entry.length;

        match entry.entry_type {
            EntryType::USABLE => {
                stats.usable_bytes += length;
                stats.total_ram_bytes += length;
            }
            EntryType::BOOTLOADER_RECLAIMABLE
            | EntryType::EXECUTABLE_AND_MODULES
            | EntryType::ACPI_RECLAIMABLE
            | EntryType::ACPI_NVS => {
                // Real physical RAM consumed by firmware, kernel or bootloader
                stats.total_ram_bytes += length;
            }
            EntryType::RESERVED => {
                stats.reserved_bytes += length;
            }
            _ => {
                // Other entries (Framebuffer MMIO, Bad Memory, etc.) are ignored for RAM calculation
            }
        }
    }

    stats
}
