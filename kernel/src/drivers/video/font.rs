// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Embedded 8x16 Bitmap Font Renderer
// Date: 2026-11-09

//! Embedded 8x16 bitmap font (.rodata) for Framebuffer rendering.

#[allow(dead_code)]
pub const FONT_WIDTH: usize = 8;
#[allow(dead_code)]
pub const FONT_HEIGHT: usize = 16;

/// Default replacement glyph (box boundary with question mark / solid block).
pub static GLYPH_UNKNOWN: [u8; 16] = [
    0b01111110,
    0b10000001,
    0b10111101,
    0b10100101,
    0b10100101,
    0b10111101,
    0b10000001,
    0b10011001,
    0b10011001,
    0b10000001,
    0b10011001,
    0b10011001,
    0b10000001,
    0b10111101,
    0b10000001,
    0b01111110,
];

/// Empty space glyph.
#[allow(dead_code)]
pub static GLYPH_SPACE: [u8; 16] = [0; 16];

// Fundamental ASCII glyph table (32..=126).
include!("font_data.in");

/// Returns the 8x16 bitmap pattern for an ASCII character.
pub fn get_glyph(c: char) -> &'static [u8; 16] {
    let b = c as u32;
    if b >= 32 && b <= 126 {
        &ASCII_GLYPHS[(b - 32) as usize]
    } else {
        &GLYPH_UNKNOWN
    }
}
