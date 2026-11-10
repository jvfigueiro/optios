// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Structured Kernel Logging Macros (info, warn, error)
// Date: 2026-11-09

//! Structured logging subsystem for OptiOS.

#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {
        $crate::serial_println!("[INFO ] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {
        $crate::serial_println!("[WARN ] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::serial_println!("[ERROR] {}", format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        $crate::serial_println!("[DEBUG] {}", format_args!($($arg)*))
    };
}
