// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Dual 8259 Programmable Interrupt Controller (PIC) Driver
// Date: 2026-11-10

//! Dual 8259 Programmable Interrupt Controller (PIC) driver.
//! Remaps legacy hardware IRQs 0..15 to IDT vectors 32..47 (0x20..0x2F).

use crate::arch::x86_64::io::{inb, io_wait, outb};
use crate::info;

const PIC1_CMD: u16 = 0x20;
const PIC1_DATA: u16 = 0x21;
const PIC2_CMD: u16 = 0xA0;
const PIC2_DATA: u16 = 0xA1;

const PIC_EOI: u8 = 0x20;

const ICW1_INIT: u8 = 0x10;
const ICW1_ICW4: u8 = 0x01;
const ICW4_8086: u8 = 0x01;

pub const PIC1_OFFSET: u8 = 0x20; // Vector 32..39
pub const PIC2_OFFSET: u8 = 0x28; // Vector 40..47

/// Initializes and remaps the Master and Slave 8259 PICs to vectors 32..47.
/// Unmasks strictly IRQ 0 (PIT System Timer) and IRQ 1 (PS/2 Keyboard).
pub fn init() {
    unsafe {
        // ICW1: Start initialization sequence in cascade mode
        outb(PIC1_CMD, ICW1_INIT | ICW1_ICW4);
        io_wait();
        outb(PIC2_CMD, ICW1_INIT | ICW1_ICW4);
        io_wait();

        // ICW2: Vector offsets (32 for Master, 40 for Slave)
        outb(PIC1_DATA, PIC1_OFFSET);
        io_wait();
        outb(PIC2_DATA, PIC2_OFFSET);
        io_wait();

        // ICW3: Tell Master PIC there is a Slave PIC at IRQ2 (0000 0100)
        outb(PIC1_DATA, 0x04);
        io_wait();
        // Tell Slave PIC its cascade identity (0000 0010)
        outb(PIC2_DATA, 0x02);
        io_wait();

        // ICW4: 8086/88 mode
        outb(PIC1_DATA, ICW4_8086);
        io_wait();
        outb(PIC2_DATA, ICW4_8086);
        io_wait();

        // OCW1: Mask all IRQs except IRQ 0 (Timer) and IRQ 1 (Keyboard) on Master
        outb(PIC1_DATA, 0b1111_1100);
        io_wait();
        outb(PIC2_DATA, 0b1111_1111);
        io_wait();
    }

    info!("8259 PIC initialized: IRQs remapped to vectors 32..47 (IRQ 0 & IRQ 1 unmasked).");
}

/// Unmasks a specific hardware IRQ line (0..15).
pub fn unmask_irq(irq: u8) {
    unsafe {
        if irq < 8 {
            let mask = inb(PIC1_DATA) & !(1 << irq);
            outb(PIC1_DATA, mask);
        } else {
            let mask = inb(PIC2_DATA) & !(1 << (irq - 8));
            outb(PIC2_DATA, mask);
            // Ensure cascade IRQ 2 is unmasked on Master PIC
            let master_mask = inb(PIC1_DATA) & !(1 << 2);
            outb(PIC1_DATA, master_mask);
        }
    }
}

/// Sends the End-Of-Interrupt (EOI) command to the PIC(s) for the given IRQ number (0..15).
#[inline]
pub fn send_eoi(irq: u8) {
    unsafe {
        if irq >= 8 {
            outb(PIC2_CMD, PIC_EOI);
        }
        outb(PIC1_CMD, PIC_EOI);
    }
}
