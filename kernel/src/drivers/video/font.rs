// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Embedded 8x16 Bitmap Font Renderer
// Date: 2026-11-10

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

/// Glyph for 'ã' (lowercase a with tilde).
pub static GLYPH_A_TILDE: [u8; 16] = [
    0b00000000,
    0b00110100,
    0b01001000,
    0b00000000,
    0b00000000,
    0b00111100,
    0b00000010,
    0b00111110,
    0b01000010,
    0b01000110,
    0b00111010,
    0b00000000,
    0b00000000,
    0b00000000,
    0b00000000,
    0b00000000,
];

/// Glyph for 'ó' (lowercase o with acute accent).
pub static GLYPH_O_ACUTE: [u8; 16] = [
    0b00000000,
    0b00001000,
    0b00010000,
    0b00000000,
    0b00000000,
    0b00111100,
    0b01000010,
    0b01000010,
    0b01000010,
    0b01000010,
    0b00111100,
    0b00000000,
    0b00000000,
    0b00000000,
    0b00000000,
    0b00000000,
];

/// Returns the 8x16 bitmap pattern for an ASCII/extended character.
pub fn get_glyph(c: char) -> &'static [u8; 16] {
    match c {
        'ã' => &GLYPH_A_TILDE,
        'ó' => &GLYPH_O_ACUTE,
        _ => {
            let b = c as u32;
            if b >= 32 && b <= 126 {
                &ASCII_GLYPHS[(b - 32) as usize]
            } else {
                &GLYPH_UNKNOWN
            }
        }
    }
}
