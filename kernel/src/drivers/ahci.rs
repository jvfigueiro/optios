// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: SATA AHCI 1.0/1.3 Read-Only Storage Driver
// Date: 2026-11-09

//! SATA AHCI 1.0/1.3 Read-Only Storage Driver for OptiOS.
//!
//! Provides robust, hardware-validated SATA drive inspection and ATA `IDENTIFY DEVICE`
//! interrogation with:
//! - Strict Read-Only guarantees by construction (`AllowedCommand` enum)
//! - BAR5 validation, 64-bit BAR rejection on Type-0 PCI headers, and sizing
//! - BIOS/OS handoff (BOHC) synchronization
//! - Time-based bounded polling using PIT (no CPU-speed-dependent spin loops)
//! - Persistent per-port DMA frames holding Command List, Received FIS, Command Table, and Buffer
//! - Dynamic slot selection via `NCS` and free-slot bitmask
//! - Automatic engine recovery on command abort or task file error

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{compiler_fence, AtomicBool, Ordering};

use optios_ahci_proto::{
    build_command_header, build_h2d_register_fis, build_prdt_entry, decode_identify_data,
    find_free_slot, AhciError, AllowedCommand,
};

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
    pub port_number: u8,
    pub reg_base: u64,
    pub dma_frame_phys: u64,
    pub dma_frame_virt: u64,
    pub ncs: u8,
    #[allow(dead_code)]
    pub s64a: bool,
    pub failed: AtomicBool,
}

impl AhciPort {
    /// Initializes and brings up a SATA port with dedicated DMA structures.
    pub unsafe fn init(
        port_number: u8,
        reg_base: u64,
        ncs: u8,
        s64a: bool,
    ) -> Result<Self, AhciError> {
        // Allocate 1 dedicated 4 KiB physical frame from PMM for the lifetime of this port.
        // Memory layout within the 4 KiB frame:
        // @0x000: Command List (1024 B = 32 slots * 32 B)  -> 1 KiB aligned
        // @0x400: Received FIS (256 B)                      -> 256 B aligned
        // @0x500: Command Table 0 (144 B)                  -> 128 B aligned
        // @0x800: Data Buffer (512 B)                       -> 512 B aligned
        let frame_phys = pmm::alloc_frame().ok_or(AhciError::NotPresent)?;
        let frame_virt = vmm::phys_to_virt::<u8>(frame_phys) as u64;

        core::ptr::write_bytes(frame_virt as *mut u8, 0, 4096);

        let clb_phys = frame_phys;
        let fb_phys = frame_phys + 0x400;
        let ctba0_phys = frame_phys + 0x500;
        let buf_phys = frame_phys + 0x800;

        // Verify alignment constraints per AHCI 1.3 spec
        assert!(clb_phys % 1024 == 0, "AHCI: CLB must be 1 KiB aligned");
        assert!(fb_phys % 256 == 0, "AHCI: FB must be 256 B aligned");
        assert!(ctba0_phys % 128 == 0, "AHCI: CTBA must be 128 B aligned");
        assert!(buf_phys % 2 == 0, "AHCI: Data buffer must be word aligned");

        // 32-bit DMA capability check: if S64A is 0, addresses must be < 4 GiB
        if !s64a {
            if clb_phys > 0xFFFF_FFFF
                || fb_phys > 0xFFFF_FFFF
                || ctba0_phys > 0xFFFF_FFFF
                || buf_phys > 0xFFFF_FFFF
            {
                warn!("AHCI: Controller lacks 64-bit DMA (S64A=0) but physical frame is above 4 GiB!");
                pmm::free_frame(frame_phys);
                return Err(AhciError::DmaAddressAbove4G);
            }
        }

        let port = Self {
            port_number,
            reg_base,
            dma_frame_phys: frame_phys,
            dma_frame_virt: frame_virt,
            ncs,
            s64a,
            failed: AtomicBool::new(false),
        };

        // Port bring-up sequence per AHCI spec
        port.bring_up()?;

        Ok(port)
    }

    /// Stops port DMA engines (ST and FRE) with bounded time timeouts.
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
        // 1. Clear ST and wait for CR=0 (up to 500 ms)
        // 2. Clear FRE and wait for FR=0 (up to 500 ms)
        let _ = self.stop_engine();

        let clb_phys = self.dma_frame_phys;
        let fb_phys = self.dma_frame_phys + 0x400;

