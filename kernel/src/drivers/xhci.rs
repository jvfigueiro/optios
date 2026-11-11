// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: USB 3.0 xHCI Host Controller Native Driver (Slice U2)
// Date: 2026-11-10

//! USB 3.0 eXtensible Host Controller Interface (xHCI) Native Driver.
//! Implements hardware initialization, BIOS/OS handoff, DMA ring setup,
//! Command Ring No-Op verification, Root Hub port enumeration, Port Reset,
//! Slot Addressing, and EP0 Device Descriptor Enumeration.
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

/// Enumerated USB device metadata.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct XhciUsbDevice {
    pub slot_id: u8,
    pub port_number: u8,
    pub speed: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    pub device_class: u8,
    pub device_subclass: u8,
    pub device_protocol: u8,
    pub interface_class: u8,
    pub interface_subclass: u8,
    pub interface_protocol: u8,
    pub configuration_val: u8,
    pub max_packet_size: u16,
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
    pub devices: Vec<XhciUsbDevice>,
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
                let dev_count = info.devices.len();
                format!(
                    "PCI {:02x}:{:02x}.{}, v{:x}.{:02x}, Ports: {} total ({} connected), Devices: {}",
                    info.pci_bus,
                    info.pci_device,
                    info.pci_function,
                    info.hci_version >> 8,
                    info.hci_version & 0xFF,
                    info.max_ports,
                    connected_count,
                    dev_count
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
static mut XHCI_DEVICE_BUFFERS: Option<Vec<DmaBuffer>> = None;

/// Tracking state for Event Ring consumption.
struct EventRingState {
    dequeue_idx: usize,
    cycle_state: bool,
}

/// Dequeues the next event from the Event Ring if one is available matching the cycle state,
/// advancing the dequeue pointer and acknowledging the event via ERDP.
fn next_event(
    event_ring: &DmaBuffer,
    intr0: &MmioRegion,
    er_state: &mut EventRingState,
) -> Option<Trb> {
    let e_idx = er_state.dequeue_idx;
    let ev_slice = &event_ring.as_slice()[e_idx * 16..(e_idx + 1) * 16];
    let ev: Trb = unsafe { core::ptr::read_unaligned(ev_slice.as_ptr() as *const Trb) };
    if ev.cycle() != er_state.cycle_state {
        return None;
    }

    // Advance event ring dequeue pointer
    er_state.dequeue_idx = (er_state.dequeue_idx + 1) % 64;
    if er_state.dequeue_idx == 0 {
        er_state.cycle_state = !er_state.cycle_state;
    }
    intr0.write_u64(
        INTR_ERDP,
        (event_ring.phys_addr() + (er_state.dequeue_idx * 16) as u64) | ERDP_EHB,
    );

    Some(ev)
}

/// Helper to send a command TRB on the Command Ring and synchronously await completion on the Event Ring.
fn send_command(
    cmd_ring: &mut DmaBuffer,
    db_regs: &MmioRegion,
    event_ring: &DmaBuffer,
    intr0: &MmioRegion,
    cmd_idx: &mut usize,
    er_state: &mut EventRingState,
    trb: Trb,
) -> Result<Trb, &'static str> {
    let idx = *cmd_idx;
    if idx >= 63 {
        return Err("Command ring full");
    }

    let cmd_phys = cmd_ring.phys_addr() + (idx * 16) as u64;
    let trb_bytes = unsafe {
        core::slice::from_raw_parts(&trb as *const Trb as *const u8, core::mem::size_of::<Trb>())
    };
    let offset = idx * 16;
    cmd_ring.as_mut_slice()[offset..offset + 16].copy_from_slice(trb_bytes);
    *cmd_idx += 1;
    compiler_fence(Ordering::SeqCst);

    // Ring Doorbell 0 for Host Controller Command Ring (Target = 0)
    db_regs.write_u32(0, 0);

    let mut matched_event: Option<Trb> = None;
    let wait_res = time::wait_until(1000, || {
        while let Some(ev) = next_event(event_ring, intr0, er_state) {
            if ev.trb_type() == TRB_TYPE_COMMAND_COMPLETION && ev.cmd_trb_pointer() == cmd_phys {
                matched_event = Some(ev);
                return true;
            } else if ev.trb_type() == TRB_TYPE_PORT_STATUS_CHANGE {
                info!("xHCI: Handled PortStatusChangeEvent on Event Ring.");
            } else {
                info!("xHCI: Handled intermediate event TRB type {} on Event Ring.", ev.trb_type());
            }
        }
        false
    });

    if wait_res.is_err() || matched_event.is_none() {
        return Err("Command timed out waiting for Event Ring completion");
    }

    Ok(matched_event.unwrap())
}

