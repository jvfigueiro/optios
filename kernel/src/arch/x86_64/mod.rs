// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: x86-64 Architecture Support & Low-Level CPU Control
// Date: 2026-11-10

//! x86-64 architecture support and low-level CPU control.

pub mod cpuid;
pub mod gdt;
pub mod idt;
pub mod io;
pub mod msr;
pub mod pic;
pub mod pit;

/// Halts the CPU until the next external interrupt arrives.
#[inline]
pub fn hlt() {
    unsafe {
        core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
    }
}

/// Disables interrupts on the current CPU core.
#[inline]
pub fn cli() {
    unsafe {
        core::arch::asm!("cli", options(nomem, nostack, preserves_flags));
    }
}

/// Enables interrupts on the current CPU core.
#[inline]
pub fn sti() {
    unsafe {
        core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
    }
}

/// Disables interrupts and halts the CPU indefinitely.
#[inline]
pub fn halt_loop() -> ! {
    loop {
        cli();
        hlt();
    }
}
