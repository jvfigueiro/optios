// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Fullscreen Classical Text Console (/dev/tty0 Style) & Dmesg Ring Buffer
// Date: 2026-11-10

//! Fullscreen Classical Text Console for OptiOS Framebuffer.
//! Replaces multi-tab graphical dashboards with a crisp VT100/VGA-style text console.
//! Provides real-time verbose boot logging, command prompt display, automatic line scrolling,
//! and an in-memory `dmesg` buffer for reviewing boot diagnostics.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

use super::font::FONT_WIDTH;
use super::framebuffer::with_framebuffer;

pub const DEFAULT_FG: u32 = 0x00E2E8F0; // Crisp light gray / VT100
pub const DEFAULT_BG: u32 = 0x00000000; // Classic terminal black

const MAX_DMESG_ENTRIES: usize = 1024;

struct ConsoleState {
    cursor_x: usize,
    cursor_y: usize,
    cols: usize,
    rows: usize,
    fg_color: u32,
    bg_color: u32,
    initialized: bool,
}

static mut CONSOLE: ConsoleState = ConsoleState {
    cursor_x: 0,
    cursor_y: 0,
    cols: 160,
    rows: 50,
    fg_color: DEFAULT_FG,
    bg_color: DEFAULT_BG,
    initialized: false,
};

static mut DMESG: Option<Vec<String>> = None;
static CONSOLE_LOCK: AtomicBool = AtomicBool::new(false);

/// Initializes the fullscreen text console for the given screen dimensions.
pub fn init(width: usize, height: usize) {
    while CONSOLE_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    unsafe {
        let cols = width / FONT_WIDTH;
        let rows = height / 16;
        CONSOLE.cols = cols;
        CONSOLE.rows = rows;
        CONSOLE.cursor_x = 0;
        CONSOLE.cursor_y = 0;
        CONSOLE.fg_color = DEFAULT_FG;
        CONSOLE.bg_color = DEFAULT_BG;
        CONSOLE.initialized = true;

        let dmesg_ptr = &raw mut DMESG;
        if (*dmesg_ptr).is_none() {
            *dmesg_ptr = Some(Vec::with_capacity(128));
        }
    }

    with_framebuffer(|fb| {
        fb.clear(DEFAULT_BG);
        fb.present();
    });

    CONSOLE_LOCK.store(false, Ordering::Release);
}

/// Returns whether the console is ready for rendering.
pub fn is_initialized() -> bool {
    unsafe { CONSOLE.initialized }
}

/// Appends a line to the in-memory dmesg buffer.
pub fn record_dmesg(line: &str) {
    unsafe {
        let dmesg_ptr = &raw mut DMESG;
        if let Some(ref mut dmesg) = *dmesg_ptr {
            if dmesg.len() >= MAX_DMESG_ENTRIES {
                dmesg.remove(0);
            }
            dmesg.push(String::from(line));
        }
    }
}

/// Returns a clone of all logged dmesg lines.
pub fn get_dmesg() -> Vec<String> {
    unsafe {
        let dmesg_ptr = &raw const DMESG;
        if let Some(ref dmesg) = *dmesg_ptr {
            dmesg.clone()
        } else {
            Vec::new()
        }
    }
}

/// Writes a string to the fullscreen console and flushes to the screen.
pub fn print_str(s: &str) {
    for b in s.bytes() {
        if b == b'\n' {
            crate::drivers::serial::SERIAL1.send_byte(b'\r');
        }
        crate::drivers::serial::SERIAL1.send_byte(b);
    }

    while CONSOLE_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    if !unsafe { CONSOLE.initialized } {
        CONSOLE_LOCK.store(false, Ordering::Release);
        return;
    }

    with_framebuffer(|fb| {
        unsafe {
            for c in s.chars() {
                match c {
                    '\n' => {
                        CONSOLE.cursor_x = 0;
                        CONSOLE.cursor_y += 1;
                        if CONSOLE.cursor_y >= CONSOLE.rows {
                            fb.scroll_up(1, CONSOLE.bg_color);
                            CONSOLE.cursor_y = CONSOLE.rows - 1;
                        }
                    }
                    '\r' => {
                        CONSOLE.cursor_x = 0;
                    }
                    '\x08' => {
                        if CONSOLE.cursor_x > 0 {
                            CONSOLE.cursor_x -= 1;
                            fb.draw_char(
                                CONSOLE.cursor_x * FONT_WIDTH,
                                CONSOLE.cursor_y * 16,
                                ' ',
                                CONSOLE.fg_color,
                                Some(CONSOLE.bg_color),
                            );
                        }
                    }
                    ch => {
                        fb.draw_char(
                            CONSOLE.cursor_x * FONT_WIDTH,
                            CONSOLE.cursor_y * 16,
                            ch,
                            CONSOLE.fg_color,
                            Some(CONSOLE.bg_color),
                        );
                        CONSOLE.cursor_x += 1;
                        if CONSOLE.cursor_x >= CONSOLE.cols {
                            CONSOLE.cursor_x = 0;
                            CONSOLE.cursor_y += 1;
                            if CONSOLE.cursor_y >= CONSOLE.rows {
                                fb.scroll_up(1, CONSOLE.bg_color);
                                CONSOLE.cursor_y = CONSOLE.rows - 1;
                            }
                        }
                    }
                }
            }
            fb.present();
        }
    });

    CONSOLE_LOCK.store(false, Ordering::Release);
}

/// Clears the fullscreen console buffer and resets cursor to top-left (0, 0).
pub fn clear() {
    while CONSOLE_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    unsafe {
        CONSOLE.cursor_x = 0;
        CONSOLE.cursor_y = 0;
    }

    with_framebuffer(|fb| {
        fb.clear(DEFAULT_BG);
        fb.present();
    });

    CONSOLE_LOCK.store(false, Ordering::Release);
}

/// Formatter wrapper allowing `write!` and `writeln!` directly to the console.
#[allow(dead_code)]
pub struct ConsoleWriter;

impl fmt::Write for ConsoleWriter {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        print_str(s);
        Ok(())
    }
}
