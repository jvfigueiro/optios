// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: USB 3.0 xHCI Host Controller Native Driver (Slice U1)
// Date: 2026-11-10

//! USB 3.0 eXtensible Host Controller Interface (xHCI) Native Driver.
//! Implements hardware initialization, BIOS/OS handoff, DMA ring setup,
//! Command Ring No-Op verification, and Root Hub port enumeration.
//! Strictly implemented using the OptiOS Driver Kit (`MmioRegion` & `DmaBuffer`).

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{compiler_fence, Ordering};

use optios_xhci_proto::*;

use crate::drivers::kit::{DmaBuffer, MmioRegion};
use crate::drivers::kit::registry::{Diagnosable, DriverMetadata, DriverOrigin, register_driver};
use crate::drivers::pci::{self, PciDevice};
use crate::mm::vmm;
use crate::time;
use crate::{info, warn};

/// Root Hub port discovery information.
#[derive(Debug, Clone)]
pub struct XhciPortInfo {
    pub port_number: u8,
    pub connected: bool,
    pub enabled: bool,
    pub speed: &'static str,
}

/// Discovered and initialized xHCI controller telemetry.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct XhciControllerInfo {
    pub present: bool,
    pub pci_bus: u8,
    pub pci_device: u8,
    pub pci_function: u8,
    pub vendor_id: u16,
    pub device_id: u16,
    pub bar0_phys: u64,
    pub bar0_virt: u64,
    pub bar_size: u64,
    pub hci_version: u16,
    pub max_slots: u8,
    pub max_intrs: u16,
    pub max_ports: u8,
    pub dma_test_success: bool,
    pub ports: Vec<XhciPortInfo>,
}

struct XhciDiagnostic;
impl Diagnosable for XhciDiagnostic {
    fn health_status(&self) -> &'static str {
        unsafe {
            if let Some(ref info) = XHCI_INFO {
                if info.dma_test_success { "OK" } else { "Degraded" }
            } else {
                "NotPresent"
            }
        }
    }

    fn diagnostics(&self) -> String {
        unsafe {
            if let Some(ref info) = XHCI_INFO {
                let connected_count = info.ports.iter().filter(|p| p.connected).count();
                format!(
                    "PCI {:02x}:{:02x}.{}, v{:x}.{:02x}, Ports: {} total ({} connected)",
                    info.pci_bus,
                    info.pci_device,
                    info.pci_function,
                    info.hci_version >> 8,
                    info.hci_version & 0xFF,
                    info.max_ports,
                    connected_count
                )
            } else {
                String::from("No xHCI controller detected")
            }
        }
    }
}

static XHCI_DIAG: XhciDiagnostic = XhciDiagnostic;
static mut XHCI_INFO: Option<XhciControllerInfo> = None;

// Persistent DMA structures kept alive for the lifetime of the kernel
static mut XHCI_DCBAA: Option<DmaBuffer> = None;
static mut XHCI_CMD_RING: Option<DmaBuffer> = None;
static mut XHCI_EVENT_RING: Option<DmaBuffer> = None;
static mut XHCI_ERST: Option<DmaBuffer> = None;

