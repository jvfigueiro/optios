// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: PS/2 Keyboard Driver (IRQ 1, Lock-Free Ring Buffer, Scan Code Set 1)
// Date: 2026-11-09

//! PS/2 Keyboard Driver (8042 Controller, IRQ 1 / Vector 33).
//! Uses a lock-free SPSC ring buffer for ISR scancode queuing and Scan Code Set 1 translation.

use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use crate::arch::x86_64::io::inb;
use crate::info;

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;

const QUEUE_SIZE: usize = 256;

static mut SCANCODE_BUFFER: [u8; QUEUE_SIZE] = [0; QUEUE_SIZE];
static WRITE_IDX: AtomicUsize = AtomicUsize::new(0);
static READ_IDX: AtomicUsize = AtomicUsize::new(0);

static SHIFT_ACTIVE: AtomicBool = AtomicBool::new(false);
static EXTENDED_ACTIVE: AtomicBool = AtomicBool::new(false);

/// High-level decoded keyboard events.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    Char(char),
    Enter,
    Escape,
    Backspace,
    Tab,
    F1,
    F2,
    F3,
    F4,
    ArrowUp,
    ArrowDown,
    ArrowLeft,
    ArrowRight,
}

/// Initializes the PS/2 keyboard driver by flushing any stale bytes in the 8042 output buffer.
pub fn init() {
    unsafe {
        // Flush pending bytes from the 8042 data port (up to 64 iterations)
        for _ in 0..64 {
            if (inb(PS2_STATUS_PORT) & 0x01) != 0 {
                let _ = inb(PS2_DATA_PORT);
            } else {
                break;
            }
        }
    }
    info!("PS/2 Keyboard initialized: Listening on IRQ 1 (port 0x60, Scan Code Set 1).");
}

/// Invoked by the IRQ 1 (Vector 33) interrupt service routine.
/// Reads a single scancode from port 0x60 and enqueues it into the lock-free ring buffer.
#[inline]
pub fn on_irq1() {
    let scancode = unsafe { inb(PS2_DATA_PORT) };
    push_scancode(scancode);
}

/// Pushes a raw scancode into the circular buffer.
#[inline]
pub fn push_scancode(scancode: u8) {
    let write = WRITE_IDX.load(Ordering::Relaxed);
    let next_write = (write + 1) % QUEUE_SIZE;
    let read = READ_IDX.load(Ordering::Acquire);

    if next_write != read {
        unsafe {
            let buf_ptr = &raw mut SCANCODE_BUFFER;
            (*buf_ptr)[write] = scancode;
        }
        WRITE_IDX.store(next_write, Ordering::Release);
    }
}

/// Pops a raw scancode from the circular buffer, if available.
#[inline]
fn pop_scancode() -> Option<u8> {
    let read = READ_IDX.load(Ordering::Relaxed);
    let write = WRITE_IDX.load(Ordering::Acquire);

    if read == write {
        return None;
    }

    let scancode = unsafe {
        let buf_ptr = &raw const SCANCODE_BUFFER;
        (*buf_ptr)[read]
    };
    READ_IDX.store((read + 1) % QUEUE_SIZE, Ordering::Release);
    Some(scancode)
}

