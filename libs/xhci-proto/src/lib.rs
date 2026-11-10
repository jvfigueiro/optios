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
pub const PORTSC_PP: u32 = 1 << 9;     // Port Power
pub const PORTSC_SPEED_MASK: u32 = 0x0F << 10;
pub const PORTSC_SPEED_SHIFT: u32 = 10;

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

// TRB Control bit flags
pub const TRB_CYCLE: u32 = 1 << 0;
pub const TRB_TC: u32 = 1 << 1;        // Toggle Cycle (for Link TRB)
pub const TRB_CH: u32 = 1 << 4;        // Chain
pub const TRB_IOC: u32 = 1 << 5;       // Interrupt On Completion

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
        // MaxSlots=64 (0x40), MaxIntrs=8 (0x008), MaxPorts=4 (0x04)
        // 0x04000840
        let val: u32 = (4 << 24) | (8 << 8) | 64;
        let (slots, intrs, ports) = parse_hcsparams1(val);
        assert_eq!(slots, 64);
        assert_eq!(intrs, 8);
        assert_eq!(ports, 4);
    }

    #[test]
    fn test_parse_portsc() {
        // Connected=1, Enabled=1, Powered=1, Speed=4 (SuperSpeed)
        // val = 1 | 2 | (1 << 9) | (4 << 10) = 1 | 2 | 512 | 4096 = 4611 (0x1203)
        let val: u32 = PORTSC_CCS | PORTSC_PED | PORTSC_PP | (4 << PORTSC_SPEED_SHIFT);
        let status = parse_portsc(val);
        assert!(status.connected);
        assert!(status.enabled);
        assert!(status.powered);
        assert_eq!(status.speed, PortSpeed::SuperSpeed);
        assert_eq!(status.speed_name(), "SuperSpeed (5 Gbps)");
    }
}