/// Helper to submit a 3-stage Control Transfer on an Endpoint 0 Transfer Ring
/// (SetupStage, optional DataStage, StatusStage) and await completion on the Event Ring.
fn send_control_transfer(
    ep0_ring: &mut DmaBuffer,
    ep0_idx: &mut usize,
    ep0_cycle: &mut bool,
    db_regs: &MmioRegion,
    slot_id: u8,
    event_ring: &DmaBuffer,
    intr0: &MmioRegion,
    er_state: &mut EventRingState,
    setup: UsbSetupPacket,
    data_buffer: &mut DmaBuffer,
    data_len: u16,
) -> Result<usize, &'static str> {
    if *ep0_idx + 3 >= 63 {
        return Err("EP0 transfer ring full");
    }

    let cycle = *ep0_cycle;
    let trt = if data_len > 0 { TRT_IN_DATA } else { TRT_NO_DATA };
    let setup_trb = build_setup_stage_trb(setup, trt, cycle);

    let offset0 = *ep0_idx * 16;
    let setup_bytes = unsafe {
        core::slice::from_raw_parts(&setup_trb as *const Trb as *const u8, 16)
    };
    ep0_ring.as_mut_slice()[offset0..offset0 + 16].copy_from_slice(setup_bytes);
    *ep0_idx += 1;

    if data_len > 0 {
        let data_trb = build_data_stage_trb(data_buffer.phys_addr(), data_len as u32, true, cycle);
        let offset1 = *ep0_idx * 16;
        let data_bytes = unsafe {
            core::slice::from_raw_parts(&data_trb as *const Trb as *const u8, 16)
        };
        ep0_ring.as_mut_slice()[offset1..offset1 + 16].copy_from_slice(data_bytes);
        *ep0_idx += 1;
    }

    // Status Stage: direction OUT for Data IN stage, with IOC=true
    let status_trb = build_status_stage_trb(false, cycle);
    let offset2 = *ep0_idx * 16;
    let status_bytes = unsafe {
        core::slice::from_raw_parts(&status_trb as *const Trb as *const u8, 16)
    };
    ep0_ring.as_mut_slice()[offset2..offset2 + 16].copy_from_slice(status_bytes);
    *ep0_idx += 1;

    compiler_fence(Ordering::SeqCst);

    // Ring Doorbell for slot_id with Target = 1 (EP0 Default Control Pipe)
    db_regs.write_u32((slot_id as usize) * 4, 1);

    let mut matched_event: Option<Trb> = None;
    let wait_res = time::wait_until(1000, || {
        while let Some(ev) = next_event(event_ring, intr0, er_state) {
            if ev.trb_type() == TRB_TYPE_TRANSFER_EVENT && ev.slot_id() == slot_id {
                matched_event = Some(ev);
                return true;
            } else if ev.trb_type() == TRB_TYPE_PORT_STATUS_CHANGE {
                info!("xHCI: Handled PortStatusChangeEvent during transfer.");
            } else {
                info!("xHCI: Handled intermediate event TRB type {} on Event Ring.", ev.trb_type());
            }
        }
        false
    });

    if wait_res.is_err() || matched_event.is_none() {
        return Err("EP0 transfer timed out waiting for Event Ring completion");
    }

    let ev = matched_event.unwrap();
    let code = ev.completion_code();
    if code != TRB_COMP_SUCCESS && code != TRB_COMP_SHORT_PACKET {
        return Err("TransferEvent reported error");
    }

    let residual = (ev.status & 0x00FF_FFFF) as usize;
    let transferred = if (data_len as usize) >= residual {
        (data_len as usize) - residual
    } else {
        data_len as usize
    };

    Ok(transferred)
}

/// Formats the xHCI controller status, discovered ports, and enumerated devices into `/proc/usb`.
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

        let _ = writeln!(out, "\nDISCOVERED USB DEVICES:");
        if info.devices.is_empty() {
            let _ = writeln!(out, "  (No USB devices enumerated)");
        } else {
            let _ = writeln!(out, "SLOT  PORT  SPEED  VID:PID    CLASS:SUB:PR  DETAILS");
            let _ = writeln!(out, "----  ----  -----  -------    ------------  -------");
            for d in &info.devices {
                let details = match d.interface_class {
                    0x03 => {
                        if d.interface_protocol == 1 { "HID Boot Keyboard" }
                        else if d.interface_protocol == 2 { "HID Boot Mouse" }
                        else { "HID Generic" }
                    },
                    0x08 => "Mass Storage",
                    0x09 => "Hub",
                    _ => "Generic",
                };
                let _ = writeln!(
                    out,
                    "{:<4}  {:<4}  {:<5}  {:04x}:{:04x}  {:02x}:{:02x}:{:02x}      {}",
                    d.slot_id, d.port_number, d.speed, d.vendor_id, d.product_id,
                    d.interface_class, d.interface_subclass, d.interface_protocol, details
                );
            }
        }
    }

    out
}

