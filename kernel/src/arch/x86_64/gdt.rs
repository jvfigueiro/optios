// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Global Descriptor Table (GDT) & Task State Segment (TSS)
// Date: 2026-11-10

//! Global Descriptor Table (GDT) and Task State Segment (TSS) for OptiOS x86-64.

use core::arch::asm;
use core::mem::size_of;

pub const KERNEL_CODE_SELECTOR: u16 = 0x08;
pub const KERNEL_DATA_SELECTOR: u16 = 0x10;
#[allow(dead_code)]
pub const USER_DATA_SELECTOR: u16 = 0x18 | 3;
#[allow(dead_code)]
pub const USER_CODE_SELECTOR: u16 = 0x20 | 3;
pub const TSS_SELECTOR: u16 = 0x28;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 1;

/// x86-64 Task State Segment structure (104 bytes).
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved0: u32,
    pub rsp: [u64; 3],
    reserved1: u64,
    /// Interrupt Stack Table pointers (IST 1..=7).
    pub ist: [u64; 7],
    reserved2: u64,
    reserved3: u16,
    pub iomap_base: u16,
}

impl TaskStateSegment {
    pub const fn new() -> Self {
        Self {
            reserved0: 0,
            rsp: [0; 3],
            reserved1: 0,
            ist: [0; 7],
            reserved2: 0,
            reserved3: 0,
            iomap_base: size_of::<TaskStateSegment>() as u16,
        }
    }
}

/// Dedicated 16 KiB emergency stack for Double Fault exception handling.
#[repr(align(16))]
struct DoubleFaultStack(#[allow(dead_code)] [u8; 16384]);
static mut DOUBLE_FAULT_STACK: DoubleFaultStack = DoubleFaultStack([0; 16384]);

/// Static kernel TSS instance.
static mut TSS: TaskStateSegment = TaskStateSegment::new();

/// Updates TSS.rsp[0] with the dedicated kernel stack for Ring 3 -> Ring 0 transitions.
pub fn set_kernel_stack(stack_top: u64) {
    unsafe {
        let tss_ptr = &raw mut TSS;
        (*tss_ptr).rsp[0] = stack_top;
    }
}

/// GDT Pointer structure loaded by `lgdt`.
#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// GDT storage:
/// 0: Null Descriptor
/// 1: Kernel Code 64-bit (0x08)
/// 2: Kernel Data 64-bit (0x10)
/// 3: User Data 64-bit   (0x18)
/// 4: User Code 64-bit   (0x20)
/// 5, 6: TSS Descriptor  (0x28, 16 bytes)
static mut GDT: [u64; 7] = [0; 7];

/// Builds a 64-bit TSS descriptor (occupies two consecutive 8-byte GDT slots).
fn make_tss_descriptor(tss_addr: u64, tss_size: u64) -> (u64, u64) {
    let limit = tss_size - 1;
    let base = tss_addr;

    let mut low = limit & 0xFFFF;
    low |= (base & 0x00FF_FFFF) << 16;
    low |= 0x89 << 40; // Present=1, DPL=0, Type=9 (64-bit TSS available)
    low |= ((limit >> 16) & 0x0F) << 48;
    low |= (base & 0xFF00_0000) << 32;

    let high = base >> 32;
    (low, high)
}

/// Initializes the OptiOS GDT and TSS, loading new selectors and emergency stacks.
pub fn init() {
    unsafe {
        // Setup dedicated stack for Double Fault in IST 1
        let stack_ptr = (&raw const DOUBLE_FAULT_STACK) as *const u8;
        let stack_top = stack_ptr as u64 + 16384;

        let tss_ptr = &raw mut TSS;
        (*tss_ptr).ist[DOUBLE_FAULT_IST_INDEX as usize - 1] = stack_top;

        let tss_addr = tss_ptr as u64;
        let tss_size = size_of::<TaskStateSegment>() as u64;
        let (tss_low, tss_high) = make_tss_descriptor(tss_addr, tss_size);

        let gdt_ptr_raw = &raw mut GDT;
        // Build GDT entries
        (*gdt_ptr_raw)[0] = 0; // Null descriptor
        (*gdt_ptr_raw)[1] = 0x0020_9A00_0000_0000; // Kernel Code 64-bit: Present, Ring 0, Executable, Readable, Long Mode
        (*gdt_ptr_raw)[2] = 0x0000_9200_0000_0000; // Kernel Data: Present, Ring 0, Writable
        (*gdt_ptr_raw)[3] = 0x0000_F200_0000_0000; // User Data: Present, Ring 3, Writable
        (*gdt_ptr_raw)[4] = 0x0020_FA00_0000_0000; // User Code 64-bit: Present, Ring 3, Executable, Readable, Long Mode
        (*gdt_ptr_raw)[5] = tss_low;
        (*gdt_ptr_raw)[6] = tss_high;

        let gdt_ptr = GdtPointer {
            limit: (size_of::<[u64; 7]>() - 1) as u16,
            base: gdt_ptr_raw as u64,
        };

        // Load new GDT
        asm!("lgdt [{}]", in(reg) &gdt_ptr, options(readonly, nostack, preserves_flags));

        // Reload CS via far return
        asm!(
            "push {sel}",
            "lea {tmp}, [2f + rip]",
            "push {tmp}",
            "retfq",
            "2:",
            sel = in(reg) KERNEL_CODE_SELECTOR as u64,
            tmp = out(reg) _,
            options(preserves_flags)
        );

        // Reload data segment registers
        asm!(
            "mov ds, {sel:x}",
            "mov es, {sel:x}",
            "mov fs, {sel:x}",
            "mov gs, {sel:x}",
            "mov ss, {sel:x}",
            sel = in(reg) KERNEL_DATA_SELECTOR,
            options(nomem, nostack, preserves_flags)
        );

        // Load Task Register (TSS)
        asm!("ltr {sel:x}", sel = in(reg) TSS_SELECTOR, options(nomem, nostack, preserves_flags));
    }
}
