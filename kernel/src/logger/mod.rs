// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Dual-Output Kernel Logger (Serial COM1 + Framebuffer Console + Dmesg)
// Date: 2026-11-10

//! Structured logging subsystem for OptiOS.
//! Routes formatted logs in real-time to both the serial console (COM1) and the
//! fullscreen text console on the Framebuffer, and records entries into `dmesg`.

use core::fmt;

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

    // Output to Framebuffer console and dmesg if available, or fallback to serial
    if let Ok(s) = core::str::from_utf8(output_bytes) {
        crate::drivers::video::console::record_dmesg(s.trim_end());
        if crate::drivers::video::console::is_initialized() {
            crate::drivers::video::console::print_str(s);
        } else {
            for &b in output_bytes {
                crate::drivers::serial::SERIAL1.send_byte(b);
            }
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