/// Polls and decodes the next `KeyEvent` from the scancode queue.
pub fn poll_key() -> Option<KeyEvent> {
    while let Some(code) = pop_scancode() {
        if code == 0xE0 {
            EXTENDED_ACTIVE.store(true, Ordering::Relaxed);
            continue;
        }

        let is_extended = EXTENDED_ACTIVE.swap(false, Ordering::Relaxed);

        // Handle key release (bit 7 set)
        if (code & 0x80) != 0 {
            let released = code & 0x7F;
            if !is_extended && (released == 0x2A || released == 0x36) {
                SHIFT_ACTIVE.store(false, Ordering::Relaxed);
            }
            continue;
        }

        // Handle extended key presses (0xE0 prefixed)
        if is_extended {
            match code {
                0x48 => return Some(KeyEvent::ArrowUp),
                0x50 => return Some(KeyEvent::ArrowDown),
                0x4B => return Some(KeyEvent::ArrowLeft),
                0x4D => return Some(KeyEvent::ArrowRight),
                _ => continue,
            }
        }

        // Handle standard Scan Code Set 1 key presses
        match code {
            0x2A | 0x36 => {
                SHIFT_ACTIVE.store(true, Ordering::Relaxed);
                continue;
            }
            0x01 => return Some(KeyEvent::Escape),
            0x0E => return Some(KeyEvent::Backspace),
            0x0F => return Some(KeyEvent::Tab),
            0x1C => return Some(KeyEvent::Enter),
            0x3B => return Some(KeyEvent::F1),
            0x3C => return Some(KeyEvent::F2),
            0x3D => return Some(KeyEvent::F3),
            0x3E => return Some(KeyEvent::F4),
            _ => {
                let shift = SHIFT_ACTIVE.load(Ordering::Relaxed);
                if let Some(ch) = translate_ascii(code, shift) {
                    return Some(KeyEvent::Char(ch));
                }
            }
        }
    }
    None
}

/// Translates Scan Code Set 1 byte into an ASCII character (US QWERTY layout).
fn translate_ascii(code: u8, shift: bool) -> Option<char> {
    let ch = match code {
        0x02 => if shift { '!' } else { '1' },
        0x03 => if shift { '@' } else { '2' },
        0x04 => if shift { '#' } else { '3' },
        0x05 => if shift { '$' } else { '4' },
        0x06 => if shift { '%' } else { '5' },
        0x07 => if shift { '^' } else { '6' },
        0x08 => if shift { '&' } else { '7' },
        0x09 => if shift { '*' } else { '8' },
        0x0A => if shift { '(' } else { '9' },
        0x0B => if shift { ')' } else { '0' },
        0x0C => if shift { '_' } else { '-' },
        0x0D => if shift { '+' } else { '=' },
        0x10 => if shift { 'Q' } else { 'q' },
        0x11 => if shift { 'W' } else { 'w' },
        0x12 => if shift { 'E' } else { 'e' },
        0x13 => if shift { 'R' } else { 'r' },
        0x14 => if shift { 'T' } else { 't' },
        0x15 => if shift { 'Y' } else { 'y' },
        0x16 => if shift { 'U' } else { 'u' },
        0x17 => if shift { 'I' } else { 'i' },
        0x18 => if shift { 'O' } else { 'o' },
        0x19 => if shift { 'P' } else { 'p' },
        0x1A => if shift { '{' } else { '[' },
        0x1B => if shift { '}' } else { ']' },
        0x1E => if shift { 'A' } else { 'a' },
        0x1F => if shift { 'S' } else { 's' },
        0x20 => if shift { 'D' } else { 'd' },
        0x21 => if shift { 'F' } else { 'f' },
        0x22 => if shift { 'G' } else { 'g' },
        0x23 => if shift { 'H' } else { 'h' },
        0x24 => if shift { 'J' } else { 'j' },
        0x25 => if shift { 'K' } else { 'k' },
        0x26 => if shift { 'L' } else { 'l' },
        0x27 => if shift { ':' } else { ';' },
        0x28 => if shift { '"' } else { '\'' },
        0x29 => if shift { '~' } else { '`' },
        0x2B => if shift { '|' } else { '\\' },
        0x2C => if shift { 'Z' } else { 'z' },
        0x2D => if shift { 'X' } else { 'x' },
        0x2E => if shift { 'C' } else { 'c' },
        0x2F => if shift { 'V' } else { 'v' },
        0x30 => if shift { 'B' } else { 'b' },
        0x31 => if shift { 'N' } else { 'n' },
        0x32 => if shift { 'M' } else { 'm' },
        0x33 => if shift { '<' } else { ',' },
        0x34 => if shift { '>' } else { '.' },
        0x35 => if shift { '?' } else { '/' },
        0x39 => ' ',
        _ => return None,
    };
    Some(ch)
}
