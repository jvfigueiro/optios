// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: UEFI GOP Linear Framebuffer Video Driver
// Date: 2026-11-09

//! UEFI GOP Framebuffer video driver.
//! Provides pixel, rectangle, border, and formatted bitmap string rendering.

use core::sync::atomic::{AtomicBool, Ordering};
use limine::framebuffer::Framebuffer;
use super::font::{get_glyph, FONT_WIDTH};

/// Framebuffer device controller.
pub struct FramebufferDriver {
    addr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: usize,
}

unsafe impl Send for FramebufferDriver {}
unsafe impl Sync for FramebufferDriver {}

impl FramebufferDriver {
    /// Creates a driver instance from Limine Framebuffer data.
    pub fn from_limine(fb: &Framebuffer) -> Self {
        Self {
            addr: fb.addr(),
            width: fb.width() as usize,
            height: fb.height() as usize,
            pitch: fb.pitch() as usize,
            bpp: fb.bpp() as usize,
        }
    }

    /// Renders a single pixel at (x, y) with a 32-bit RGB color (0x00RRGGBB).
    #[inline]
    pub fn put_pixel(&mut self, x: usize, y: usize, color: u32) {
        if x >= self.width || y >= self.height {
            return;
        }

        let bytes_per_pixel = self.bpp / 8;
        let offset = y * self.pitch + x * bytes_per_pixel;

        unsafe {
            let pixel_ptr = self.addr.add(offset);
            *(pixel_ptr as *mut u32) = color;
        }
    }

    /// Clears the entire display buffer with a solid color.
    pub fn clear(&mut self, color: u32) {
        for y in 0..self.height {
            for x in 0..self.width {
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Draws a filled rectangle.
    pub fn fill_rect(&mut self, start_x: usize, start_y: usize, width: usize, height: usize, color: u32) {
        let max_x = (start_x + width).min(self.width);
        let max_y = (start_y + height).min(self.height);

        for y in start_y..max_y {
            for x in start_x..max_x {
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Draws a rectangular border with specified thickness.
    pub fn draw_rect_border(&mut self, x: usize, y: usize, w: usize, h: usize, thickness: usize, color: u32) {
        self.fill_rect(x, y, w, thickness, color);
        if h > thickness {
            self.fill_rect(x, y + h - thickness, w, thickness, color);
        }
        self.fill_rect(x, y, thickness, h, color);
        if w > thickness {
            self.fill_rect(x + w - thickness, y, thickness, h, color);
        }
    }

    /// Draws an 8x16 ASCII glyph at (x, y).
    pub fn draw_char(&mut self, x: usize, y: usize, c: char, fg_color: u32, bg_color: Option<u32>) {
        let glyph = get_glyph(c);

        for (row_idx, &row_byte) in glyph.iter().enumerate() {
            let py = y + row_idx;
            if py >= self.height {
                break;
            }

            for col_idx in 0..FONT_WIDTH {
                let px = x + col_idx;
                if px >= self.width {
                    break;
                }

                let is_set = ((row_byte >> (7 - col_idx)) & 1) != 0;
                if is_set {
                    self.put_pixel(px, py, fg_color);
                } else if let Some(bg) = bg_color {
                    self.put_pixel(px, py, bg);
                }
            }
        }
    }

    /// Draws an ASCII string starting at (x, y).
    pub fn draw_string(&mut self, mut x: usize, y: usize, text: &str, fg_color: u32, bg_color: Option<u32>) {
        for c in text.chars() {
            if c == '\n' {
                break;
            }
            self.draw_char(x, y, c, fg_color, bg_color);
            x += FONT_WIDTH;
            if x + FONT_WIDTH > self.width {
                break;
            }
        }
    }
}

// Global emergency framebuffer state for panic and exception dumps
static mut GLOBAL_FB: Option<FramebufferDriver> = None;
static FB_LOCK: AtomicBool = AtomicBool::new(false);

/// Initializes the global emergency framebuffer reference.
pub fn init_global_framebuffer(fb: &Framebuffer) {
    unsafe {
        GLOBAL_FB = Some(FramebufferDriver::from_limine(fb));
    }
}

/// Executes a closure with mutable access to the global framebuffer.
pub fn with_framebuffer<F: FnOnce(&mut FramebufferDriver)>(f: F) {
    while FB_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        if let Some(ref mut fb) = GLOBAL_FB {
            f(fb);
        }
    }

    FB_LOCK.store(false, Ordering::Release);
}