/// Formats the xHCI controller status and discovered ports into `/proc/usb`.
pub fn format_proc_usb() -> String {
    use core::fmt::Write;
    let mut out = String::new();

    unsafe {
        let Some(ref info) = XHCI_INFO else {
            out.push_str("No USB Host Controllers registered in system.\n");
            return out;
        };

        let _ = writeln!(
            out,
            "USB 3.0 xHCI Controller [{:02x}:{:02x}.{}] ({:04x}:{:04x}):",
            info.pci_bus, info.pci_device, info.pci_function, info.vendor_id, info.device_id
        );
        let _ = writeln!(
            out,
            "  HCI Version: {:x}.{:02x} | MaxSlots: {} | MaxPorts: {} | BAR0: 0x{:08x} ({} KiB)",
            info.hci_version >> 8, info.hci_version & 0xFF, info.max_slots, info.max_ports, info.bar0_phys, info.bar_size / 1024
        );
        let _ = writeln!(out, "  DMA Command Ring Verification: {}", if info.dma_test_success { "PASSED (Completion Code 1)" } else { "FAILED" });
        let _ = writeln!(out, "\nROOT HUB PORTS:");
        let _ = writeln!(out, "PORT  CONNECTED  ENABLED  SPEED");
        let _ = writeln!(out, "----  ---------  -------  -----");

        for p in &info.ports {
            let conn_str = if p.connected { "YES" } else { "NO" };
            let en_str = if p.enabled { "YES" } else { "NO" };
            let _ = writeln!(out, "{:<5} {:<10} {:<8} {}", p.port_number, conn_str, en_str, p.speed);
        }
    }

    out
}

