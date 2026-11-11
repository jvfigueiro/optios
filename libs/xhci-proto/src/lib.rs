// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: USB 3.0 eXtensible Host Controller Interface (xHCI 1.2) Protocol Definitions
// Date: 2026-11-10

//! Pure, hardware-independent `#![no_std]` crate implementing data structures,
//! register definitions, TRB formats, and helpers for xHCI 1.0/1.1/1.2 Host Controllers.

#![no_std]

// ---------------------------------------------------------------------------
// Capability Register Offsets (relative to MMIO BAR0 base)
// ---------------------------------------------------------------------------
pub const CAP_CAPLENGTH: usize = 0x00;
pub const CAP_HCIVERSION: usize = 0x02;
pub const CAP_HCSPARAMS1: usize = 0x04;
pub const CAP_HCSPARAMS2: usize = 0x08;
pub const CAP_HCSPARAMS3: usize = 0x0C;
pub const CAP_HCCPARAMS1: usize = 0x10;
pub const CAP_DBOFF: usize = 0x14;
pub const CAP_RTSOFF: usize = 0x18;

pub const HCCPARAMS1_CSZ: u32 = 1 << 2; // Context Size (0 = 32 bytes, 1 = 64 bytes)

// ---------------------------------------------------------------------------
// Operational Register Offsets (relative to base + CAPLENGTH)
// ---------------------------------------------------------------------------
pub const OP_USBCMD: usize = 0x00;
pub const OP_USBSTS: usize = 0x04;
pub const OP_PAGESIZE: usize = 0x08;
pub const OP_DNCTRL: usize = 0x14;
pub const OP_CRCR: usize = 0x18;
pub const OP_DCBAAP: usize = 0x30;
pub const OP_CONFIG: usize = 0x38;
pub const OP_PORTSC_BASE: usize = 0x400;

// ---------------------------------------------------------------------------
// Register Bitmasks & Flags
// ---------------------------------------------------------------------------
// USBCMD
pub const USBCMD_RS: u32 = 1 << 0;     // Run/Stop
pub const USBCMD_HCRST: u32 = 1 << 1;  // Host Controller Reset
pub const USBCMD_INTE: u32 = 1 << 2;   // Interrupter Enable
pub const USBCMD_HSEE: u32 = 1 << 3;   // Host System Error Enable

// USBSTS
pub const USBSTS_HCH: u32 = 1 << 0;    // Host Controller Halted
pub const USBSTS_HSE: u32 = 1 << 2;    // Host System Error
pub const USBSTS_EINT: u32 = 1 << 3;   // Event Interrupt
pub const USBSTS_PCD: u32 = 1 << 4;    // Port Change Detect
pub const USBSTS_CNR: u32 = 1 << 11;   // Controller Not Ready

// CRCR
pub const CRCR_RCS: u64 = 1 << 0;      // Ring Cycle State

// PORTSC
pub const PORTSC_CCS: u32 = 1 << 0;    // Current Connect Status
pub const PORTSC_PED: u32 = 1 << 1;    // Port Enabled/Disabled
pub const PORTSC_OCA: u32 = 1 << 3;    // Over-current Active
pub const PORTSC_PR: u32 = 1 << 4;     // Port Reset
pub const PORTSC_PLS_MASK: u32 = 0x0F << 5;
pub const PORTSC_PP: u32 = 1 << 9;     // Port Power
pub const PORTSC_SPEED_MASK: u32 = 0x0F << 10;
pub const PORTSC_SPEED_SHIFT: u32 = 10;

// PORTSC Write-1-to-Clear (W1C) Change Bits: bits 17..=23
pub const PORTSC_CSC: u32 = 1 << 17;   // Connect Status Change
pub const PORTSC_PEC: u32 = 1 << 18;   // Port Enable/Disable Change
pub const PORTSC_WRC: u32 = 1 << 19;   // Warm Port Reset Change
pub const PORTSC_OCC: u32 = 1 << 20;   // Over-current Change
pub const PORTSC_PRC: u32 = 1 << 21;   // Port Reset Change
pub const PORTSC_PLC: u32 = 1 << 22;   // Port Link State Change
pub const PORTSC_CEC: u32 = 1 << 23;   // Config Error Change

