// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: PCI/PCIe Configuration Space Scanner & Bus Mastering Control
// Date: 2026-11-10

//! PCI Bus Enumeration Driver using I/O configuration ports (0xCF8 / 0xCFC).
//! Scans buses 0..=255 with Multi-Function detection and BAR0..BAR5 inspection.

extern crate alloc;
use alloc::vec::Vec;
use crate::arch::x86_64::io::{inl, inw, outl, outw};
use crate::info;

const PCI_CONFIG_ADDRESS: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;

/// Represents an enumerated PCI/PCIe hardware device or function.
#[derive(Debug, Clone, Copy)]
pub struct PciDevice {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub header_type: u8,
    pub bars: [u32; 6],
}

impl PciDevice {
    /// Translates known standard PCI Vendor IDs to descriptive names.
    pub fn vendor_name(&self) -> &'static str {
        match self.vendor_id {
            0x8086 => "Intel Corporation",
            0x1234 => "QEMU / Bochs Virtual Device",
            0x1AF4 => "Red Hat, Inc. (VirtIO)",
            0x10EC => "Realtek Semiconductor Corp.",
            0x1022 => "Advanced Micro Devices (AMD)",
            0x10DE => "NVIDIA Corporation",
            0x1000 => "LSI Logic / Broadcom",
            0x1013 => "Cirrus Logic",
            0x15AD => "VMware Inc.",
            0x1B36 => "Red Hat, Inc. (QEMU PCIe)",
            _ => "Unknown Vendor",
        }
    }

    /// Translates PCI Class, Subclass, and ProgIF codes to standard device category names.
    pub fn class_name(&self) -> &'static str {
        match (self.class_code, self.subclass, self.prog_if) {
            (0x00, 0x01, _) => "VGA Compatible Unclassified Device",
            (0x01, 0x00, _) => "SCSI Bus Controller",
            (0x01, 0x01, _) => "IDE Interface Controller",
            (0x01, 0x06, 0x01) => "SATA Controller (AHCI 1.0)",
            (0x01, 0x06, _) => "SATA Storage Controller",
            (0x01, 0x08, 0x02) => "NVMe Express Controller",
            (0x01, 0x08, _) => "Non-Volatile Memory (NVMe)",
            (0x01, _, _) => "Mass Storage Controller",
            (0x02, 0x00, _) => "Gigabit Ethernet Controller",
            (0x02, _, _) => "Network Controller",
            (0x03, 0x00, _) => "VGA Compatible Controller",
            (0x03, _, _) => "Display Controller",
            (0x04, 0x01, _) => "Multimedia Audio Controller",
            (0x04, 0x03, _) => "High Definition Audio (HDA)",
            (0x04, _, _) => "Multimedia Controller",
            (0x05, 0x00, _) => "RAM Memory Controller",
            (0x05, _, _) => "Memory Controller",
            (0x06, 0x00, _) => "Host / PCIe Root Bridge",
            (0x06, 0x01, _) => "ISA / LPC Interface Bridge",
            (0x06, 0x04, _) => "PCI-to-PCI Bridge",
            (0x06, _, _) => "System Bridge Device",
            (0x0C, 0x03, 0x00) => "USB UHCI Controller",
            (0x0C, 0x03, 0x10) => "USB OHCI Controller",
            (0x0C, 0x03, 0x20) => "USB2 EHCI Controller",
            (0x0C, 0x03, 0x30) => "USB3 xHCI Controller",
            (0x0C, 0x03, _) => "USB Controller",
            (0x0C, 0x05, _) => "SMBus System Controller",
            (0x0C, _, _) => "Serial Bus Controller",
            _ => "Peripheral Device",
        }
    }

    /// Returns the first non-zero Base Address Register (BAR) and its index, if any.
    #[allow(dead_code)]
    pub fn primary_bar(&self) -> Option<(usize, u32)> {
        for (i, &bar) in self.bars.iter().enumerate() {
            let masked = if (bar & 1) != 0 {
                bar & 0xFFFF_FFFC // I/O Space BAR
            } else {
                bar & 0xFFFF_FFF0 // Memory Space BAR
            };
            if masked != 0 {
                return Some((i, bar));
            }
        }
        None
    }
}

/// Reads a 16-bit word from PCI configuration space.
#[inline]
pub fn pci_read_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inw(PCI_CONFIG_DATA + ((offset & 2) as u16))
    }
}

