// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Pure ANSI Escape Sequence Parser State Machine
// Date: 2026-11-10

//! Pure `#![no_std]` ANSI/VT100 escape sequence parser for OptiOS.
//!
//! Provides a zero-allocation state machine that parses byte streams and
//! yields structured [`AnsiAction`] events for terminal emulators.

#![no_std]

#[cfg(test)]
mod tests;

/// Color definition for terminal text and background.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiColor {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    Default,
}

impl AnsiColor {
    /// Maps the color to a standard 32-bit `0x00RRGGBB` RGB value.
    pub const fn to_rgb32(self, is_bg: bool) -> u32 {
        match self {
            Self::Black => 0x00000000,
            Self::Red => 0x00CC3333,
            Self::Green => 0x0033CC33,
            Self::Yellow => 0x00CCCC33,
            Self::Blue => 0x003366CC,
            Self::Magenta => 0x00CC33CC,
            Self::Cyan => 0x0033CCCC,
            Self::White => 0x00CCCCCC,
            Self::BrightBlack => 0x00666666,
            Self::BrightRed => 0x00FF5555,
            Self::BrightGreen => 0x0055FF55,
            Self::BrightYellow => 0x00FFFF55,
            Self::BrightBlue => 0x005599FF,
            Self::BrightMagenta => 0x00FF55FF,
            Self::BrightCyan => 0x0055FFFF,
            Self::BrightWhite => 0x00FFFFFF,
            Self::Default => {
                if is_bg {
                    0x00000000 // Default background: pure black
                } else {
                    0x00E0E0E0 // Default foreground: light gray
                }
            }
        }
    }
}

/// Actions emitted by the ANSI parser to drive the console video driver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnsiAction {
    /// Render a single printable ASCII or extended byte.
    Print(u8),
    /// Set text foreground color.
    SetFg(AnsiColor),
    /// Set text background color.
    SetBg(AnsiColor),
    /// Toggle bold / bright text style.
    SetBold(bool),
    /// Reset all styling, colors, and attributes to default.
    ResetAttributes,
    /// Clear the entire video console and reset cursor.
    ClearScreen,
    /// Clear from current cursor position to end of line.
    ClearLine,
    /// Move cursor to 0-based `(row, col)` position.
    SetCursorPosition { row: usize, col: usize },
}

/// Internal parser states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    Ground,
    Escape,
    Csi,
}

/// Maximum number of parameters in a single CSI sequence (e.g. `\x1b[1;36;40m`).
const MAX_PARAMS: usize = 8;

/// Zero-allocation, stream-oriented ANSI sequence parser.
pub struct AnsiParser {
    state: State,
    params: [u16; MAX_PARAMS],
    param_count: usize,
    current_param: u16,
    has_param: bool,
}

impl AnsiParser {
    /// Creates a new ANSI parser in the initial ground state.
    pub const fn new() -> Self {
        Self {
            state: State::Ground,
            params: [0; MAX_PARAMS],
            param_count: 0,
            current_param: 0,
            has_param: false,
        }
    }

    /// Resets parser state back to ground.
    pub fn reset(&mut self) {
        self.state = State::Ground;
        self.param_count = 0;
        self.current_param = 0;
        self.has_param = false;
    }