pub const PORTSC_W1C_MASK: u32 = PORTSC_CSC
    | PORTSC_PEC
    | PORTSC_WRC
    | PORTSC_OCC
    | PORTSC_PRC
    | PORTSC_PLC
    | PORTSC_CEC;

/// Preserves the PORTSC value for Read-Modify-Write by zeroing all W1C bits.
#[inline]
pub const fn portsc_preserve(raw: u32) -> u32 {
    raw & !PORTSC_W1C_MASK
}

// Interrupter 0 Registers (relative to base + RTSOFF + 0x20)
pub const INTR_IMAN: usize = 0x00;
pub const INTR_IMOD: usize = 0x04;
pub const INTR_ERSTSZ: usize = 0x08;
pub const INTR_ERSTBA: usize = 0x10;
pub const INTR_ERDP: usize = 0x18;

pub const IMAN_IP: u32 = 1 << 0;       // Interrupt Pending
pub const IMAN_IE: u32 = 1 << 1;       // Interrupt Enable
pub const ERDP_EHB: u64 = 1 << 3;      // Event Handler Busy

// Extended Capabilities
pub const EXT_CAP_USB_LEGACY: u8 = 1;
pub const USBLEGSUP_BIOS_OWNED: u32 = 1 << 16;
pub const USBLEGSUP_OS_OWNED: u32 = 1 << 24;

// ---------------------------------------------------------------------------
// TRB Types (xHCI 1.2 Table 6-91)
// ---------------------------------------------------------------------------
pub const TRB_TYPE_NORMAL: u8 = 1;
pub const TRB_TYPE_SETUP_STAGE: u8 = 2;
pub const TRB_TYPE_DATA_STAGE: u8 = 3;
pub const TRB_TYPE_STATUS_STAGE: u8 = 4;
pub const TRB_TYPE_LINK: u8 = 6;
pub const TRB_TYPE_ENABLE_SLOT: u8 = 9;
pub const TRB_TYPE_DISABLE_SLOT: u8 = 10;
pub const TRB_TYPE_ADDRESS_DEVICE: u8 = 11;
pub const TRB_TYPE_CONFIGURE_ENDPOINT: u8 = 12;
pub const TRB_TYPE_NOOP_CMD: u8 = 23;
pub const TRB_TYPE_TRANSFER_EVENT: u8 = 32;
pub const TRB_TYPE_COMMAND_COMPLETION: u8 = 33;
pub const TRB_TYPE_PORT_STATUS_CHANGE: u8 = 34;

// TRB Completion Codes
pub const TRB_COMP_INVALID: u8 = 0;
pub const TRB_COMP_SUCCESS: u8 = 1;
pub const TRB_COMP_DATA_BUFFER_ERROR: u8 = 2;
pub const TRB_COMP_BABBLE_DETECTED: u8 = 3;
pub const TRB_COMP_USB_TRANSACTION_ERROR: u8 = 4;
pub const TRB_COMP_TRB_ERROR: u8 = 5;
pub const TRB_COMP_STALL_ERROR: u8 = 6;
pub const TRB_COMP_RESOURCE_ERROR: u8 = 7;
pub const TRB_COMP_BANDWIDTH_ERROR: u8 = 8;
pub const TRB_COMP_NO_SLOTS_AVAILABLE: u8 = 9;
pub const TRB_COMP_SHORT_PACKET: u8 = 13;

// TRB Control bit flags
pub const TRB_CYCLE: u32 = 1 << 0;
pub const TRB_TC: u32 = 1 << 1;        // Toggle Cycle (for Link TRB)
pub const TRB_CH: u32 = 1 << 4;        // Chain
pub const TRB_IOC: u32 = 1 << 5;       // Interrupt On Completion
pub const TRB_IDT: u32 = 1 << 6;       // Immediate Data Transfer
pub const TRB_DIR_IN: u32 = 1 << 16;   // Direction IN

// ---------------------------------------------------------------------------
// Transfer Request Block (TRB) — 16 bytes, 16-byte aligned
// ---------------------------------------------------------------------------
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Trb {
    pub param: u64,
    pub status: u32,
    pub control: u32,
}

