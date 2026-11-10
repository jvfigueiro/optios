// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: SATA AHCI 1.0/1.3 Read-Only Storage Driver & Sector DMA Engine
// Date: 2026-11-10

//! SATA AHCI 1.0/1.3 Read-Only Storage Driver for OptiOS.
//!
//! Provides robust, hardware-validated SATA drive inspection, ATA `IDENTIFY DEVICE`,
//! ATA `READ DMA EXT` (LBA48), and ATA `SMART READ DATA` interrogation with:
//! - Strict Read-Only guarantees by construction (`AllowedCommand` enum)
//! - Physical memory safety: sector reads chunked to max 8 sectors (4 KiB) per DMA transaction
//! - BAR5 validation, 64-bit BAR rejection on Type-0 PCI headers, and sizing
//! - BIOS/OS handoff (BOHC) synchronization
//! - Time-based bounded polling using PIT (no CPU-speed-dependent spin loops)
//! - Dedicated per-port DMA frames holding Command List, Received FIS, Command Table, and 4 KiB Data Buffer
//! - Dynamic slot selection via `NCS` and free-slot bitmask
//! - Automatic engine recovery on command abort or task file error
//! - MBR/GPT partition scanning via `optios-partition-table`
//! - `BlockDevice` trait implementation

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, AtomicBool, Ordering};

use optios_ahci_proto::{
    build_command_header, build_h2d_register_fis, build_prdt_entry, build_read_dma_ext_fis,
    build_smart_read_data_fis, decode_identify_data, find_free_slot, parse_smart_data, AhciError,
    AllowedCommand, BlockDevice, SmartTelemetry,
};
use optios_partition_table::{parse_gpt, parse_mbr, GptPartition};

use crate::drivers::pci::{self, PciDevice};
use crate::mm::{pmm, vmm};
use crate::time;
use crate::{info, warn};

pub const SATA_SIG_ATA: u32 = 0x0000_0101;
pub const SATA_SIG_ATAPI: u32 = 0xEB14_0101;

pub const HBA_GHC_AE: u32 = 1 << 31;
pub const HBA_GHC_IE: u32 = 1 << 1;

pub const HBA_PXCMD_ST: u32 = 1 << 0;
pub const HBA_PXCMD_FRE: u32 = 1 << 4;
pub const HBA_PXCMD_FR: u32 = 1 << 14;
pub const HBA_PXCMD_CR: u32 = 1 << 15;
pub const HBA_PXIS_TFES: u32 = 1 << 30;

pub const ATA_DEV_BSY: u32 = 0x80;
pub const ATA_DEV_DRQ: u32 = 0x08;
pub const ATA_TFD_ERR: u32 = 0x01;

pub const TIMEOUT_ENGINE_STOP_MS: u64 = 500;
pub const TIMEOUT_PORT_BUSY_MS: u64 = 1000;
pub const TIMEOUT_CMD_COMPLETE_MS: u64 = 1000;
pub const TIMEOUT_BOH_MS: u64 = 500;

/// Persistent per-port structure managing AHCI registers and dedicated DMA frames.
pub struct AhciPort {
    #[allow(dead_code)]
    pub port_number: u8,
    pub reg_base: u64,
    pub ctrl_frame_phys: u64,
    pub ctrl_frame_virt: u64,
    pub data_frame_phys: u64,
    pub data_frame_virt: u64,
    pub ncs: u8,
    #[allow(dead_code)]
    pub s64a: bool,
    pub failed: AtomicBool,
    pub total_sectors: u64,
}