/// Probes PCI devices for an xHCI USB 3.0 controller, initializes hardware,
/// brings up DMA Command/Event rings via Driver Kit, executes a verification No-Op,
/// and discovers attached USB Root Hub ports.
pub fn init(devices: &[PciDevice]) -> Option<XhciControllerInfo> {
    let xhci_dev = devices
        .iter()
        .find(|d| d.class_code == 0x0C && d.subclass == 0x03 && d.prog_if == 0x30);

    let dev = match xhci_dev {
        Some(d) => d,
        None => {
            info!("xHCI: No USB 3.0 xHCI controller found on PCI bus.");
            return None;
        }
    };

    info!(
        "xHCI: Controller found on PCI [{:02x}:{:02x}.{}] (Vendor {:04x}, Device {:04x}).",
        dev.bus, dev.device, dev.function, dev.vendor_id, dev.device_id
    );

    unsafe {
        pci::enable_bus_mastering_and_mmio(dev.bus, dev.device, dev.function);

        // Read BAR0 (MMIO 64-bit)
        let bar0_low = pci::pci_read_config_dword(dev.bus, dev.device, dev.function, 0x10);
        let is_64bit = ((bar0_low >> 1) & 0x03) == 0x02;
        let bar0_high = if is_64bit {
            pci::pci_read_config_dword(dev.bus, dev.device, dev.function, 0x14)
        } else {
            0
        };

        let bar0_phys = (((bar0_high as u64) << 32) | ((bar0_low & 0xFFFF_FFF0) as u64)) as u64;
        if bar0_phys == 0 {
            warn!("xHCI: BAR0 physical address is null.");
            return None;
        }

        // Size BAR0
        let orig_cmd = pci::pci_read_config_word(dev.bus, dev.device, dev.function, 0x04);
        pci::pci_write_config_word(dev.bus, dev.device, dev.function, 0x04, orig_cmd & !0x0002);

        pci::pci_write_config_dword(dev.bus, dev.device, dev.function, 0x10, 0xFFFF_FFFF);
        let size_mask_low = pci::pci_read_config_dword(dev.bus, dev.device, dev.function, 0x10);
        pci::pci_write_config_dword(dev.bus, dev.device, dev.function, 0x10, bar0_low);

        pci::pci_write_config_word(dev.bus, dev.device, dev.function, 0x04, orig_cmd);

        let mut bar_size = (!(size_mask_low & 0xFFFF_FFF0)).wrapping_add(1) as u64;
        if bar_size == 0 || bar_size > 0x1000_0000 {
            bar_size = 65536; // Fallback to 64 KiB
        }
        let bar_size = bar_size.max(4096);

        let bar0_virt = match vmm::map_mmio(bar0_phys, bar_size as usize) {
            Ok(v) => v,
            Err(e) => {
                warn!("xHCI: Failed to map MMIO at 0x{:08x}: {}", bar0_phys, e);
                return None;
            }
        };

        let mmio = MmioRegion::new(bar0_virt, bar_size as usize);

        let caplength = mmio.read_u8(CAP_CAPLENGTH) as usize;
        let hci_version = mmio.read_u16(CAP_HCIVERSION);
        let hcsparams1 = mmio.read_u32(CAP_HCSPARAMS1);
        let hccparams1 = mmio.read_u32(CAP_HCCPARAMS1);
        let dboff = mmio.read_u32(CAP_DBOFF) as usize;
        let rtsoff = mmio.read_u32(CAP_RTSOFF) as usize;

        let (max_slots, max_intrs, max_ports) = parse_hcsparams1(hcsparams1);

        info!(
            "xHCI Controller [{:02x}:{:02x}.{}]: BAR0 phys 0x{:08x} -> virt 0x{:016x} ({} KiB), v{:x}.{:02x}, MaxSlots={}, MaxIntrs={}, MaxPorts={}",
            dev.bus, dev.device, dev.function, bar0_phys, bar0_virt, bar_size / 1024,
            hci_version >> 8, hci_version & 0xFF, max_slots, max_intrs, max_ports
        );

        // -------------------------------------------------------------------
        // 1. BIOS/OS Handoff Synchronization (Extended Capabilities)
        // -------------------------------------------------------------------
        let xecp = ((hccparams1 >> 16) & 0xFFFF) as usize;
        if xecp != 0 {
            let mut cap_offset = xecp * 4;
            while cap_offset + 4 <= mmio.size() {
                let cap_val = mmio.read_u32(cap_offset);
                let cap_id = (cap_val & 0xFF) as u8;
                let next = ((cap_val >> 8) & 0xFF) as usize;

                if cap_id == EXT_CAP_USB_LEGACY {
                    info!("xHCI: USB Legacy Support Capability found at offset 0x{:x}.", cap_offset);
                    // Claim OS ownership
                    mmio.write_u32(cap_offset, cap_val | USBLEGSUP_OS_OWNED);

                    let handoff_res = time::wait_until(500, || {
                        (mmio.read_u32(cap_offset) & USBLEGSUP_BIOS_OWNED) == 0
                    });

                    if handoff_res.is_ok() {
                        info!("xHCI: BIOS/OS handoff successfully acquired ownership.");
                    } else {
                        warn!("xHCI: BIOS/OS handoff timed out waiting for BIOS semaphore.");
                    }
                    break;
                }

                if next == 0 {
                    break;
                }
                cap_offset += next * 4;
            }
        }

        // Operational Registers window
        if caplength + 0x400 > mmio.size() {
            warn!("xHCI: Operational registers exceed mapped MMIO window.");
            return None;
        }
        let op_regs = mmio.sub_region(caplength, mmio.size() - caplength);

        // -------------------------------------------------------------------
        // 2. Halt & Reset Controller
        // -------------------------------------------------------------------
        let mut usbcmd = op_regs.read_u32(OP_USBCMD);
        if (usbcmd & USBCMD_RS) != 0 {
            op_regs.write_u32(OP_USBCMD, usbcmd & !USBCMD_RS);
            let _ = time::wait_until(500, || (op_regs.read_u32(OP_USBSTS) & USBSTS_HCH) != 0);
        }

        // Issue Host Controller Reset
        op_regs.write_u32(OP_USBCMD, USBCMD_HCRST);
        let reset_res = time::wait_until(1000, || {
            (op_regs.read_u32(OP_USBCMD) & USBCMD_HCRST) == 0
                && (op_regs.read_u32(OP_USBSTS) & USBSTS_CNR) == 0
        });

        if reset_res.is_err() {
            warn!("xHCI: Host Controller Reset timed out.");
            return None;
        }
        info!("xHCI: Host Controller Reset completed successfully.");

        // -------------------------------------------------------------------
        // 3. Allocate DMA Structures via Driver Kit
        // -------------------------------------------------------------------
        // MaxSlots enabled
        op_regs.write_u32(OP_CONFIG, max_slots as u32);

        // DCBAA (Device Context Base Address Array): 64-byte aligned
        let mut dcbaa = match DmaBuffer::allocate_4k() {
            Some(b) => b,
            None => {
                warn!("xHCI: Failed to allocate DMA buffer for DCBAA.");
                return None;
            }
        };
        dcbaa.zero();
        op_regs.write_u64(OP_DCBAAP, dcbaa.phys_addr());

        // Command Ring: 64 TRBs (1024 bytes), with Link TRB at index 63
        let mut cmd_ring = match DmaBuffer::allocate_4k() {
            Some(b) => b,
            None => {
                warn!("xHCI: Failed to allocate DMA buffer for Command Ring.");
                return None;
            }
        };
        cmd_ring.zero();

        // Write Link TRB at index 63 pointing back to ring base, with Toggle Cycle = true, Cycle = true
        let link_trb = build_link_trb(cmd_ring.phys_addr(), true, true);
        let link_bytes = core::slice::from_raw_parts(
            &link_trb as *const Trb as *const u8,
            core::mem::size_of::<Trb>(),
        );
        let link_offset = 63 * core::mem::size_of::<Trb>();
        cmd_ring.as_mut_slice()[link_offset..link_offset + 16].copy_from_slice(link_bytes);

        // Write CRCR (Command Ring Control Register) with initial Cycle State = 1
        op_regs.write_u64(OP_CRCR, cmd_ring.phys_addr() | CRCR_RCS);

        // Event Ring (Segment size: 64 TRBs = 1024 bytes)
        let mut event_ring = match DmaBuffer::allocate_4k() {
            Some(b) => b,
            None => {
                warn!("xHCI: Failed to allocate DMA buffer for Event Ring.");
                return None;
            }
        };
        event_ring.zero();

        // ERST (Event Ring Segment Table)
        let mut erst = match DmaBuffer::allocate_4k() {
            Some(b) => b,
            None => {
                warn!("xHCI: Failed to allocate DMA buffer for ERST.");
                return None;
            }
        };
        erst.zero();

        let erst_entry = ErstEntry::new(event_ring.phys_addr(), 64);
        let erst_bytes = core::slice::from_raw_parts(
            &erst_entry as *const ErstEntry as *const u8,
            core::mem::size_of::<ErstEntry>(),
        );
        erst.as_mut_slice()[0..16].copy_from_slice(erst_bytes);

        // Program Interrupter 0
        if rtsoff + 0x40 > mmio.size() {
            warn!("xHCI: Runtime register space exceeds MMIO window.");
            return None;
        }
        let intr0 = mmio.sub_region(rtsoff + 0x20, 0x20);
        intr0.write_u32(INTR_ERSTSZ, 1);
        intr0.write_u64(INTR_ERSTBA, erst.phys_addr());
        intr0.write_u64(INTR_ERDP, event_ring.phys_addr() | ERDP_EHB);
        intr0.write_u32(INTR_IMAN, IMAN_IP | IMAN_IE);

        // -------------------------------------------------------------------
        // 4. Start Host Controller
        // -------------------------------------------------------------------
        usbcmd = op_regs.read_u32(OP_USBCMD);
        usbcmd |= USBCMD_RS | USBCMD_INTE;
        op_regs.write_u32(OP_USBCMD, usbcmd);

        let start_res = time::wait_until(500, || (op_regs.read_u32(OP_USBSTS) & USBSTS_HCH) == 0);
        if start_res.is_err() {
            warn!("xHCI: Controller failed to clear HCH (Halted) flag.");
            return None;
        }
        info!("xHCI: Controller running (USBCMD.RS=1, USBSTS.HCH=0).");

        // -------------------------------------------------------------------
        // 5. DMA Proof of Life: No-Op Command TRB Verification
        // -------------------------------------------------------------------
        let mut dma_test_success = false;
        if dboff + 4 <= mmio.size() {
            let db_regs = mmio.sub_region(dboff, 256 * 4);

            // Command TRB 0: No-Op Command, Cycle = 1
            let noop_trb = build_noop_cmd(true);
            let noop_bytes = core::slice::from_raw_parts(
                &noop_trb as *const Trb as *const u8,
                core::mem::size_of::<Trb>(),
            );
            cmd_ring.as_mut_slice()[0..16].copy_from_slice(noop_bytes);

            compiler_fence(Ordering::SeqCst);

            // Ring Doorbell 0 for Host Controller Command Ring (Target = 0)
            db_regs.write_u32(0, 0);

            // Poll Event Ring 0 for completion
            let wait_event = time::wait_until(1000, || {
                let ev_slice = &event_ring.as_slice()[0..16];
                let ev_trb: Trb = core::ptr::read_unaligned(ev_slice.as_ptr() as *const Trb);
                ev_trb.cycle()
            });

            if wait_event.is_ok() {
                let ev_slice = &event_ring.as_slice()[0..16];
                let ev_trb: Trb = core::ptr::read_unaligned(ev_slice.as_ptr() as *const Trb);
                if ev_trb.trb_type() == TRB_TYPE_COMMAND_COMPLETION && ev_trb.completion_code() == TRB_COMP_SUCCESS {
                    dma_test_success = true;
                    info!(
                        "xHCI: Command Ring DMA verified! CommandCompletionEvent received: Code={} (Success), Slot={}.",
                        ev_trb.completion_code(), ev_trb.slot_id()
                    );
                    // Acknowledge event by updating ERDP
                    intr0.write_u64(INTR_ERDP, (event_ring.phys_addr() + 16) | ERDP_EHB);
                } else {
                    warn!(
                        "xHCI: Unexpected Event TRB type {} code {}",
                        ev_trb.trb_type(), ev_trb.completion_code()
                    );
                }
            } else {
                warn!("xHCI: Timed out waiting for CommandCompletionEvent on Event Ring.");
            }
        }

        // -------------------------------------------------------------------
        // 6. Enumerate Root Hub Ports
        // -------------------------------------------------------------------
        let mut port_infos = Vec::new();
        for port_num in 1..=max_ports {
            let portsc_offset = OP_PORTSC_BASE + ((port_num as usize - 1) * 0x10);
            if portsc_offset + 4 <= op_regs.size() {
                let portsc = op_regs.read_u32(portsc_offset);
                let status = parse_portsc(portsc);

                if status.connected {
                    info!(
                        "xHCI Root Hub Port {}: Connected=true, Enabled={}, Speed={} (PORTSC=0x{:08x})",
                        port_num, status.enabled, status.speed_name(), portsc
                    );
                } else {
                    info!(
                        "xHCI Root Hub Port {}: Connected=false, Speed={} (PORTSC=0x{:08x})",
                        port_num, status.speed_name(), portsc
                    );
                }

                port_infos.push(XhciPortInfo {
                    port_number: port_num,
                    connected: status.connected,
                    enabled: status.enabled,
                    speed: status.speed_name(),
                });
            }
        }

        let info_obj = XhciControllerInfo {
            present: true,
            pci_bus: dev.bus,
            pci_device: dev.device,
            pci_function: dev.function,
            vendor_id: dev.vendor_id,
            device_id: dev.device_id,
            bar0_phys,
            bar0_virt,
            bar_size,
            hci_version,
            max_slots,
            max_intrs,
            max_ports,
            dma_test_success,
            ports: port_infos,
        };

        XHCI_DCBAA = Some(dcbaa);
        XHCI_CMD_RING = Some(cmd_ring);
        XHCI_EVENT_RING = Some(event_ring);
        XHCI_ERST = Some(erst);

        XHCI_INFO = Some(info_obj.clone());

        // Register xHCI driver in central Driver Registry
        register_driver(
            DriverMetadata {
                name: "xhci",
                spdx_license: "GPL-2.0-or-later",
                origin: DriverOrigin::Native,
                version: "1.2.0",
            },
            Some(&XHCI_DIAG),
        );

        Some(info_obj)
    }
}