impl Trb {
    /// Constructs a raw TRB.
    pub const fn new(param: u64, status: u32, control: u32) -> Self {
        Self { param, status, control }
    }

    /// Creates an empty (zeroed) TRB.
    pub const fn zero() -> Self {
        Self { param: 0, status: 0, control: 0 }
    }

    /// Returns the TRB type ID (bits 10..15 of control).
    #[inline]
    pub fn trb_type(&self) -> u8 {
        ((self.control >> 10) & 0x3F) as u8
    }

    /// Returns the cycle bit (bit 0 of control).
    #[inline]
    pub fn cycle(&self) -> bool {
        (self.control & TRB_CYCLE) != 0
    }

    /// Returns the completion code for an event TRB (bits 24..31 of status).
    #[inline]
    pub fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xFF) as u8
    }

    /// Returns the slot ID (bits 24..31 of control in command completion events).
    #[inline]
    pub fn slot_id(&self) -> u8 {
        ((self.control >> 24) & 0xFF) as u8
    }

    /// Returns the pointer parameter (e.g. command TRB physical address).
    #[inline]
    pub fn cmd_trb_pointer(&self) -> u64 {
        self.param
    }
}

/// Builds a No-Op Command TRB (Type 23).
pub fn build_noop_cmd(cycle: bool) -> Trb {
    let mut control = (TRB_TYPE_NOOP_CMD as u32) << 10;
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(0, 0, control)
}

/// Builds an Enable Slot Command TRB (Type 9).
pub fn build_enable_slot_cmd(cycle: bool) -> Trb {
    let mut control = (TRB_TYPE_ENABLE_SLOT as u32) << 10;
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(0, 0, control)
}

/// Builds an Address Device Command TRB (Type 11).
pub fn build_address_device_cmd(input_ctx_phys: u64, slot_id: u8, bsr: bool, cycle: bool) -> Trb {
    let mut control = ((TRB_TYPE_ADDRESS_DEVICE as u32) << 10) | ((slot_id as u32) << 24);
    if bsr {
        control |= 1 << 9; // Block Set Address Request
    }
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(input_ctx_phys, 0, control)
}

/// Builds a Link TRB (Type 6) pointing back to the beginning of the ring.
pub fn build_link_trb(ring_start_phys: u64, toggle_cycle: bool, cycle: bool) -> Trb {
    let mut control = (TRB_TYPE_LINK as u32) << 10;
    if toggle_cycle {
        control |= TRB_TC;
    }
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(ring_start_phys, 0, control)
}

/// Transfer Route Type (TRT) for SetupStage TRB
pub const TRT_NO_DATA: u32 = 0;
pub const TRT_OUT_DATA: u32 = 2;
pub const TRT_IN_DATA: u32 = 3;

/// Builds a SetupStage TRB (Type 2) holding an 8-byte immediate Setup Packet.
pub fn build_setup_stage_trb(packet: UsbSetupPacket, trt: u32, cycle: bool) -> Trb {
    let param = packet.as_u64();
    let status = 8; // Transfer length is always 8 bytes for a USB Setup Packet
    let mut control = ((TRB_TYPE_SETUP_STAGE as u32) << 10) | TRB_IDT | ((trt & 0x03) << 16);
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(param, status, control)
}

/// Builds a DataStage TRB (Type 3).
pub fn build_data_stage_trb(buf_phys: u64, length: u32, dir_in: bool, cycle: bool) -> Trb {
    let mut control = (TRB_TYPE_DATA_STAGE as u32) << 10;
    if dir_in {
        control |= TRB_DIR_IN;
    }
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(buf_phys, length, control)
}

/// Builds a StatusStage TRB (Type 4).
pub fn build_status_stage_trb(dir_in: bool, cycle: bool) -> Trb {
    let mut control = ((TRB_TYPE_STATUS_STAGE as u32) << 10) | TRB_IOC;
    if dir_in {
        control |= TRB_DIR_IN;
    }
    if cycle {
        control |= TRB_CYCLE;
    }
    Trb::new(0, 0, control)
}