        // 3. Write CLB/CLBU and FB/FBU (both 32-bit halves)
        mmio_write32(self.reg_base, 0x00, (clb_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(self.reg_base, 0x04, (clb_phys >> 32) as u32);
        mmio_write32(self.reg_base, 0x08, (fb_phys & 0xFFFF_FFFF) as u32);
        mmio_write32(self.reg_base, 0x0C, (fb_phys >> 32) as u32);

        // 4. Clear pending PxSERR and PxIS
        mmio_write32(self.reg_base, 0x30, 0xFFFF_FFFF);
        mmio_write32(self.reg_base, 0x10, 0xFFFF_FFFF);

        // 5. Ensure interrupts on this port are disabled (polling mode)
        mmio_write32(self.reg_base, 0x14, 0);

        // 6. Set FRE
        let mut cmd = mmio_read32(self.reg_base, 0x18);
        cmd |= HBA_PXCMD_FRE;
        mmio_write32(self.reg_base, 0x18, cmd);

        // 7. Wait until BSY and DRQ clear in PxTFD (timeout 1000 ms)
        time::wait_until(TIMEOUT_PORT_BUSY_MS, || {
            (mmio_read32(self.reg_base, 0x20) & (ATA_DEV_BSY | ATA_DEV_DRQ)) == 0
        })?;

        // 8. Set ST
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

    /// Executes a safe Read-Only ATA command via DMA.
    /// Only variants of `AllowedCommand` can be passed.
    pub unsafe fn execute_command(&self, cmd: AllowedCommand) -> Result<(), AhciError> {
        if self.failed.load(Ordering::SeqCst) {
            return Err(AhciError::PortFailed);
        }

        let slot = self.find_free_slot()?;
        let ctba_phys = self.dma_frame_phys + 0x500;
        let buf_phys = self.dma_frame_phys + 0x800;

        // Pointers inside the dedicated frame
        let cmd_hdr_ptr = (self.dma_frame_virt + (slot as u64 * 32)) as *mut u32;
        let cfis_ptr = (self.dma_frame_virt + 0x500) as *mut u8;
        let prdt_ptr = (self.dma_frame_virt + 0x580) as *mut u32;
        let buf_ptr = (self.dma_frame_virt + 0x800) as *mut u8;

        // Zero destination buffer
        core::ptr::write_bytes(buf_ptr, 0, 512);

        // Build CFIS (20 bytes)
        let cfis_slice = core::slice::from_raw_parts_mut(cfis_ptr, 20);
        build_h2d_register_fis(cmd, cfis_slice)?;

        // Build PRDT (1 entry for 512 bytes)
        let prdt_slice = core::slice::from_raw_parts_mut(prdt_ptr, 4);
        build_prdt_entry(buf_phys, 512, false, prdt_slice)?;

        // Build Command Header (32 bytes: dw0..dw3)
        // cfl=5 dwords (20 bytes), prdtl=1 (strictly read-only by builder construction)
        let cmd_hdr_slice = core::slice::from_raw_parts_mut(cmd_hdr_ptr, 4);
        build_command_header(5, 1, ctba_phys, cmd_hdr_slice)?;

        // Wait until port is not BSY / DRQ before issue (timeout 1000 ms)
        time::wait_until(TIMEOUT_PORT_BUSY_MS, || {
            (mmio_read32(self.reg_base, 0x20) & (ATA_DEV_BSY | ATA_DEV_DRQ)) == 0
        })?;

        // Memory fence before issuing command
        compiler_fence(Ordering::SeqCst);

        // Issue command slot via PxCI
        mmio_write32(self.reg_base, 0x38, 1u32 << slot);

        // Wait for command completion (PxCI bit cleared, or Task File Error / TFD error)
        let wait_res = time::wait_until(TIMEOUT_CMD_COMPLETE_MS, || {
            let ci = mmio_read32(self.reg_base, 0x38);
            let is = mmio_read32(self.reg_base, 0x10);
            let tfd = mmio_read32(self.reg_base, 0x20);
            (ci & (1u32 << slot)) == 0 || (is & HBA_PXIS_TFES) != 0 || (tfd & ATA_TFD_ERR) != 0
        });

        // Check for task file errors or timeout
        let is = mmio_read32(self.reg_base, 0x10);
        let tfd = mmio_read32(self.reg_base, 0x20);

        if wait_res.is_err() || (is & HBA_PXIS_TFES) != 0 || (tfd & ATA_TFD_ERR) != 0 {
            // Error recovery sequence per AHCI spec: stop engine, clear errors
            let _ = self.stop_engine();
            mmio_write32(self.reg_base, 0x30, 0xFFFF_FFFF); // Clear PxSERR
            mmio_write32(self.reg_base, 0x10, 0xFFFF_FFFF); // Clear PxIS

            // Check PxTFD after stopping the engine
            let tfd_after = mmio_read32(self.reg_base, 0x20);
            let bsy = (tfd_after & ATA_DEV_BSY) != 0;
            let drq = (tfd_after & ATA_DEV_DRQ) != 0;
            let err = (tfd_after & ATA_TFD_ERR) != 0;

            if bsy || drq {
                self.failed.store(true, Ordering::SeqCst);
                warn!(
                    "AHCI: Port {} hung after recovery (PxTFD=0x{:08x}: BSY={}, DRQ={}, ERR={}). Port marked Failed; ST not re-enabled.",
                    self.port_number, tfd_after, bsy, drq, err
                );
                return Err(AhciError::PortFailed);
            }

            // If BSY and DRQ are clear, restore FRE first, then ST (same order as bring_up per AHCI 1.3 spec)
            let mut pcmd = mmio_read32(self.reg_base, 0x18);
            pcmd |= HBA_PXCMD_FRE;
            mmio_write32(self.reg_base, 0x18, pcmd);

            pcmd = mmio_read32(self.reg_base, 0x18);
            pcmd |= HBA_PXCMD_ST;
            mmio_write32(self.reg_base, 0x18, pcmd);

            warn!(
                "AHCI: Port {} recovered engine after error (PxTFD=0x{:08x}: BSY={}, DRQ={}, ERR={}). FRE and ST re-enabled.",
                self.port_number, tfd_after, bsy, drq, err
            );

            if wait_res.is_err() {
                return Err(AhciError::Timeout);
            }
            return Err(AhciError::TaskFileError);
        }

        Ok(())
    }

    /// Reads the 256 words of IDENTIFY data from the data buffer.
    pub unsafe fn read_identify_buffer(&self) -> [u16; 256] {
        let mut words = [0u16; 256];
        let src = (self.dma_frame_virt + 0x800) as *const u16;
        for i in 0..256 {
            words[i] = read_volatile(src.add(i));
        }
        words
    }
}

/// Identified SATA disk drive metadata and S.M.A.R.T. telemetry.
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
}

/// Summary of the AHCI controller and all discovered SATA drives.
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
    pub atapi_ports_count: usize,
    #[allow(dead_code)]
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

/// Returns a reference to an initialized port by port number.
#[allow(dead_code)]
pub fn get_port(port_num: u8) -> Option<&'static AhciPort> {
    unsafe {
        let ports_ptr = &raw const PORTS;
        (*ports_ptr).iter().find(|p| p.port_number == port_num)
    }
}

