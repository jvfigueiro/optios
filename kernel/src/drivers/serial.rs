// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: 16550 UART Serial Port Driver (COM1 - 0x3F8)
// Date: 2026-11-10

//! 16550 UART Serial Port Driver (COM1 - 0x3F8).
//! Primary low-level telemetry channel for early kernel debugging.

use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::arch::x86_64::io::{inb, outb};

const COM1: u16 = 0x3F8;

/// 16550 UART serial port controller.
pub struct SerialPort {
    port: u16,
}

impl SerialPort {
    pub const fn new(port: u16) -> Self {
        Self { port }
    }

    /// Initializes the UART controller to 38400 baud, 8N1, FIFO enabled.
    pub unsafe fn init(&self) {
        // Disable all interrupts
        outb(self.port + 1, 0x00);
        // Enable DLAB (Divisor Latch Access Bit)
        outb(self.port + 3, 0x80);
        // Set divisor to 3 for 38400 baud (clock 115200 / 3)
        outb(self.port + 0, 0x03);
        outb(self.port + 1, 0x00);
        // 8 data bits, no parity, 1 stop bit (8N1)
        outb(self.port + 3, 0x03);
        // Enable FIFO, clear buffers, 14-byte threshold
        outb(self.port + 2, 0xC7);
        // Enable RTS/DSR lines and auxiliary output
        outb(self.port + 4, 0x0B);
    }

    /// Checks if the transmitter holding register is empty.
    #[inline]
    fn is_transmit_empty(&self) -> bool {
        unsafe { (inb(self.port + 5) & 0x20) != 0 }
    }

    /// Transmits a single byte over the serial port.
    pub fn send_byte(&self, byte: u8) {
        while !self.is_transmit_empty() {
            core::hint::spin_loop();
        }
        unsafe {
            outb(self.port, byte);
        }
    }

    /// Writes a UTF-8 string slice over the serial port.
    pub fn write_str(&self, s: &str) {
        for byte in s.bytes() {
            if byte == b'\n' {
                self.send_byte(b'\r');
            }
            self.send_byte(byte);
        }
    }
}

impl fmt::Write for SerialPort {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        SerialPort::write_str(self, s);
        Ok(())
    }
}

/// Static COM1 serial port instance.
pub static SERIAL1: SerialPort = SerialPort::new(COM1);

#[allow(dead_code)]
static SERIAL_LOCK: AtomicBool = AtomicBool::new(false);

/// Thread-safe synchronized serial print function.
#[allow(dead_code)]
pub fn print_serial(args: fmt::Arguments) {
    use core::fmt::Write;
    while SERIAL_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let mut port = SerialPort::new(COM1);
    let _ = port.write_fmt(args);

    SERIAL_LOCK.store(false, Ordering::Release);
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::drivers::serial::print_serial(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => ($crate::serial_print!("{}\n", format_args!($($arg)*)));
}