/// Probes PCI devices for an xHCI USB 3.0 controller, initializes hardware,
/// brings up DMA Command/Event rings via Driver Kit, executes a verification No-Op,
/// performs port reset and slot addressing, and enumerates USB device descriptors via EP0.
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

        // Fix HCIVERSION read: Read 32-bit aligned dword at offset 0x00
        let cap_dw0 = mmio.read_u32(0x00);
        let caplength = (cap_dw0 & 0xFF) as usize;
        let hci_version = ((cap_dw0 >> 16) & 0xFFFF) as u16;

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
        let mut er_state = EventRingState {
            dequeue_idx: 0,
            cycle_state: true,
        };
        let mut cmd_idx = 0usize;

        if dboff + 4 > mmio.size() {
            warn!("xHCI: Doorbell register offset exceeds MMIO space.");
            return None;
        }
        let db_regs = mmio.sub_region(dboff, 256 * 4);

        // Command TRB 0: No-Op Command, Cycle = 1
        let noop_trb = build_noop_cmd(true);
        match send_command(
            &mut cmd_ring,
            &db_regs,
            &event_ring,
            &intr0,
            &mut cmd_idx,
            &mut er_state,
            noop_trb,
        ) {
            Ok(ev) => {
                if ev.trb_type() == TRB_TYPE_COMMAND_COMPLETION && ev.completion_code() == TRB_COMP_SUCCESS {
                    dma_test_success = true;
                    info!(
                        "xHCI: Command Ring DMA verified! CommandCompletionEvent received: Code={} (Success), Slot={}.",
                        ev.completion_code(), ev.slot_id()
                    );
                } else {
                    warn!(
                        "xHCI: Unexpected Event TRB type {} code {}",
                        ev.trb_type(), ev.completion_code()
                    );
                }
            }
            Err(e) => {
                warn!("xHCI: Failed during No-Op DMA verification: {}", e);
            }
        }

        // -------------------------------------------------------------------
        // 6. Enumerate Root Hub Ports & USB Device Addressing / Descriptors
        // -------------------------------------------------------------------
        let mut port_infos = Vec::new();
        let mut discovered_devices = Vec::new();
        let mut dev_buffers = Vec::new();
        let csz_64 = (hccparams1 & HCCPARAMS1_CSZ) != 0;

        for port_num in 1..=max_ports {
            let portsc_offset = OP_PORTSC_BASE + ((port_num as usize - 1) * 0x10);
            if portsc_offset + 4 > op_regs.size() {
                continue;
            }

            let mut portsc = op_regs.read_u32(portsc_offset);
            let mut status = parse_portsc(portsc);

            if !status.connected {
                port_infos.push(XhciPortInfo {
                    port_number: port_num,
                    connected: false,
                    enabled: false,
                    speed: status.speed_name(),
                });
                continue;
            }

            info!(
                "xHCI Root Hub Port {}: Connected=true, Enabled={}, Speed={} (PORTSC=0x{:08x})",
                port_num, status.enabled, status.speed_name(), portsc
            );

            // Passo 1 (Port Reset): If PED == 0, reset port using portsc_preserve
            if !status.enabled {
                info!("xHCI: Port {} connected but not enabled. Issuing Port Reset...", port_num);
                let reset_val = portsc_preserve(portsc) | PORTSC_PR;
                op_regs.write_u32(portsc_offset, reset_val);

                // Await Port Reset completion (PR == 0 and PED == 1)
                let reset_res = time::wait_until(500, || {
                    let val = op_regs.read_u32(portsc_offset);
                    (val & PORTSC_PR) == 0 && (val & PORTSC_PED) != 0
                });

                if reset_res.is_err() {
                    warn!("xHCI: Port {} reset timed out or failed to enable.", port_num);
                    port_infos.push(XhciPortInfo {
                        port_number: port_num,
                        connected: true,
                        enabled: false,
                        speed: status.speed_name(),
                    });
                    continue;
                }

                // Clear Port Reset Change (PRC) W1C bit
                let prc_val = op_regs.read_u32(portsc_offset);
                op_regs.write_u32(portsc_offset, portsc_preserve(prc_val) | PORTSC_PRC);

                // Re-read PORTSC to obtain negotiated speed
                portsc = op_regs.read_u32(portsc_offset);
                status = parse_portsc(portsc);
                info!(
                    "xHCI: Port {} enabled after reset: Speed={} (PORTSC=0x{:08x})",
                    port_num, status.speed_name(), portsc
                );
            }

            port_infos.push(XhciPortInfo {
                port_number: port_num,
                connected: status.connected,
                enabled: status.enabled,
                speed: status.speed_name(),
            });

            // Passo 2 (EnableSlotCommand)
            let enable_cmd = build_enable_slot_cmd(true);
            let comp_trb = match send_command(
                &mut cmd_ring,
                &db_regs,
                &event_ring,
                &intr0,
                &mut cmd_idx,
                &mut er_state,
                enable_cmd,
            ) {
                Ok(trb) => trb,
                Err(e) => {
                    warn!("xHCI: Port {} EnableSlotCommand failed: {}", port_num, e);
                    continue;
                }
            };

            if comp_trb.completion_code() != TRB_COMP_SUCCESS {
                warn!("xHCI: Port {} EnableSlotCommand error: Code={}", port_num, comp_trb.completion_code());
                continue;
            }

            let slot_id = comp_trb.slot_id();
            info!("xHCI: Slot {} allocated for Port {}.", slot_id, port_num);

            // Passo 3: Output Device Context + EP0 Ring + Input Context + AddressDeviceCommand
            let mut output_ctx = match DmaBuffer::allocate_4k() {
                Some(b) => b,
                None => {
                    warn!("xHCI: Failed to allocate DMA buffer for Output Device Context.");
                    continue;
                }
            };
            output_ctx.zero();

            // Record Output Device Context in DCBAA[slot_id]
            let dcbaa_slot_offset = (slot_id as usize) * 8;
            let out_phys = output_ctx.phys_addr();
            dcbaa.as_mut_slice()[dcbaa_slot_offset..dcbaa_slot_offset + 8]
                .copy_from_slice(&out_phys.to_le_bytes());

            // Allocate EP0 Transfer Ring
            let mut ep0_ring = match DmaBuffer::allocate_4k() {
                Some(b) => b,
                None => {
                    warn!("xHCI: Failed to allocate DMA buffer for EP0 Ring.");
                    continue;
                }
            };
            ep0_ring.zero();

            // Write Link TRB at index 63
            let ep0_link_trb = build_link_trb(ep0_ring.phys_addr(), true, true);
            let ep0_link_bytes = core::slice::from_raw_parts(
                &ep0_link_trb as *const Trb as *const u8,
                core::mem::size_of::<Trb>(),
            );
            let ep0_link_offset = 63 * 16;
            ep0_ring.as_mut_slice()[ep0_link_offset..ep0_link_offset + 16].copy_from_slice(ep0_link_bytes);

            // Determine max packet size
            let max_packet_size: u16 = match status.speed {
                PortSpeed::LowSpeed => 8,
                PortSpeed::FullSpeed => 64,
                PortSpeed::HighSpeed => 64,
                PortSpeed::SuperSpeed | PortSpeed::SuperSpeedPlus => 512,
                PortSpeed::Undefined => 64,
            };

            // Allocate and format Input Context
            let mut input_ctx = match DmaBuffer::allocate_4k() {
                Some(b) => b,
                None => {
                    warn!("xHCI: Failed to allocate DMA buffer for Input Context.");
                    continue;
                }
            };
            input_ctx.zero();

            format_input_context(
                input_ctx.as_mut_slice(),
                csz_64,
                port_num,
                status.speed,
                max_packet_size,
                ep0_ring.phys_addr(),
            );

            // Send Address Device Command
            let addr_cmd = build_address_device_cmd(input_ctx.phys_addr(), slot_id, false, true);
            let addr_res = match send_command(
                &mut cmd_ring,
                &db_regs,
                &event_ring,
                &intr0,
                &mut cmd_idx,
                &mut er_state,
                addr_cmd,
            ) {
                Ok(trb) => trb,
                Err(e) => {
                    warn!("xHCI: AddressDeviceCommand for Slot {} failed: {}", slot_id, e);
                    continue;
                }
            };

            if addr_res.completion_code() != TRB_COMP_SUCCESS {
                warn!("xHCI: AddressDeviceCommand error: Code={}", addr_res.completion_code());
                continue;
            }
            info!("xHCI: Device on Slot {} addressed successfully.", slot_id);

            // Passo 4: Read USB Descriptors via EP0 Transfer Ring
            let mut data_buf = match DmaBuffer::allocate_4k() {
                Some(b) => b,
                None => {
                    warn!("xHCI: Failed to allocate DMA buffer for Control Transfer data.");
                    continue;
                }
            };
            data_buf.zero();

            let mut ep0_idx = 0usize;
            let mut ep0_cycle = true;

            // 4a: GET_DESCRIPTOR (Device, 18 bytes)
            let dev_setup = UsbSetupPacket::get_descriptor(USB_DESC_DEVICE, 0, 18);
            let dev_res = send_control_transfer(
                &mut ep0_ring,
                &mut ep0_idx,
                &mut ep0_cycle,
                &db_regs,
                slot_id,
                &event_ring,
                &intr0,
                &mut er_state,
                dev_setup,
                &mut data_buf,
                18,
            );

            if let Err(e) = dev_res {
                warn!("xHCI: Slot {} GET_DESCRIPTOR (Device) failed: {}", slot_id, e);
                continue;
            }

            let dev_desc = match UsbDeviceDescriptor::parse(&data_buf.as_slice()[0..18]) {
                Some(d) => d,
                None => {
                    warn!("xHCI: Slot {} failed to parse UsbDeviceDescriptor.", slot_id);
                    continue;
                }
            };

            info!(
                "xHCI Slot {}: Device Descriptor: VID:PID={:04x}:{:04x}, USB {:x}.{:02x}, Class=0x{:02x}, MaxPacketSize={}",
                slot_id,
                dev_desc.id_vendor,
                dev_desc.id_product,
                dev_desc.bcd_usb >> 8,
                dev_desc.bcd_usb & 0xFF,
                dev_desc.b_device_class,
                dev_desc.b_max_packet_size0
            );

            // 4b: GET_DESCRIPTOR (Configuration, 32 bytes)
            data_buf.zero();
            let cfg_setup = UsbSetupPacket::get_descriptor(USB_DESC_CONFIGURATION, 0, 32);
            let cfg_res = send_control_transfer(
                &mut ep0_ring,
                &mut ep0_idx,
                &mut ep0_cycle,
                &db_regs,
                slot_id,
                &event_ring,
                &intr0,
                &mut er_state,
                cfg_setup,
                &mut data_buf,
                32,
            );

            if let Err(e) = cfg_res {
                warn!("xHCI: Slot {} GET_DESCRIPTOR (Configuration) failed: {}", slot_id, e);
                continue;
            }

            let cfg_val = if data_buf.as_slice().len() >= 9 && data_buf.as_slice()[1] == USB_DESC_CONFIGURATION {
                data_buf.as_slice()[5]
            } else {
                1
            };

            let (if_class, if_sub, if_proto) = scan_interfaces(&data_buf.as_slice()[0..32]).unwrap_or((0, 0, 0));
            let class_name = match if_class {
                0x03 => {
                    if if_proto == 1 { "HID Boot Keyboard" }
                    else if if_proto == 2 { "HID Boot Mouse" }
                    else { "HID Generic" }
                },
                0x08 => "Mass Storage",
                0x09 => "Hub",
                _ => "Generic",
            };

            info!(
                "xHCI Slot {}: Configuration 0x{:02x}, Interface 0: Class=0x{:02x} ({}), Subclass=0x{:02x}, Protocol=0x{:02x}",
                slot_id, cfg_val, if_class, class_name, if_sub, if_proto
            );
            info!(
                "xHCI Slot {}: Discovered USB Device [{:04x}:{:04x}] {}",
                slot_id, dev_desc.id_vendor, dev_desc.id_product, class_name
            );

            discovered_devices.push(XhciUsbDevice {
                slot_id,
                port_number: port_num,
                speed: status.speed_name(),
                vendor_id: dev_desc.id_vendor,
                product_id: dev_desc.id_product,
                device_class: dev_desc.b_device_class,
                device_subclass: dev_desc.b_device_sub_class,
                device_protocol: dev_desc.b_device_protocol,
                interface_class: if_class,
                interface_subclass: if_sub,
                interface_protocol: if_proto,
                configuration_val: cfg_val,
                max_packet_size: dev_desc.b_max_packet_size0 as u16,
            });

            // Persist device buffers so DMA structures remain valid
            dev_buffers.push(output_ctx);
            dev_buffers.push(ep0_ring);
            dev_buffers.push(input_ctx);
            dev_buffers.push(data_buf);
        }

        XHCI_DEVICE_BUFFERS = Some(dev_buffers);

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
            devices: discovered_devices,
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