/// Probes the PCI inventory for a SATA AHCI 1.0 controller, validates BAR5,
/// performs BOHC handoff, configures ports with dedicated DMA frames,
/// and queries ATA IDENTIFY DEVICE in Read-Only mode.
pub fn init(pci_devices: &[PciDevice]) -> AhciControllerInfo {
    let mut info = AhciControllerInfo::default();

    let Some(ahci_pci) = pci_devices
        .iter()
        .find(|d| d.class_code == 0x01 && d.subclass == 0x06 && d.prog_if == 0x01)
    else {
        warn!("AHCI: No SATA AHCI 1.0 controller (01:06:01) found on PCI bus.");
        return info;
    };

    info.present = true;
    info.pci_bus = ahci_pci.bus;
    info.pci_device = ahci_pci.device;
    info.pci_function = ahci_pci.function;
    info.vendor_id = ahci_pci.vendor_id;
    info.device_id = ahci_pci.device_id;

    // 1. Read initial PCI Command Register (16-bit word at offset 0x04)
    let orig_cmd = pci::pci_read_config_word(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x04);

    // 2. Validate ABAR (BAR5 at offset 0x24)
    let raw_bar5 = pci::pci_read_config_dword(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x24);

    // Must be Memory Space BAR (bit 0 == 0)
    if (raw_bar5 & 0x01) != 0 {
        warn!("AHCI: BAR5 is I/O space, not Memory space. Rejecting controller.");
        return info;
    }

    // Decode memory type bits (bits 2:1):
    // 00b = 32-bit BAR (valid for Type-0 PCI header BAR5)
    // 10b = 64-bit BAR (invalid because Type-0 PCI header has no BAR6 at 0x28)
    let bar_type = (raw_bar5 >> 1) & 0x03;
    if bar_type == 0x02 {
        warn!("AHCI: BAR5 reports 64-bit memory space, but Type-0 PCI header has no BAR6. Rejecting controller.");
        return info;
    }

    let abar_phys = (raw_bar5 & 0xFFFF_FFF0) as u64;
    if abar_phys == 0 {
        warn!("AHCI: Controller BAR5 (ABAR) is unallocated (0x0)!");
        return info;
    }
    info.abar_phys = abar_phys;

    // Size the BAR per PCI specification:
    // Temporarily disable Memory Space decoding (bit 1) and Bus Mastering (bit 2) via 16-bit write
    // so Status register write-1-to-clear bits at offset 0x06 are unaffected.
    let disabled_cmd = orig_cmd & !((1 << 1) | (1 << 2));
    pci::pci_write_config_word(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x04, disabled_cmd);

    // Write all 1s to determine BAR sizing mask, read back mask, and restore raw_bar5
    pci::pci_write_config_dword(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x24, 0xFFFF_FFFF);
    let size_mask = pci::pci_read_config_dword(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x24);
    pci::pci_write_config_dword(ahci_pci.bus, ahci_pci.device, ahci_pci.function, 0x24, raw_bar5);

    // Restore PCI Command Register with Memory Space (bit 1) and Bus Master (bit 2) enabled via 16-bit write
    pci::pci_write_config_word(
        ahci_pci.bus,
        ahci_pci.device,
        ahci_pci.function,
        0x04,
        orig_cmd | (1 << 1) | (1 << 2),
    );

    let bar_size = (!(size_mask & 0xFFFF_FFF0)).wrapping_add(1) as u64;
    info.bar_size = bar_size;

    // 3. Map ABAR MMIO registers uncached (WRITE_THROUGH | CACHE_DISABLE)
    // Initially map first page (4 KiB or bounded by bar_size) to access Generic Host Registers
    let initial_map_len = (bar_size as usize).min(4096);
    let abar_virt = match vmm::map_mmio(abar_phys, initial_map_len) {
        Ok(v) => v,
        Err(e) => {
            warn!("AHCI: Failed to map initial ABAR MMIO at 0x{:08x}: {}", abar_phys, e);
            return info;
        }
    };
    info.abar_virt = abar_virt;

    unsafe {
        // Read Ports Implemented (PI) register at offset 0x0C
        let pi = mmio_read32(abar_virt, 0x0C);
        info.ports_implemented = pi;

        // Map min(bar_size, 0x100 + 0x80 * (highest_implemented_port + 1)); never beyond the BAR
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
            "AHCI: ABAR registers span {} bytes (BAR size: {} bytes, highest port: {}); MMIO mapping is page-granular (4096 bytes).",
            map_len, bar_size, highest_implemented_port
        );

        // 4. Ensure AHCI Enable (GHC.AE, bit 31) is active and Global Interrupts disabled (GHC.IE = 0)
        let mut ghc = mmio_read32(abar_virt, 0x04);
        ghc |= HBA_GHC_AE;
        ghc &= !HBA_GHC_IE; // Polling mode only: no stray IRQs
        mmio_write32(abar_virt, 0x04, ghc);

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

        // BIOS/OS Handoff if supported.
        // NOTE: QEMU's ICH9 AHCI controller does not advertise BOH (CAP2.BOH = 0).
        // Therefore, this BIOS handoff code path is currently untested against physical UEFI firmware
        // in virtualized test environments and requires physical hardware validation in future milestones.
        if boh {
            let bohc = mmio_read32(abar_virt, 0x28);
            // Request OS ownership (set bit 1 = OOS)
            mmio_write32(abar_virt, 0x28, bohc | (1 << 1));
            // Wait with timeout for BIOS ownership to clear (bit 0 = BOS)
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

        // 5. Initialize implemented ports (0..31)
        for port_num in 0..32u8 {
            if (pi & (1 << port_num)) == 0 {
                continue;
            }

            let port_base = abar_virt + 0x100 + (port_num as u64 * 0x80);
            let ssts = mmio_read32(port_base, 0x28);
            let det = ssts & 0x0F;
            let ipm = (ssts >> 8) & 0x0F;

            // DET == 3 (Device detected & PHY established) and IPM == 1 (Active state)
            if det != 3 || ipm != 1 {
                continue;
            }

            let sig = mmio_read32(port_base, 0x24);
            if sig == SATA_SIG_ATA {
                match AhciPort::init(port_num, port_base, ncs, s64a) {
                    Ok(port) => {
                        // Issue IDENTIFY DEVICE (0xEC) in Read-Only mode
                        match port.execute_command(AllowedCommand::IdentifyDevice) {
                            Ok(()) => {
                                let id_words = port.read_identify_buffer();
                                let disk = decode_identify_data(&id_words);
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
                                info.drives.push(AhciDriveInfo {
                                    port_number: port_num,
                                    model: disk.model,
                                    serial: disk.serial,
                                    firmware: disk.firmware,
                                    total_sectors_lba48: disk.total_sectors,
                                    capacity_mib: disk.capacity_mib,
                                    smart_supported: disk.smart_supported,
                                    smart_enabled: disk.smart_enabled,
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
                info!("AHCI Port {}: SATAPI Optical Drive detected (SIG=0xEB140101).", port_num);
                // Also initialize the port structure for SATAPI so tests can probe it
                if let Ok(port) = AhciPort::init(port_num, port_base, ncs, s64a) {
                    let ports_raw = &raw mut PORTS;
                    (*ports_raw).push(port);
                }
            }
        }
    }

    info
}