// ---------------------------------------------------------------------------
// Event Ring Segment Table (ERST) Entry — 16 bytes
// ---------------------------------------------------------------------------
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ErstEntry {
    pub ring_segment_base_address: u64,
    pub ring_segment_size: u32,
    pub reserved: u32,
}

impl ErstEntry {
    pub const fn new(ring_segment_base_address: u64, ring_segment_size: u32) -> Self {
        Self {
            ring_segment_base_address,
            ring_segment_size,
            reserved: 0,
        }
    }
}

// ---------------------------------------------------------------------------
// xHCI Context Structures & Formatter
// ---------------------------------------------------------------------------

/// Sets up an xHCI Input Context buffer for Slot Context and EP0 Default Control Endpoint.
/// Handles both 32-byte (CSZ=0) and 64-byte (CSZ=1) context structures.
pub fn format_input_context(
    buf: &mut [u8],
    csz_64: bool,
    port_num: u8,
    speed: PortSpeed,
    max_packet_size: u16,
    ep0_ring_phys: u64,
) {
    let ctx_size = if csz_64 { 64 } else { 32 };
    assert!(buf.len() >= ctx_size * 3, "Input Context buffer too small");

    // Clear entire structure
    for b in buf.iter_mut() {
        *b = 0;
    }

    // 1. Input Control Context (at offset 0): Add Context flags = bit 0 (Slot) | bit 1 (EP0) = 0x03
    let add_flags = 0x03u32;
    buf[4..8].copy_from_slice(&add_flags.to_le_bytes());

    // 2. Slot Context (at offset ctx_size * 1)
    let slot_off = ctx_size;
    let speed_val = match speed {
        PortSpeed::FullSpeed => 1,
        PortSpeed::LowSpeed => 2,
        PortSpeed::HighSpeed => 3,
        PortSpeed::SuperSpeed => 4,
        PortSpeed::SuperSpeedPlus => 5,
        PortSpeed::Undefined => 0,
    };

    // Slot Context DW0: Speed in bits 20..23, Context Entries = 1 in bits 27..31
    let slot_dw0: u32 = ((speed_val & 0x0F) << 20) | (1 << 27);
    buf[slot_off..slot_off + 4].copy_from_slice(&slot_dw0.to_le_bytes());

    // Slot Context DW1: Root Hub Port Number in bits 16..23
    let slot_dw1: u32 = (port_num as u32) << 16;
    buf[slot_off + 4..slot_off + 8].copy_from_slice(&slot_dw1.to_le_bytes());

    // 3. Endpoint 0 Context (at offset ctx_size * 2)
    let ep0_off = ctx_size * 2;
    // EP0 DW1: Error Count = 3 (bits 1..2), EP Type = 4 Control Bidirectional (bits 3..5), Max Packet Size (bits 16..31)
    let ep0_dw1: u32 = (3 << 1) | (4 << 3) | ((max_packet_size as u32) << 16);
    buf[ep0_off + 4..ep0_off + 8].copy_from_slice(&ep0_dw1.to_le_bytes());

    // EP0 DW2 & DW3: TR Dequeue Pointer with Dequeue Cycle State (DCS) = 1 in bit 0
    let deq_lo: u32 = (ep0_ring_phys as u32) | 1;
    let deq_hi: u32 = (ep0_ring_phys >> 32) as u32;
    buf[ep0_off + 8..ep0_off + 12].copy_from_slice(&deq_lo.to_le_bytes());
    buf[ep0_off + 12..ep0_off + 16].copy_from_slice(&deq_hi.to_le_bytes());

    // EP0 DW4: Average TRB Length = 8 bytes
    let ep0_dw4: u32 = 8;
    buf[ep0_off + 16..ep0_off + 20].copy_from_slice(&ep0_dw4.to_le_bytes());
}