impl AhciPort {
    /// Initializes and brings up a SATA port with dedicated control and data DMA frames.
    pub unsafe fn init(
        port_number: u8,
        reg_base: u64,
        ncs: u8,
        s64a: bool,
    ) -> Result<Self, AhciError> {
        // Allocate 1 dedicated 4 KiB physical frame for control structures:
        // @0x000: Command List (1024 B = 32 slots * 32 B)  -> 1 KiB aligned
        // @0x400: Received FIS (256 B)                      -> 256 B aligned
        // @0x500: Command Table 0 (144 B)                  -> 128 B aligned
        let ctrl_frame_phys = pmm::alloc_frame().ok_or(AhciError::NotPresent)?;
        let ctrl_frame_virt = vmm::phys_to_virt::<u8>(ctrl_frame_phys) as u64;
        core::ptr::write_bytes(ctrl_frame_virt as *mut u8, 0, 4096);

        // Allocate 1 dedicated 4 KiB physical frame for sector DMA transfers (8 sectors max per chunk)
        let data_frame_phys = pmm::alloc_frame().ok_or(AhciError::NotPresent)?;
        let data_frame_virt = vmm::phys_to_virt::<u8>(data_frame_phys) as u64;
        core::ptr::write_bytes(data_frame_virt as *mut u8, 0, 4096);

        let clb_phys = ctrl_frame_phys;
        let fb_phys = ctrl_frame_phys + 0x400;
        let ctba0_phys = ctrl_frame_phys + 0x500;

        assert!(clb_phys % 1024 == 0, "AHCI: CLB must be 1 KiB aligned");
        assert!(fb_phys % 256 == 0, "AHCI: FB must be 256 B aligned");
        assert!(ctba0_phys % 128 == 0, "AHCI: CTBA must be 128 B aligned");
        assert!(
            data_frame_phys % 512 == 0,
            "AHCI: Data buffer must be 512 B aligned"
        );

        if !s64a {
            if clb_phys > 0xFFFF_FFFF || fb_phys > 0xFFFF_FFFF || data_frame_phys > 0xFFFF_FFFF {
                return Err(AhciError::DmaAddressAbove4G);
            }
        }

        let port = Self {
            port_number,
            reg_base,
            ctrl_frame_phys,
            ctrl_frame_virt,
            data_frame_phys,
            data_frame_virt,
            ncs,
            s64a,
            failed: AtomicBool::new(false),
            total_sectors: 0,
        };

        port.bring_up()?;
        Ok(port)
    }

    /// Stops command and FIS receive engines per AHCI 1.3 spec Section 10.3.1.
    pub unsafe fn stop_engine(&self) -> Result<(), AhciError> {
        let mut cmd = mmio_read32(self.reg_base, 0x18);
        cmd &= !HBA_PXCMD_ST;
        mmio_write32(self.reg_base, 0x18, cmd);

        time::wait_until(TIMEOUT_ENGINE_STOP_MS, || {
            (mmio_read32(self.reg_base, 0x18) & HBA_PXCMD_CR) == 0
        })?;

        cmd = mmio_read32(self.reg_base, 0x18);
        cmd &= !HBA_PXCMD_FRE;
        mmio_write32(self.reg_base, 0x18, cmd);

        time::wait_until(TIMEOUT_ENGINE_STOP_MS, || {
            (mmio_read32(self.reg_base, 0x18) & HBA_PXCMD_FR) == 0
        })?;

        Ok(())
    }

