// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Dual-Output Kernel Logger (Serial COM1 + Framebuffer Console + Dmesg)
// Date: 2026-11-10

//! Structured logging subsystem for OptiOS.
//! Routes formatted logs in real-time to both the serial console (COM1) and the
//! fullscreen text console on the Framebuffer, and records entries into `dmesg`.

use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

static MUTE_CONSOLE: AtomicBool = AtomicBool::new(false);

/// Sets whether kernel logs are forwarded to the video text console.
/// When muted, logs are still routed to COM1 serial and the dmesg ring buffer.
pub fn set_console_logging(enabled: bool) {
    MUTE_CONSOLE.store(!enabled, Ordering::Relaxed);
}

struct StackWriter<'a> {
    buf: &'a mut [u8],
    len: usize,
}

impl<'a> fmt::Write for StackWriter<'a> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        for b in s.bytes() {
            if self.len < self.buf.len() {
                self.buf[self.len] = b;
                self.len += 1;
            }
        }
        Ok(())
    }
}

pub fn log_impl(prefix: &str, args: fmt::Arguments) {
    let mut buffer = [0u8; 1024];
    let mut writer = StackWriter {
        buf: &mut buffer,
        len: 0,
    };

    use fmt::Write;
    let _ = write!(writer, "{} {}\n", prefix, args);

    let output_bytes = &writer.buf[..writer.len];

    // Always output formatted logs to COM1 serial
    for &b in output_bytes {
        if b == b'\n' {
            crate::drivers::serial::SERIAL1.send_byte(b'\r');
        }
        crate::drivers::serial::SERIAL1.send_byte(b);
    }

    // Record into dmesg and forward to video text console if not muted
    if let Ok(s) = core::str::from_utf8(output_bytes) {
        crate::drivers::video::console::record_dmesg(s.trim_end());
        if !MUTE_CONSOLE.load(Ordering::Relaxed) && crate::drivers::video::console::is_initialized() {
            crate::drivers::video::console::print_to_screen_only(s);
        }
    }
}

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::logger::log_impl("[INFO ]", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::logger::log_impl("[WARN ]", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::log_impl("[ERROR]", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::logger::log_impl("[DEBUG]", format_args!($($arg)*))
    };
}