// ---------------------------------------------------------------------------
// USB Standard Protocol Requests and Descriptors
// ---------------------------------------------------------------------------
pub const USB_REQ_GET_STATUS: u8 = 0x00;
pub const USB_REQ_CLEAR_FEATURE: u8 = 0x01;
pub const USB_REQ_SET_FEATURE: u8 = 0x03;
pub const USB_REQ_SET_ADDRESS: u8 = 0x05;
pub const USB_REQ_GET_DESCRIPTOR: u8 = 0x06;
pub const USB_REQ_SET_DESCRIPTOR: u8 = 0x07;
pub const USB_REQ_GET_CONFIGURATION: u8 = 0x08;
pub const USB_REQ_SET_CONFIGURATION: u8 = 0x09;
pub const USB_REQ_GET_INTERFACE: u8 = 0x0A;
pub const USB_REQ_SET_INTERFACE: u8 = 0x0B;

pub const USB_DESC_DEVICE: u8 = 1;
pub const USB_DESC_CONFIGURATION: u8 = 2;
pub const USB_DESC_STRING: u8 = 3;
pub const USB_DESC_INTERFACE: u8 = 4;
pub const USB_DESC_ENDPOINT: u8 = 5;
pub const USB_DESC_HID: u8 = 0x21;

/// Standard 8-byte USB Setup Packet (USB 2.0 Spec 9.3).
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbSetupPacket {
    pub bm_request_type: u8,
    pub b_request: u8,
    pub w_value: u16,
    pub w_index: u16,
    pub w_length: u16,
}

impl UsbSetupPacket {
    pub const fn new(bm_request_type: u8, b_request: u8, w_value: u16, w_index: u16, w_length: u16) -> Self {
        Self {
            bm_request_type,
            b_request,
            w_value,
            w_index,
            w_length,
        }
    }

    /// Builds a standard `GET_DESCRIPTOR` request packet.
    pub const fn get_descriptor(desc_type: u8, desc_index: u8, length: u16) -> Self {
        Self::new(
            0x80, // Device-to-Host, Standard, Device
            USB_REQ_GET_DESCRIPTOR,
            ((desc_type as u16) << 8) | (desc_index as u16),
            0,
            length,
        )
    }

    /// Builds a standard `SET_CONFIGURATION` request packet.
    pub const fn set_configuration(config_val: u8) -> Self {
        Self::new(
            0x00, // Host-to-Device, Standard, Device
            USB_REQ_SET_CONFIGURATION,
            config_val as u16,
            0,
            0,
        )
    }

    /// Converts this 8-byte setup packet into a 64-bit unsigned integer for TRB param.
    pub fn as_u64(&self) -> u64 {
        let b0 = self.bm_request_type as u64;
        let b1 = (self.b_request as u64) << 8;
        let b2 = (self.w_value as u64) << 16;
        let b3 = (self.w_index as u64) << 32;
        let b4 = (self.w_length as u64) << 48;
        b0 | b1 | b2 | b3 | b4
    }
}

/// Standard 18-byte USB Device Descriptor (USB 2.0 Spec 9.6.1).
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbDeviceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub bcd_usb: u16,
    pub b_device_class: u8,
    pub b_device_sub_class: u8,
    pub b_device_protocol: u8,
    pub b_max_packet_size0: u8,
    pub id_vendor: u16,
    pub id_product: u16,
    pub bcd_device: u16,
    pub i_manufacturer: u8,
    pub i_product: u8,
    pub i_serial_number: u8,
    pub b_num_configurations: u8,
}

impl UsbDeviceDescriptor {
    /// Parses an 18-byte slice into a valid `UsbDeviceDescriptor`.
    pub fn parse(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 18 {
            return None;
        }
        if bytes[0] != 18 || bytes[1] != USB_DESC_DEVICE {
            return None;
        }

        let bcd_usb = u16::from_le_bytes([bytes[2], bytes[3]]);
        let id_vendor = u16::from_le_bytes([bytes[8], bytes[9]]);
        let id_product = u16::from_le_bytes([bytes[10], bytes[11]]);
        let bcd_device = u16::from_le_bytes([bytes[12], bytes[13]]);

        Some(Self {
            b_length: bytes[0],
            b_descriptor_type: bytes[1],
            bcd_usb,
            b_device_class: bytes[4],
            b_device_sub_class: bytes[5],
            b_device_protocol: bytes[6],
            b_max_packet_size0: bytes[7],
            id_vendor,
            id_product,
            bcd_device,
            i_manufacturer: bytes[14],
            i_product: bytes[15],
            i_serial_number: bytes[16],
            b_num_configurations: bytes[17],
        })
    }
}