/// Writes a 16-bit word to PCI configuration space.
#[inline]
pub fn pci_write_config_word(bus: u8, device: u8, function: u8, offset: u8, value: u16) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        outw(PCI_CONFIG_DATA + ((offset & 2) as u16), value);
    }
}

/// Reads a 32-bit dword from PCI configuration space.
#[inline]
pub fn pci_read_config_dword(bus: u8, device: u8, function: u8, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        inl(PCI_CONFIG_DATA)
    }
}

/// Writes a 32-bit dword to PCI configuration space.
#[inline]
pub fn pci_write_config_dword(bus: u8, device: u8, function: u8, offset: u8, value: u32) {
    let address = 0x8000_0000u32
        | ((bus as u32) << 16)
        | (((device & 0x1F) as u32) << 11)
        | (((function & 0x07) as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        outl(PCI_CONFIG_ADDRESS, address);
        outl(PCI_CONFIG_DATA, value);
    }
}

/// Enables PCI Memory Space (bit 1) and PCI Bus Mastering (bit 2) in the Command register (0x04)
/// using a 16-bit write so Status register write-1-to-clear bits at offset 0x06 are unaffected.
#[allow(dead_code)]
pub fn enable_bus_mastering_and_mmio(bus: u8, device: u8, function: u8) {
    let cmd = pci_read_config_word(bus, device, function, 0x04);
    let updated = cmd | (1 << 1) | (1 << 2);
    pci_write_config_word(bus, device, function, 0x04, updated);
}

/// Reads details of a confirmed valid PCI function and populates a `PciDevice`.
fn probe_function(bus: u8, device: u8, function: u8, reg0: u32) -> PciDevice {
    let vendor_id = (reg0 & 0xFFFF) as u16;
    let device_id = (reg0 >> 16) as u16;

    let reg8 = pci_read_config_dword(bus, device, function, 0x08);
    let prog_if = ((reg8 >> 8) & 0xFF) as u8;
    let subclass = ((reg8 >> 16) & 0xFF) as u8;
    let class_code = ((reg8 >> 24) & 0xFF) as u8;

    let regc = pci_read_config_dword(bus, device, function, 0x0C);
    let header_type = ((regc >> 16) & 0xFF) as u8;

    let mut bars = [0u32; 6];
    let layout_type = header_type & 0x7F;
    if layout_type == 0x00 {
        // Standard PCI device has 6 BARs (0x10..=0x24)
        for i in 0..6 {
            bars[i] = pci_read_config_dword(bus, device, function, 0x10 + (i as u8 * 4));
        }
    } else if layout_type == 0x01 {
        // PCI-to-PCI bridge has 2 BARs (0x10..=0x14)
        for i in 0..2 {
            bars[i] = pci_read_config_dword(bus, device, function, 0x10 + (i as u8 * 4));
        }
    }

    PciDevice {
        bus,
        device,
        function,
        vendor_id,
        device_id,
        class_code,
        subclass,
        prog_if,
        header_type,
        bars,
    }
}

/// Scans all PCI buses 0..=255 across all 32 slots and up to 8 functions per slot.
pub fn scan_pci_bus() -> Vec<PciDevice> {
    let mut devices = Vec::new();

    for bus in 0..=255u8 {
        for device in 0..32u8 {
            // Check Function 0 first
            let reg0 = pci_read_config_dword(bus, device, 0, 0x00);
            let vendor_id = (reg0 & 0xFFFF) as u16;
            if vendor_id == 0xFFFF {
                continue; // Slot is empty; skip all functions
            }

            let dev0 = probe_function(bus, device, 0, reg0);
            let is_multi_function = (dev0.header_type & 0x80) != 0;
            devices.push(dev0);

            // Only scan functions 1..=7 if bit 7 of Header Type is set
            if is_multi_function {
                for function in 1..8u8 {
                    let r0 = pci_read_config_dword(bus, device, function, 0x00);
                    if (r0 & 0xFFFF) as u16 == 0xFFFF {
                        continue;
                    }
                    devices.push(probe_function(bus, device, function, r0));
                }
            }
        }
    }

    info!("PCI Scanner: Enumerated {} devices across buses 0..=255.", devices.len());
    for dev in &devices {
        let bar0 = dev.bars[0];
        info!(
            "PCI [{:02x}:{:02x}.{}] ID: {:04x}:{:04x} (ProgIF {:02x}, BAR0 {:08x}) | {} ({})",
            dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id,
            dev.prog_if, bar0, dev.class_name(), dev.vendor_name()
        );
    }

    devices
}