    /// Brings up port DMA engines with programmed pointers.
    pub unsafe fn bring_up(&self) -> Result<(), AhciError> {
        let _ = self.stop_engine();

        let clb_phys = self.ctrl_frame_phys;
        let fb_phys = self.ctrl_frame_phys + 0x400;

        mmio_write32(self.reg_base, 0x00, (clb_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(self.reg_base, 0x04, (clb_phys >> 32) as u32);
        mmio_write32(self.reg_base, 0x08, (fb_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(self.reg_base, 0x0C, (fb_phys >> 32) as u32);

        mmio_write32(self.reg_base, 0x30, 0xFFFF_FFFF);
        mmio_write32(self.reg_base, 0x10, 0xFFFF_FFFF);
        mmio_write32(self.reg_base, 0x14, 0);

        let mut cmd = mmio_read32(self.reg_base, 0x18);
        cmd |= HBA_PXCMD_FRE;
        mmio_write32(self.reg_base, 0x18, cmd);

        time::wait_until(TIMEOUT_PORT_BUSY_MS, || {
            (mmio_read32(self.reg_base, 0x20) & (ATA_DEV_BSY | ATA_DEV_DRQ)) == 0
        })?;

        cmd = mmio_read32(self.reg_base, 0x18);
        cmd |= HBA_PXCMD_ST;
        mmio_write32(self.reg_base, 0x18, cmd);

        Ok(())
    }

    /// Selects an unoccupied command slot within `0..NCS`.
    pub fn find_free_slot(&self) -> Result<u8, AhciError> {
        unsafe {
            let ci = mmio_read32(self.reg_base, 0x38);
            let sact = mmio_read32(self.reg_base, 0x34);
            find_free_slot(ci | sact, self.ncs).ok_or(AhciError::NoFreeSlot)
        }
    }

    /// Executes an ATA command with specified payload size (up to 4096 bytes).
    unsafe fn dispatch_command(
        &self,
        slot: u8,
        byte_count: u32,
        build_fis: impl FnOnce(&mut [u8]) -> Result<(), AhciError>,
    ) -> Result<(), AhciError> {
        if self.failed.load(Ordering::SeqCst) {
            return Err(AhciError::PortFailed);
        }

        let ctba_phys = self.ctrl_frame_phys + 0x500;
        let cmd_hdr_ptr = (self.ctrl_frame_virt + (slot as u64 * 32)) as *mut u32;
        let cfis_ptr = (self.ctrl_frame_virt + 0x500) as *mut u8;
        let prdt_ptr = (self.ctrl_frame_virt + 0x580) as *mut u32;

        let cfis_slice = core::slice::from_raw_parts_mut(cfis_ptr, 20);
        build_fis(cfis_slice)?;

        let prdt_slice = core::slice::from_raw_parts_mut(prdt_ptr, 4);
        build_prdt_entry(self.data_frame_phys, byte_count, false, prdt_slice)?;

        let cmd_hdr_slice = core::slice::from_raw_parts_mut(cmd_hdr_ptr, 4);
        build_command_header(5, 1, ctba_phys, cmd_hdr_slice)?;

        time::wait_until(TIMEOUT_PORT_BUSY_MS, || {
            (mmio_read32(self.reg_base, 0x20) & (ATA_DEV_BSY | ATA_DEV_DRQ)) == 0
        })?;

        compiler_fence(Ordering::SeqCst);
        mmio_write32(self.reg_base, 0x38, 1u32 << slot);

        let wait_res = time::wait_until(TIMEOUT_CMD_COMPLETE_MS, || {
            let ci = mmio_read32(self.reg_base, 0x38);
            let is = mmio_read32(self.reg_base, 0x10);
            let tfd = mmio_read32(self.reg_base, 0x20);
            (ci & (1u32 << slot)) == 0 || (is & HBA_PXIS_TFES) != 0 || (tfd & ATA_TFD_ERR) != 0
        });

        let is = mmio_read32(self.reg_base, 0x10);
        let tfd = mmio_read32(self.reg_base, 0x20);

        if wait_res.is_err() || (is & HBA_PXIS_TFES) != 0 || (tfd & ATA_TFD_ERR) != 0 {
            let _ = self.stop_engine();
            mmio_write32(self.reg_base, 0x10, 0xFFFF_FFFF);
            mmio_write32(self.reg_base, 0x30, 0xFFFF_FFFF);

            let tfd_after = mmio_read32(self.reg_base, 0x20);
            let bsy = (tfd_after & ATA_DEV_BSY) != 0;
            let drq = (tfd_after & ATA_DEV_DRQ) != 0;

            if bsy || drq {
                self.failed.store(true, Ordering::SeqCst);
                return Err(AhciError::PortFailed);
            }

            let mut cmd_reg = mmio_read32(self.reg_base, 0x18);
            cmd_reg |= HBA_PXCMD_FRE;
            mmio_write32(self.reg_base, 0x18, cmd_reg);

            cmd_reg = mmio_read32(self.reg_base, 0x18);
            cmd_reg |= HBA_PXCMD_ST;
            mmio_write32(self.reg_base, 0x18, cmd_reg);

            if wait_res.is_err() {
                return Err(AhciError::Timeout);
            } else {
                return Err(AhciError::TaskFileError);
            }
        }

        Ok(())
    }

    /// Executes an ATA IDENTIFY DEVICE command.
    pub unsafe fn execute_command(&self, cmd: AllowedCommand) -> Result<(), AhciError> {
        let slot = self.find_free_slot()?;
        self.dispatch_command(slot, 512, |slice| build_h2d_register_fis(cmd, slice))
    }

    /// Executes an ATA READ DMA EXT command for `sector_count` sectors (1..=8).
    pub unsafe fn execute_read_dma_ext(
        &self,
        lba: u64,
        sector_count: u16,
    ) -> Result<(), AhciError> {
        if sector_count == 0 || sector_count > 8 {
            return Err(AhciError::BufferTooSmall);
        }
        let slot = self.find_free_slot()?;
        let bytes = sector_count as u32 * 512;
        self.dispatch_command(slot, bytes, |slice| {
            build_read_dma_ext_fis(lba, sector_count, slice)
        })
    }

    /// Reads arbitrary consecutive sectors using 8-sector (4 KiB) DMA chunks.
    pub unsafe fn read_sectors(
        &self,
        mut lba: u64,
        mut count: u32,
        buf: &mut [u8],
    ) -> Result<(), AhciError> {
        if buf.len() < (count as usize * 512) {
            return Err(AhciError::BufferTooSmall);
        }
        let mut buf_offset = 0;
        while count > 0 {
            let chunk_sectors = count.min(8) as u16;
            let chunk_bytes = chunk_sectors as usize * 512;
            self.execute_read_dma_ext(lba, chunk_sectors)?;
            let src = core::slice::from_raw_parts(self.data_frame_virt as *const u8, chunk_bytes);
            buf[buf_offset..buf_offset + chunk_bytes].copy_from_slice(src);
            buf_offset += chunk_bytes;
            lba += chunk_sectors as u64;
            count -= chunk_sectors as u32;
        }
        Ok(())
    }

    /// Executes ATA SMART READ DATA (`0xB0` / `0xD0`) and parses vital telemetry.
    pub unsafe fn read_smart_data(&self) -> Result<SmartTelemetry, AhciError> {
        let slot = self.find_free_slot()?;
        self.dispatch_command(slot, 512, |slice| build_smart_read_data_fis(slice))?;
        let src = core::slice::from_raw_parts(self.data_frame_virt as *const u8, 512);
        let mut raw_data = [0u8; 512];
        raw_data.copy_from_slice(src);
        Ok(parse_smart_data(&raw_data))
    }

    /// Reads the 512-byte payload from IDENTIFY DEVICE.
    pub unsafe fn read_identify_buffer(&self) -> [u16; 256] {
        let mut words = [0u16; 256];
        let buf_ptr = self.data_frame_virt as *const u16;
        for i in 0..256 {
            words[i] = read_volatile(buf_ptr.add(i));
        }
        words
    }
}

impl BlockDevice for AhciPort {
    fn read_sectors(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), AhciError> {
        unsafe { AhciPort::read_sectors(self, lba, count, buf) }
    }

    fn sector_size(&self) -> u32 {
        512
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}

/// Discovered and verified drive metadata.
#[derive(Debug, Clone)]
pub struct AhciDriveInfo {
    pub port_number: u8,
    pub model: String,
    pub serial: String,
    pub firmware: String,
    pub total_sectors_lba48: u64,
    pub capacity_mib: u64,
    pub smart_supported: bool,
    pub smart_enabled: bool,
    pub smart_telemetry: Option<SmartTelemetry>,
    pub partitions: Vec<GptPartition>,
    #[allow(dead_code)]
    pub has_gpt: bool,
    #[allow(dead_code)]
    pub has_mbr: bool,
}

/// Telemetry structure capturing AHCI controller capabilities and discovered drives.
#[derive(Debug, Clone)]
pub struct AhciControllerInfo {
    pub present: bool,
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub abar_phys: u64,
    pub abar_virt: u64,
    pub bar_size: u64,
    pub version_major: u16,
    pub version_minor: u16,
    pub ports_implemented: u32,
    pub atapi_ports_count: u32,
    pub s64a: bool,
    pub ncs: u8,
    pub boh: bool,
    pub drives: Vec<AhciDriveInfo>,
}

impl Default for AhciControllerInfo {
    fn default() -> Self {
        Self {
            present: false,
            pci_bus: 0,
            pci_device: 0,
            pci_function: 0,
            vendor_id: 0,
            device_id: 0,
            abar_phys: 0,
            abar_virt: 0,
            bar_size: 0,
            version_major: 0,
            version_minor: 0,
            ports_implemented: 0,
            atapi_ports_count: 0,
            s64a: false,
            ncs: 0,
            boh: false,
            drives: Vec::new(),
        }
    }
}

#[inline]
pub unsafe fn mmio_read32(base: u64, offset: usize) -> u32 {
    read_volatile((base + offset as u64) as *const u32)
}

#[inline]
pub unsafe fn mmio_write32(base: u64, offset: usize, val: u32) {
    write_volatile((base + offset as u64) as *mut u32, val);
}

/// Global list of initialized AHCI ports.
static mut PORTS: Vec<AhciPort> = Vec::new();

/// Global controller telemetry cache.
static mut CONTROLLER_INFO: Option<AhciControllerInfo> = None;

/// Returns a reference to an initialized port by port number.
#[allow(dead_code)]
pub fn get_port(port_num: u8) -> Option<&'static AhciPort> {
    unsafe {
        let ports_ptr = &raw const PORTS;
        (*ports_ptr).iter().find(|p| p.port_number == port_num)
    }
}

/// Returns a copy of the controller and drive metadata.
pub fn get_controller_info() -> Option<AhciControllerInfo> {
    unsafe {
        let info_ptr = &raw const CONTROLLER_INFO;
        (*info_ptr).clone()
    }
}

/// Probes the PCI inventory for a SATA AHCI 1.0 controller, validates BAR5,
/// performs BOHC handoff, configures ports with dedicated DMA frames,
/// interrogates drives via `IDENTIFY DEVICE`, reads S.M.A.R.T. telemetry,
/// and discovers GPT/MBR partition tables.
pub fn init(devices: &[PciDevice]) -> AhciControllerInfo {
    let mut info = AhciControllerInfo::default();

    let ahci_dev = devices
        .iter()
        .find(|d| d.class_code == 0x01 && d.subclass == 0x06 && d.prog_if == 0x01);

    let dev = match ahci_dev {
        Some(d) => d,
        None => {
            info!("AHCI: No SATA AHCI 1.0 controller found on PCI bus.");
            return info;
        }
    };

    info.present = true;
    info.pci_bus = dev.bus;
    info.pci_device = dev.device;
    info.pci_function = dev.function;
    info.vendor_id = dev.vendor_id;
    info.device_id = dev.device_id;

    unsafe {
        pci::enable_bus_mastering_and_mmio(dev.bus, dev.device, dev.function);

        let bar5_raw = pci::pci_read_config_dword(dev.bus, dev.device, dev.function, 0x24);
        if (bar5_raw & 0x01) != 0 {
            warn!("AHCI: BAR5 is I/O space; AHCI requires 32-bit MMIO.");
            return info;
        }

        let bar_type = (bar5_raw >> 1) & 0x03;
        if bar_type == 0x02 {
            warn!("AHCI: 64-bit BAR5 detected on Type-0 PCI header; unsupported in 32-bit MMIO.");
            return info;
        }

        let abar_phys = (bar5_raw & 0xFFFF_FFF0) as u64;
        if abar_phys == 0 {
            warn!("AHCI: ABAR physical address is null.");
            return info;
        }
        info.abar_phys = abar_phys;

        let orig_cmd = pci::pci_read_config_word(dev.bus, dev.device, dev.function, 0x04);
        pci::pci_write_config_word(dev.bus, dev.device, dev.function, 0x04, orig_cmd & !0x0002);

        pci::pci_write_config_dword(dev.bus, dev.device, dev.function, 0x24, 0xFFFF_FFFF);
        let size_mask = pci::pci_read_config_dword(dev.bus, dev.device, dev.function, 0x24);
        pci::pci_write_config_dword(dev.bus, dev.device, dev.function, 0x24, bar5_raw);
        pci::pci_write_config_word(dev.bus, dev.device, dev.function, 0x04, orig_cmd);

        let bar_size = (!(size_mask & 0xFFFF_FFF0)).wrapping_add(1) as u64;
        info.bar_size = bar_size;

        let initial_map_len = (bar_size as usize).min(4096);
        let abar_virt = match vmm::map_mmio(abar_phys, initial_map_len) {
            Ok(v) => v,
            Err(e) => {
                warn!("AHCI: Failed to map initial ABAR MMIO at 0x{:08x}: {}", abar_phys, e);
                return info;
            }
        };
        info.abar_virt = abar_virt;

        let pi = mmio_read32(abar_virt, 0x0C);
        info.ports_implemented = pi;

        let highest_implemented_port = if pi == 0 { 0 } else { 31 - pi.leading_zeros() as u64 };
        let needed_len = 0x100 + 0x80 * (highest_implemented_port + 1);
        let map_len = bar_size.min(needed_len);

        if map_len > initial_map_len as u64 {
            if let Err(e) = vmm::map_mmio(abar_phys, map_len as usize) {
                warn!("AHCI: Failed to map full ABAR MMIO ({} bytes): {}", map_len, e);
                return info;
            }
        }

        info!(
            "AHCI: ABAR registers span {} bytes (BAR size: {} bytes, highest port: {}); MMIO mapping is page-granular ({} bytes).",
            needed_len,
            bar_size,
            highest_implemented_port,
            ((map_len + 4095) / 4096) * 4096
        );

        let ghc = mmio_read32(abar_virt, 0x04);
        mmio_write32(abar_virt, 0x04, (ghc | HBA_GHC_AE) & !HBA_GHC_IE);

        let vs = mmio_read32(abar_virt, 0x10);
        info.version_major = ((vs >> 16) & 0xFFFF) as u16;
        info.version_minor = (vs & 0xFFFF) as u16;

        let cap = mmio_read32(abar_virt, 0x00);
        let s64a = (cap & (1 << 31)) != 0;
        let ncs = (((cap >> 8) & 0x1F) + 1) as u8;
        info.s64a = s64a;
        info.ncs = ncs;

        let cap2 = mmio_read32(abar_virt, 0x24);
        let boh = (cap2 & 1) != 0;
        info.boh = boh;

        if boh {
            let bohc = mmio_read32(abar_virt, 0x28);
            mmio_write32(abar_virt, 0x28, bohc | (1 << 1));
            let handoff_res = time::wait_until(TIMEOUT_BOH_MS, || {
                (mmio_read32(abar_virt, 0x28) & 1) == 0
            });
            if handoff_res.is_ok() {
                info!("AHCI: BIOS/OS handoff (BOHC) completed successfully.");
            } else {
                warn!("AHCI: BIOS/OS handoff (BOHC) timed out waiting for BOS to clear.");
            }
        } else {
            info!("AHCI: BIOS/OS handoff (BOHC) not supported by hardware (untested in QEMU).");
        }

        let pi = mmio_read32(abar_virt, 0x0C);
        info.ports_implemented = pi;

        info!(
            "AHCI Controller [{:02x}:{:02x}.{}] ({:04x}:{:04x}): ABAR phys 0x{:08x} -> virt 0x{:016x} (Uncached PWT|PCD), AHCI v{:x}.{:02x}, PI=0x{:08x}",
            info.pci_bus,
            info.pci_device,
            info.pci_function,
            info.vendor_id,
            info.device_id,
            info.abar_phys,
            info.abar_virt,
            info.version_major,
            info.version_minor,
            info.ports_implemented
        );
        info!(
            "AHCI Capabilities: S64A={}, NCS={}, BOH={}, BAR Size={} KiB",
            s64a,
            ncs,
            boh,
            bar_size / 1024
        );

        for port_num in 0..32u8 {
            if (pi & (1 << port_num)) == 0 {
                continue;
            }

            let port_base = abar_virt + 0x100 + (port_num as u64 * 0x80);
            let ssts = mmio_read32(port_base, 0x28);
            let det = ssts & 0x0F;
            let ipm = (ssts >> 8) & 0x0F;

            if det != 3 || ipm != 1 {
                continue;
            }

            let sig = mmio_read32(port_base, 0x24);
            if sig == SATA_SIG_ATA {
                match AhciPort::init(port_num, port_base, ncs, s64a) {
                    Ok(mut port) => {
                        match port.execute_command(AllowedCommand::IdentifyDevice) {
                            Ok(()) => {
                                let id_words = port.read_identify_buffer();
                                let disk = decode_identify_data(&id_words);
                                port.total_sectors = disk.total_sectors;

                                info!(
                                    "AHCI Port {}: SATA Disk Model='{}', Serial='{}', FW='{}', Sectors={} ({} MiB), SMART=[Supported: {}, Enabled: {}] (Read-Only)",
                                    port_num,
                                    disk.model,
                                    disk.serial,
                                    disk.firmware,
                                    disk.total_sectors,
                                    disk.capacity_mib,
                                    disk.smart_supported,
                                    disk.smart_enabled
                                );

                                // Probe S.M.A.R.T. telemetry if supported
                                let smart_telemetry = if disk.smart_supported {
                                    match port.read_smart_data() {
                                        Ok(tel) => {
                                            info!(
                                                "AHCI Port {}: S.M.A.R.T. Telemetry: Temp={:?}C, POH={:?}, Cycles={:?}, Realloc={:?}",
                                                port_num,
                                                tel.temperature_c,
                                                tel.power_on_hours,
                                                tel.power_cycles,
                                                tel.reallocated_sectors
                                            );
                                            Some(tel)
                                        }
                                        Err(e) => {
                                            warn!(
                                                "AHCI Port {}: SMART READ DATA returned {:?}",
                                                port_num, e
                                            );
                                            None
                                        }
                                    }
                                } else {
                                    None
                                };

                                // Scan MBR/GPT partition tables
                                let mut mbr_sector = [0u8; 512];
                                let mut partitions = Vec::new();
                                let mut has_mbr = false;
                                let mut has_gpt = false;

                                if let Ok(()) = port.read_sectors(0, 1, &mut mbr_sector) {
                                    if let Ok(mbr) = parse_mbr(&mbr_sector) {
                                        has_mbr = true;
                                        if mbr.is_protective {
                                            info!(
                                                "AHCI Port {}: Partition Table: Protective MBR found (type 0xEE).",
                                                port_num
                                            );
                                            let mut gpt_hdr_sector = [0u8; 512];
                                            if let Ok(()) =
                                                port.read_sectors(1, 1, &mut gpt_hdr_sector)
                                            {
                                                let mut entries_buf = alloc::vec![0u8; 16384];
                                                if let Ok(()) =
                                                    port.read_sectors(2, 32, &mut entries_buf)
                                                {
                                                    if let Ok(gpt) = parse_gpt(
                                                        &gpt_hdr_sector,
                                                        &entries_buf,
                                                    ) {
                                                        has_gpt = true;
                                                        info!(
                                                            "AHCI Port {}: GPT Header verified successfully (CRC32 valid, {} partitions).",
                                                            port_num,
                                                            gpt.partitions.len()
                                                        );
                                                        info!(
                                                            "AHCI Port {}: Valid GPT partition table found ({} partitions):",
                                                            port_num,
                                                            gpt.partitions.len()
                                                        );
                                                        for part in &gpt.partitions {
                                                            info!(
                                                                "  -> Partition {}: '{}' ({}) LBA {}..{} ({} MiB)",
                                                                part.index,
                                                                part.name,
                                                                part.type_name,
                                                                part.start_lba,
                                                                part.end_lba,
                                                                part.size_mib
                                                            );
                                                        }
                                                        partitions = gpt.partitions;
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }

                                info.drives.push(AhciDriveInfo {
                                    port_number: port_num,
                                    model: disk.model,
                                    serial: disk.serial,
                                    firmware: disk.firmware,
                                    total_sectors_lba48: disk.total_sectors,
                                    capacity_mib: disk.capacity_mib,
                                    smart_supported: disk.smart_supported,
                                    smart_enabled: disk.smart_enabled,
                                    smart_telemetry,
                                    partitions,
                                    has_gpt,
                                    has_mbr,
                                });
                            }
                            Err(e) => {
                                warn!("AHCI: Port {} IDENTIFY DEVICE failed: {:?}", port_num, e);
                            }
                        }
                        let ports_raw = &raw mut PORTS;
                        (*ports_raw).push(port);
                    }
                    Err(e) => {
                        warn!("AHCI: Failed to initialize Port {}: {:?}", port_num, e);
                    }
                }
            } else if sig == SATA_SIG_ATAPI {
                info.atapi_ports_count += 1;
                info!(
                    "AHCI Port {}: SATAPI Optical Drive detected (SIG=0xEB140101).",
                    port_num
                );
                if let Ok(port) = AhciPort::init(port_num, port_base, ncs, s64a) {
                    let ports_raw = &raw mut PORTS;
                    (*ports_raw).push(port);
                }
            }
        }

        let ctrl_ptr = &raw mut CONTROLLER_INFO;
        *ctrl_ptr = Some(info.clone());
    }

    info
}