/// Standard 9-byte USB Configuration Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbConfigurationDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub w_total_length: u16,
    pub b_num_interfaces: u8,
    pub b_configuration_value: u8,
    pub i_configuration: u8,
    pub bm_attributes: u8,
    pub b_max_power: u8,
}

/// Standard 9-byte USB Interface Descriptor.
#[repr(C, packed)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UsbInterfaceDescriptor {
    pub b_length: u8,
    pub b_descriptor_type: u8,
    pub b_interface_number: u8,
    pub b_alternate_setting: u8,
    pub b_num_endpoints: u8,
    pub b_interface_class: u8,
    pub b_interface_sub_class: u8,
    pub b_interface_protocol: u8,
    pub i_interface: u8,
}

/// Scans raw configuration buffer bytes and extracts interface class, subclass, and protocol.
pub fn scan_interfaces(bytes: &[u8]) -> Option<(u8, u8, u8)> {
    if bytes.len() < 9 {
        return None;
    }
    let mut offset = 0;
    while offset + 2 <= bytes.len() {
        let len = bytes[offset] as usize;
        let desc_type = bytes[offset + 1];

        if len == 0 || offset + len > bytes.len() {
            break;
        }

        if desc_type == USB_DESC_INTERFACE && len >= 9 {
            let class = bytes[offset + 5];
            let sub_class = bytes[offset + 6];
            let proto = bytes[offset + 7];
            return Some((class, sub_class, proto));
        }

        offset += len;
    }
    None
}

// ---------------------------------------------------------------------------
// Capability and Port Parsing Helpers
// ---------------------------------------------------------------------------

/// Parses `HCSPARAMS1` into `(max_slots, max_intrs, max_ports)`.
pub fn parse_hcsparams1(val: u32) -> (u8, u16, u8) {
    let max_slots = (val & 0xFF) as u8;
    let max_intrs = ((val >> 8) & 0x7FF) as u16;
    let max_ports = ((val >> 24) & 0xFF) as u8;
    (max_slots, max_intrs, max_ports)
}

/// Port speed identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortSpeed {
    Undefined,
    FullSpeed,      // 12 Mbps (USB 1.1/2.0)
    LowSpeed,       // 1.5 Mbps (USB 1.0/1.1)
    HighSpeed,      // 480 Mbps (USB 2.0)
    SuperSpeed,     // 5 Gbps (USB 3.0)
    SuperSpeedPlus, // 10 Gbps (USB 3.1)
}

impl PortSpeed {
    pub const fn from_raw(speed: u32) -> Self {
        match speed {
            1 => Self::FullSpeed,
            2 => Self::LowSpeed,
            3 => Self::HighSpeed,
            4 => Self::SuperSpeed,
            5 => Self::SuperSpeedPlus,
            _ => Self::Undefined,
        }
    }

    pub const fn name(&self) -> &'static str {
        match self {
            Self::Undefined => "Unknown",
            Self::FullSpeed => "Full-Speed (12 Mbps)",
            Self::LowSpeed => "Low-Speed (1.5 Mbps)",
            Self::HighSpeed => "High-Speed (480 Mbps)",
            Self::SuperSpeed => "SuperSpeed (5 Gbps)",
            Self::SuperSpeedPlus => "SuperSpeedPlus (10 Gbps)",
        }
    }
}

/// Decoded status of an xHCI Root Hub port.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PortStatus {
    pub connected: bool,
    pub enabled: bool,
    pub powered: bool,
    pub speed: PortSpeed,
}

impl PortStatus {
    pub fn speed_name(&self) -> &'static str {
        self.speed.name()
    }
}

/// Parses the 32-bit `PORTSC` register.
pub fn parse_portsc(val: u32) -> PortStatus {
    let connected = (val & PORTSC_CCS) != 0;
    let enabled = (val & PORTSC_PED) != 0;
    let powered = (val & PORTSC_PP) != 0;
    let raw_speed = (val & PORTSC_SPEED_MASK) >> PORTSC_SPEED_SHIFT;

    PortStatus {
        connected,
        enabled,
        powered,
        speed: PortSpeed::from_raw(raw_speed),
    }
}