    /// Consumes a single input byte and executes a callback for any actions produced.
    pub fn parse_byte<F>(&mut self, byte: u8, mut emit: F)
    where
        F: FnMut(AnsiAction),
    {
        match self.state {
            State::Ground => {
                if byte == 0x1B {
                    self.state = State::Escape;
                } else {
                    emit(AnsiAction::Print(byte));
                }
            }
            State::Escape => {
                if byte == b'[' {
                    self.state = State::Csi;
                    self.param_count = 0;
                    self.current_param = 0;
                    self.has_param = false;
                } else {
                    // Malformed or non-CSI escape sequence: return to ground
                    self.state = State::Ground;
                    if byte != 0x1B {
                        emit(AnsiAction::Print(byte));
                    }
                }
            }
            State::Csi => match byte {
                b'0'..=b'9' => {
                    self.has_param = true;
                    self.current_param = self
                        .current_param
                        .saturating_mul(10)
                        .saturating_add((byte - b'0') as u16);
                }
                b';' => {
                    if self.param_count < MAX_PARAMS {
                        self.params[self.param_count] = self.current_param;
                        self.param_count += 1;
                    }
                    self.current_param = 0;
                    self.has_param = true;
                }
                b'?' | b' ' | b'!' | b'"' | b'#' | b'$' | b'%' | b'&' | b'\'' | b'(' | b')'
                | b'*' | b'+' | b',' | b'-' | b'.' | b'/' | b':' | b'<' | b'=' | b'>' => {
                    // Intermediate / private parameter characters (e.g. \x1b[?25h): consume and stay in CSI
                }
                b'm' => {
                    // Finalize last parameter
                    if self.has_param && self.param_count < MAX_PARAMS {
                        self.params[self.param_count] = self.current_param;
                        self.param_count += 1;
                    }

                    if self.param_count == 0 {
                        emit(AnsiAction::ResetAttributes);
                    } else {
                        for i in 0..self.param_count {
                            match self.params[i] {
                                0 => emit(AnsiAction::ResetAttributes),
                                1 => emit(AnsiAction::SetBold(true)),
                                22 => emit(AnsiAction::SetBold(false)),
                                30 => emit(AnsiAction::SetFg(AnsiColor::Black)),
                                31 => emit(AnsiAction::SetFg(AnsiColor::Red)),
                                32 => emit(AnsiAction::SetFg(AnsiColor::Green)),
                                33 => emit(AnsiAction::SetFg(AnsiColor::Yellow)),
                                34 => emit(AnsiAction::SetFg(AnsiColor::Blue)),
                                35 => emit(AnsiAction::SetFg(AnsiColor::Magenta)),
                                36 => emit(AnsiAction::SetFg(AnsiColor::Cyan)),
                                37 => emit(AnsiAction::SetFg(AnsiColor::White)),
                                39 => emit(AnsiAction::SetFg(AnsiColor::Default)),
                                40 => emit(AnsiAction::SetBg(AnsiColor::Black)),
                                41 => emit(AnsiAction::SetBg(AnsiColor::Red)),
                                42 => emit(AnsiAction::SetBg(AnsiColor::Green)),
                                43 => emit(AnsiAction::SetBg(AnsiColor::Yellow)),
                                44 => emit(AnsiAction::SetBg(AnsiColor::Blue)),
                                45 => emit(AnsiAction::SetBg(AnsiColor::Magenta)),
                                46 => emit(AnsiAction::SetBg(AnsiColor::Cyan)),
                                47 => emit(AnsiAction::SetBg(AnsiColor::White)),
                                49 => emit(AnsiAction::SetBg(AnsiColor::Default)),
                                90 => emit(AnsiAction::SetFg(AnsiColor::BrightBlack)),
                                91 => emit(AnsiAction::SetFg(AnsiColor::BrightRed)),
                                92 => emit(AnsiAction::SetFg(AnsiColor::BrightGreen)),
                                93 => emit(AnsiAction::SetFg(AnsiColor::BrightYellow)),
                                94 => emit(AnsiAction::SetFg(AnsiColor::BrightBlue)),
                                95 => emit(AnsiAction::SetFg(AnsiColor::BrightMagenta)),
                                96 => emit(AnsiAction::SetFg(AnsiColor::BrightCyan)),
                                97 => emit(AnsiAction::SetFg(AnsiColor::BrightWhite)),
                                100 => emit(AnsiAction::SetBg(AnsiColor::BrightBlack)),
                                101 => emit(AnsiAction::SetBg(AnsiColor::BrightRed)),
                                102 => emit(AnsiAction::SetBg(AnsiColor::BrightGreen)),
                                103 => emit(AnsiAction::SetBg(AnsiColor::BrightYellow)),
                                104 => emit(AnsiAction::SetBg(AnsiColor::BrightBlue)),
                                105 => emit(AnsiAction::SetBg(AnsiColor::BrightMagenta)),
                                106 => emit(AnsiAction::SetBg(AnsiColor::BrightCyan)),
                                107 => emit(AnsiAction::SetBg(AnsiColor::BrightWhite)),
                                _ => {} // Ignore unknown SGR codes
                            }
                        }
                    }
                    self.reset();
                }
                b'J' => {
                    // 2J: Clear screen
                    if self.current_param == 2 || (self.param_count == 1 && self.params[0] == 2) {
                        emit(AnsiAction::ClearScreen);
                    }
                    self.reset();
                }
                b'K' => {
                    // Clear line
                    emit(AnsiAction::ClearLine);
                    self.reset();
                }
                b'H' | b'f' => {
                    // Cursor positioning: \x1b[r;cH (1-based, default 1,1)
                    if self.has_param && self.param_count < MAX_PARAMS {
                        self.params[self.param_count] = self.current_param;
                        self.param_count += 1;
                    }
                    let r = if self.param_count > 0 && self.params[0] > 0 {
                        (self.params[0] - 1) as usize
                    } else {
                        0
                    };
                    let c = if self.param_count > 1 && self.params[1] > 0 {
                        (self.params[1] - 1) as usize
                    } else {
                        0
                    };
                    emit(AnsiAction::SetCursorPosition { row: r, col: c });
                    self.reset();
                }
                _ => {
                    // Unknown or unsupported CSI terminator: discard and return to ground
                    self.reset();
                }
            },
        }
    }
}
