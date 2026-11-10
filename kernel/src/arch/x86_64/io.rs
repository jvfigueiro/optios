// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: x86-64 Port I/O Primitives (outb, inb, outw, inw, outl, inl, io_wait)
// Date: 2026-11-09

//! Low-level x86-64 port I/O instructions.

use core::arch::asm;

/// Writes an 8-bit byte to the specified I/O port.
#[inline]
pub unsafe fn outb(port: u16, val: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
        options(nomem, nostack, preserves_flags)
    );
}

/// Reads an 8-bit byte from the specified I/O port.
#[inline]
pub unsafe fn inb(port: u16) -> u8 {
    let val: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") val,
        options(nomem, nostack, preserves_flags)
    );
    val
}

/// Writes a 16-bit word to the specified I/O port.
#[inline]
pub unsafe fn outw(port: u16, val: u16) {
    asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") val,
        options(nomem, nostack, preserves_flags)
    );
}

/// Reads a 16-bit word from the specified I/O port.
#[inline]
pub unsafe fn inw(port: u16) -> u16 {
    let val: u16;
    asm!(
        "in ax, dx",
        in("dx") port,
        out("ax") val,
        options(nomem, nostack, preserves_flags)
    );
    val
}

/// Writes a 32-bit dword to the specified I/O port.
#[inline]
pub unsafe fn outl(port: u16, val: u32) {
    asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") val,
        options(nomem, nostack, preserves_flags)
    );
}

/// Reads a 32-bit dword from the specified I/O port.
#[inline]
pub unsafe fn inl(port: u16) -> u32 {
    let val: u32;
    asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") val,
        options(nomem, nostack, preserves_flags)
    );
    val
}

/// Brief I/O delay by writing to the unused 0x80 diagnostic port.
#[inline]
#[allow(dead_code)]
pub unsafe fn io_wait() {
    outb(0x80, 0);
}