// ---------------------------------------------------------------------------
// Unit Tests (Executed on Host via ./scripts/test.sh)
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trb_layout_and_size() {
        assert_eq!(core::mem::size_of::<Trb>(), 16);
        assert_eq!(core::mem::align_of::<Trb>(), 16);
    }

    #[test]
    fn test_erst_entry_layout_and_size() {
        assert_eq!(core::mem::size_of::<ErstEntry>(), 16);
        assert_eq!(core::mem::align_of::<ErstEntry>(), 16);
    }

    #[test]
    fn test_portsc_w1c_preservation() {
        // Raw PORTSC with connect status, port enabled, and several W1C change flags set
        let raw = PORTSC_CCS | PORTSC_PED | PORTSC_PP | PORTSC_CSC | PORTSC_PRC;
        let preserved = portsc_preserve(raw);

        // W1C change bits must be stripped
        assert_eq!(preserved & PORTSC_CSC, 0);
        assert_eq!(preserved & PORTSC_PRC, 0);
        assert_eq!(preserved & PORTSC_W1C_MASK, 0);

        // Core operational bits must be preserved
        assert_ne!(preserved & PORTSC_CCS, 0);
        assert_ne!(preserved & PORTSC_PED, 0);
        assert_ne!(preserved & PORTSC_PP, 0);
    }

    #[test]
    fn test_noop_cmd_builder() {
        let trb_c0 = build_noop_cmd(false);
        assert_eq!(trb_c0.trb_type(), TRB_TYPE_NOOP_CMD);
        assert!(!trb_c0.cycle());

        let trb_c1 = build_noop_cmd(true);
        assert_eq!(trb_c1.trb_type(), TRB_TYPE_NOOP_CMD);
        assert!(trb_c1.cycle());
    }

    #[test]
    fn test_link_trb_builder_and_toggle_cycle() {
        let link = build_link_trb(0x1000_2000, true, true);
        assert_eq!(link.param, 0x1000_2000);
        assert_eq!(link.trb_type(), TRB_TYPE_LINK);
        assert!(link.cycle());
        assert_ne!(link.control & TRB_TC, 0);
    }

    #[test]
    fn test_command_completion_event_parsing() {
        let trb = Trb::new(
            0x0000_0000_1234_5000,
            (TRB_COMP_SUCCESS as u32) << 24,
            ((TRB_TYPE_COMMAND_COMPLETION as u32) << 10) | (5 << 24) | TRB_CYCLE,
        );
        assert_eq!(trb.trb_type(), TRB_TYPE_COMMAND_COMPLETION);
        assert_eq!(trb.cmd_trb_pointer(), 0x1234_5000);
        assert_eq!(trb.completion_code(), TRB_COMP_SUCCESS);
        assert_eq!(trb.slot_id(), 5);
        assert!(trb.cycle());
    }

    #[test]
    fn test_parse_hcsparams1() {
        let val: u32 = (4 << 24) | (8 << 8) | 64;
        let (slots, intrs, ports) = parse_hcsparams1(val);
        assert_eq!(slots, 64);
        assert_eq!(intrs, 8);
        assert_eq!(ports, 4);
    }

    #[test]
    fn test_parse_portsc() {
        let val: u32 = PORTSC_CCS | PORTSC_PED | PORTSC_PP | (4 << PORTSC_SPEED_SHIFT);
        let status = parse_portsc(val);
        assert!(status.connected);
        assert!(status.enabled);
        assert!(status.powered);
        assert_eq!(status.speed, PortSpeed::SuperSpeed);
        assert_eq!(status.speed_name(), "SuperSpeed (5 Gbps)");
    }

    #[test]
    fn test_control_transfer_trb_builders() {
        let setup_pkt = UsbSetupPacket::get_descriptor(USB_DESC_DEVICE, 0, 18);
        let setup_trb = build_setup_stage_trb(setup_pkt, TRT_IN_DATA, true);
        assert_eq!(setup_trb.trb_type(), TRB_TYPE_SETUP_STAGE);
        assert!(setup_trb.cycle());
        assert_eq!(setup_trb.status, 8); // Setup packet length

        let data_trb = build_data_stage_trb(0x1000_5000, 18, true, true);
        assert_eq!(data_trb.trb_type(), TRB_TYPE_DATA_STAGE);
        assert_eq!(data_trb.param, 0x1000_5000);
        assert_eq!(data_trb.status, 18);
        assert!(data_trb.cycle());
        assert_ne!(data_trb.control & TRB_DIR_IN, 0);

        let status_trb = build_status_stage_trb(false, true);
        assert_eq!(status_trb.trb_type(), TRB_TYPE_STATUS_STAGE);
        assert!(status_trb.cycle());
        assert_ne!(status_trb.control & TRB_IOC, 0);

        let addr_trb = build_address_device_cmd(0x2000_0000, 3, false, true);
        assert_eq!(addr_trb.trb_type(), TRB_TYPE_ADDRESS_DEVICE);
        assert_eq!(addr_trb.slot_id(), 3);
        assert_eq!(addr_trb.param, 0x2000_0000);
    }

    #[test]
    fn test_input_context_formatting_32_vs_64() {
        // Test 32-byte context (CSZ = 0)
        let mut buf32 = [0u8; 128];
        format_input_context(&mut buf32, false, 5, PortSpeed::HighSpeed, 64, 0x5000_0000);

        let add_flags = u32::from_le_bytes([buf32[4], buf32[5], buf32[6], buf32[7]]);
        assert_eq!(add_flags, 0x03);

        // Slot Context at offset 32
        let slot_dw0 = u32::from_le_bytes([buf32[32], buf32[33], buf32[34], buf32[35]]);
        assert_eq!((slot_dw0 >> 20) & 0x0F, 3); // HighSpeed
        assert_eq!((slot_dw0 >> 27) & 0x1F, 1); // 1 context entry

        // EP0 Context at offset 64
        let ep0_dw1 = u32::from_le_bytes([buf32[68], buf32[69], buf32[70], buf32[71]]);
        assert_eq!((ep0_dw1 >> 16) & 0xFFFF, 64); // MaxPacketSize = 64

        // Test 64-byte context (CSZ = 1)
        let mut buf64 = [0u8; 256];
        format_input_context(&mut buf64, true, 5, PortSpeed::HighSpeed, 64, 0x5000_0000);

        // Slot Context at offset 64
        let slot_dw0_64 = u32::from_le_bytes([buf64[64], buf64[65], buf64[66], buf64[67]]);
        assert_eq!((slot_dw0_64 >> 20) & 0x0F, 3);

        // EP0 Context at offset 128
        let ep0_dw1_64 = u32::from_le_bytes([buf64[132], buf64[133], buf64[134], buf64[135]]);
        assert_eq!((ep0_dw1_64 >> 16) & 0xFFFF, 64);
    }

    #[test]
    fn test_usb_device_descriptor_parsing() {
        // 18-byte QEMU USB Mouse descriptor
        let raw_desc: [u8; 18] = [
            18,                   // bLength
            1,                    // bDescriptorType (Device)
            0x00, 0x02,           // bcdUSB = 2.00
            0x00,                 // bDeviceClass (Defined at Interface level)
            0x00,                 // bDeviceSubClass
            0x00,                 // bDeviceProtocol
            64,                   // bMaxPacketSize0
            0x27, 0x06,           // idVendor = 0x0627
            0x01, 0x00,           // idProduct = 0x0001
            0x00, 0x01,           // bcdDevice = 1.00
            1, 2, 3,              // String indices
            1,                    // bNumConfigurations
        ];

        let desc = UsbDeviceDescriptor::parse(&raw_desc).expect("Failed to parse device descriptor");
        assert_eq!(desc.b_length, 18);
        assert_eq!(desc.b_descriptor_type, 1);
        assert_eq!(desc.bcd_usb, 0x0200);
        assert_eq!(desc.id_vendor, 0x0627);
        assert_eq!(desc.id_product, 0x0001);
        assert_eq!(desc.b_max_packet_size0, 64);
        assert_eq!(desc.b_num_configurations, 1);
    }
}
