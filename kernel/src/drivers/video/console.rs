// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
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
use optios_ansi_parser::{AnsiAction, AnsiParser};

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
    bold: bool,
    ansi: AnsiParser,
    utf8_lead: u8,
    initialized: bool,
}

static mut CONSOLE: ConsoleState = ConsoleState {
    cursor_x: 0,
    cursor_y: 0,
    cols: 160,
    rows: 50,
    fg_color: DEFAULT_FG,
    bg_color: DEFAULT_BG,
    bold: false,
    ansi: AnsiParser::new(),
    utf8_lead: 0,
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

#[inline]
fn render_char_glyph(
    fb: &mut crate::drivers::video::framebuffer::FramebufferDriver,
    con: *mut ConsoleState,
    ch: char,
) {
    unsafe {
        let fg = if (*con).bold && (*con).fg_color == DEFAULT_FG {
            0x00FFFFFF // Bright white for bold default
        } else {
            (*con).fg_color
        };

        fb.draw_char(
            (*con).cursor_x * FONT_WIDTH,
            (*con).cursor_y * 16,
            ch,
            fg,
            Some((*con).bg_color),
        );
        (*con).cursor_x += 1;
        if (*con).cursor_x >= (*con).cols {
            (*con).cursor_x = 0;
            (*con).cursor_y += 1;
            if (*con).cursor_y >= (*con).rows {
                fb.scroll_up(1, (*con).bg_color);
                (*con).cursor_y = (*con).rows - 1;
            }
        }
    }
}

/// Writes a string to the fullscreen console and COM1 serial, then flushes to the screen.
pub fn print_str(s: &str) {
    print_str_internal(s, true);
}

/// Writes a string directly to the video screen only (without forwarding to COM1 serial).
pub fn print_to_screen_only(s: &str) {
    print_str_internal(s, false);
}

fn print_str_internal(s: &str, send_serial: bool) {
    if send_serial {
        for b in s.bytes() {
            if b == b'\n' {
                crate::drivers::serial::SERIAL1.send_byte(b'\r');
            }
            crate::drivers::serial::SERIAL1.send_byte(b);
        }
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
            let con = &raw mut CONSOLE;
            for b in s.bytes() {
                (*con).ansi.parse_byte(b, |action| match action {
                    AnsiAction::Print(byte) => {
                        // 2-byte UTF-8 decoding
                        if (*con).utf8_lead != 0 {
                            let lead = (*con).utf8_lead;
                            (*con).utf8_lead = 0;
                            if (byte & 0xC0) == 0x80 {
                                let cp = (((lead as u32) & 0x1F) << 6) | ((byte as u32) & 0x3F);
                                if let Some(ch) = core::char::from_u32(cp) {
                                    render_char_glyph(fb, con, ch);
                                    return;
                                }
                            }
                        }

                        if (byte & 0xE0) == 0xC0 {
                            (*con).utf8_lead = byte;
                            return;
                        }

                        match byte {
                            b'\n' => {
                                (*con).cursor_x = 0;
                                (*con).cursor_y += 1;
                                if (*con).cursor_y >= (*con).rows {
                                    fb.scroll_up(1, (*con).bg_color);
                                    (*con).cursor_y = (*con).rows - 1;
                                }
                            }
                            b'\r' => {
                                (*con).cursor_x = 0;
                            }
                            0x08 => {
                                if (*con).cursor_x > 0 {
                                    (*con).cursor_x -= 1;
                                    fb.draw_char(
                                        (*con).cursor_x * FONT_WIDTH,
                                        (*con).cursor_y * 16,
                                        ' ',
                                        (*con).fg_color,
                                        Some((*con).bg_color),
                                    );
                                }
                            }
                            byte_val => {
                                render_char_glyph(fb, con, byte_val as char);
                            }
                        }
                    }
                    AnsiAction::SetFg(color) => {
                        (*con).fg_color = color.to_rgb32(false);
                    }
                    AnsiAction::SetBg(color) => {
                        (*con).bg_color = color.to_rgb32(true);
                    }
                    AnsiAction::SetBold(bold) => {
                        (*con).bold = bold;
                    }
                    AnsiAction::ResetAttributes => {
                        (*con).fg_color = DEFAULT_FG;
                        (*con).bg_color = DEFAULT_BG;
                        (*con).bold = false;
                    }
                    AnsiAction::ClearScreen => {
                        (*con).cursor_x = 0;
                        (*con).cursor_y = 0;
                        fb.clear((*con).bg_color);
                    }
                    AnsiAction::ClearLine => {
                        for c in (*con).cursor_x..(*con).cols {
                            fb.draw_char(
                                c * FONT_WIDTH,
                                (*con).cursor_y * 16,
                                ' ',
                                (*con).fg_color,
                                Some((*con).bg_color),
                            );
                        }
                    }
                    AnsiAction::SetCursorPosition { row, col } => {
                        (*con).cursor_y = row.min((*con).rows.saturating_sub(1));
                        (*con).cursor_x = col.min((*con).cols.saturating_sub(1));
                    }
                });
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
