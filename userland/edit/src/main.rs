// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Standalone Userland Kilo-Style Text Inspector & Editor (/bin/edit)
// Date: 2026-11-10

//! Standalone Ring 3 userland text inspector and editor (`/bin/edit`).
//! Operates in unprivileged userland, loads file contents via POSIX VFS syscalls,
//! renders full terminal views using ANSI escape sequences (`\x1b[2J`, `\x1b[H`),
//! renders reverse-video status bars (`\x1b[7m`), and responds to arrow-key scrolling
//! and clean exit signals (`Ctrl+Q` / `0x11` or `'q'`).

#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::panic::PanicInfo;

use optios_libc::{close, exit, extract_args, open, print, read, OptiAllocator};

#[global_allocator]
static ALLOCATOR: OptiAllocator = OptiAllocator::new();

const SCREEN_ROWS: usize = 48;
const SCREEN_COLS: usize = 160;

struct Editor {
    filename: String,
    lines: Vec<String>,
    cursor_row: usize,
    cursor_col: usize,
    row_offset: usize,
    col_offset: usize,
    status_msg: String,
}

impl Editor {
    fn new(filename: &str) -> Self {
        Self {
            filename: String::from(filename),
            lines: Vec::new(),
            cursor_row: 0,
            cursor_col: 0,
            row_offset: 0,
            col_offset: 0,
            status_msg: String::from("HELP: [Ctrl+Q or Q] Exit | [Up/Down Arrows] Scroll"),
        }
    }

    fn load_file(&mut self, path: &str) {
        self.filename = String::from(path);
        match open(path, 0, 0) {
            Ok(fd) => {
                let mut content = Vec::new();
                let mut buf = [0u8; 1024];
                loop {
                    match read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => content.extend_from_slice(&buf[..n]),
                        Err(_) => break,
                    }
                }
                let _ = close(fd);

                if let Ok(text) = core::str::from_utf8(&content) {
                    self.lines = text.lines().map(String::from).collect();
                }
                if self.lines.is_empty() {
                    self.lines.push(String::new());
                }
                self.status_msg = format!("Loaded {} lines from '{}'", self.lines.len(), path);
            }
            Err(_) => {
                self.lines.push(String::new());
                self.status_msg = format!("Cannot open '{}' (New File)", path);
            }
        }
    }

    fn render(&self) {
        // Move home
        print!("\x1b[?25l\x1b[H");

        // 1. Text rows (1..=SCREEN_ROWS)
        for r in 0..SCREEN_ROWS {
            let file_row = self.row_offset + r;
            if file_row < self.lines.len() {
                let line = &self.lines[file_row];
                let display = if self.col_offset < line.len() {
                    let slice = &line[self.col_offset..];
                    if slice.len() > SCREEN_COLS {
                        &slice[..SCREEN_COLS]
                    } else {
                        slice
                    }
                } else {
                    ""
                };
                print!("{}", display);
            } else {
                print!("~");
            }
            // Clear line to end and newline
            print!("\x1b[K\r\n");
        }

        // 2. Status Bar (Row 49): Reverse video (\x1b[7m)
        let left_status = format!(
            " OptiOS Edit - {} - {} lines ",
            self.filename,
            self.lines.len()
        );
        let right_status = format!(
            " Line {}/{} Col {} | [Ctrl+Q] Exit ",
            self.cursor_row + 1,
            self.lines.len(),
            self.cursor_col + 1
        );

        let pad_len = if SCREEN_COLS > (left_status.len() + right_status.len()) {
            SCREEN_COLS - (left_status.len() + right_status.len())
        } else {
            0
        };

        print!("\x1b[7m");
        print!("{}", left_status);
        for _ in 0..pad_len {
            print!(" ");
        }
        print!("{}", right_status);
        print!("\x1b[m\r\n");

        // 3. Command/Message Bar (Row 50)
        print!("{}", self.status_msg);
        print!("\x1b[K");

        // 4. Position cursor & restore visibility
        let screen_cursor_row = (self.cursor_row - self.row_offset) + 1;
        let screen_cursor_col = (self.cursor_col - self.col_offset) + 1;
        print!("\x1b[{};{}H\x1b[?25h", screen_cursor_row, screen_cursor_col);
    }

    fn move_cursor(&mut self, key: u8) {
        match key {
            b'A' => {
                // Up
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    if self.cursor_row < self.row_offset {
                        self.row_offset = self.cursor_row;
                    }
                }
            }
            b'B' => {
                // Down
                if self.cursor_row + 1 < self.lines.len() {
                    self.cursor_row += 1;
                    if self.cursor_row >= self.row_offset + SCREEN_ROWS {
                        self.row_offset = self.cursor_row - SCREEN_ROWS + 1;
                    }
                }
            }
            b'C' => {
                // Right
                let cur_line_len = self.lines.get(self.cursor_row).map(|l| l.len()).unwrap_or(0);
                if self.cursor_col < cur_line_len && self.cursor_col + 1 < SCREEN_COLS {
                    self.cursor_col += 1;
                }
            }
            b'D' => {
                // Left
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                }
            }
            _ => {}
        }
    }
}

#[no_mangle]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    let args = unsafe { extract_args(argc, argv) };
    let path = if args.len() > 1 {
        args[1]
    } else {
        "[No Name]"
    };

    let mut editor = Editor::new(path);
    if args.len() > 1 {
        editor.load_file(path);
    } else {
        editor.lines.push(String::new());
    }

    // Clear terminal screen and render initial view
    print!("\x1b[2J");
    editor.render();

    // Event loop
    let mut input_buf = [0u8; 1];
    loop {
        match read(0, &mut input_buf) {
            Ok(1) => {
                let b = input_buf[0];
                // Clean exit on Ctrl+Q (0x11) or 'q'
                if b == 0x11 || b == b'q' {
                    print!("\x1b[2J\x1b[H\x1b[?25h");
                    exit(0);
                }

                // ANSI Escape sequence for arrow keys (\x1b[A .. \x1b[D)
                if b == 0x1B {
                    let mut seq = [0u8; 2];
                    if read(0, &mut seq[..1]).is_ok() && seq[0] == b'[' {
                        if read(0, &mut seq[1..2]).is_ok() {
                            editor.move_cursor(seq[1]);
                        }
                    }
                } else if b == b'j' {
                    // Vi-style scroll down
                    editor.move_cursor(b'B');
                } else if b == b'k' {
                    // Vi-style scroll up
                    editor.move_cursor(b'A');
                }

                editor.render();
            }
            Ok(_) => {}
            Err(_) => {
                print!("\x1b[2J\x1b[H\x1b[?25h");
                exit(1);
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print!("\x1b[2J\x1b[H\x1b[?25h");
    exit(1);
}
