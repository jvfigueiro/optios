// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: UEFI GOP Linear Framebuffer Video Driver with Double Buffering
// Date: 2026-11-10

//! UEFI GOP Framebuffer video driver with RAM Double Buffering.
//! Provides pixel, rectangle, border, bitmap string rendering, and tear-free display presentation.

use super::font::{get_glyph, FONT_WIDTH};
use crate::mm::{pmm, vmm};
use core::sync::atomic::{AtomicBool, Ordering};
use limine::framebuffer::Framebuffer;

pub const BACKBUFFER_VIRT_BASE: u64 = 0xFFFF_D800_0000_0000;

/// Framebuffer device controller supporting RAM backbuffering.
pub struct FramebufferDriver {
    pub hw_addr: *mut u8,
    pub backbuffer: *mut u8,
    pub width: usize,
    pub height: usize,
    pub pitch: usize,
    pub bpp: usize,
    pub double_buffered: bool,
}

unsafe impl Send for FramebufferDriver {}
unsafe impl Sync for FramebufferDriver {}

impl FramebufferDriver {
    /// Creates a driver instance from Limine Framebuffer data.
    pub fn from_limine(fb: &Framebuffer) -> Self {
        let addr = fb.addr();
        Self {
            hw_addr: addr,
            backbuffer: addr,
            width: fb.width() as usize,
            height: fb.height() as usize,
            pitch: fb.pitch() as usize,
            bpp: fb.bpp() as usize,
            double_buffered: false,
        }
    }

    /// Allocates and maps a contiguous virtual RAM backbuffer backed by physical PMM frames.
    pub fn enable_double_buffering(&mut self) -> Result<(), &'static str> {
        if self.double_buffered {
            return Ok(());
        }

        let total_bytes = self.pitch * self.height;
        let num_pages = (total_bytes + 4095) / 4096;

        for i in 0..num_pages {
            let paddr = match pmm::alloc_frame() {
                Some(p) => p,
                None => return Err("Framebuffer: Out of memory allocating backbuffer frames"),
            };
            let vaddr = BACKBUFFER_VIRT_BASE + (i as u64 * 4096);
            if let Err(e) =
                vmm::map_page(vaddr, paddr, vmm::flags::PRESENT | vmm::flags::WRITABLE)
            {
                return Err(e);
            }
        }

        unsafe {
            core::ptr::write_bytes(BACKBUFFER_VIRT_BASE as *mut u8, 0, total_bytes);
        }

        self.backbuffer = BACKBUFFER_VIRT_BASE as *mut u8;
        self.double_buffered = true;
        Ok(())
    }

    /// Flushes the RAM backbuffer to physical hardware video memory (MMIO) using 64-bit word writes.
    pub fn present(&mut self) {
        if !self.double_buffered {
            return;
        }

        let total_bytes = self.pitch * self.height;
        unsafe {
            let src = self.backbuffer as *const u64;
            let dst = self.hw_addr as *mut u64;
            let qwords = total_bytes / 8;
            for i in 0..qwords {
                core::ptr::write_volatile(dst.add(i), *src.add(i));
            }
            let remainder = total_bytes % 8;
            if remainder > 0 {
                let src_rem = self.backbuffer.add(qwords * 8);
                let dst_rem = self.hw_addr.add(qwords * 8);
                for i in 0..remainder {
                    core::ptr::write_volatile(dst_rem.add(i), *src_rem.add(i));
                }
            }
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
            let pixel_ptr = self.backbuffer.add(offset);
            *(pixel_ptr as *mut u32) = color;
        }
    }

    /// Clears the entire active buffer with a solid color.
    pub fn clear(&mut self, color: u32) {
        if self.bpp == 32 {
            let pixels = self.pitch / 4 * self.height;
            unsafe {
                let ptr = self.backbuffer as *mut u32;
                for i in 0..pixels {
                    *ptr.add(i) = color;
                }
            }
        } else {
            for y in 0..self.height {
                for x in 0..self.width {
                    self.put_pixel(x, y, color);
                }
            }
        }
    }

    /// Draws a filled rectangle.
    pub fn fill_rect(
        &mut self,
        start_x: usize,
        start_y: usize,
        width: usize,
        height: usize,
        color: u32,
    ) {
        let max_x = (start_x + width).min(self.width);
        let max_y = (start_y + height).min(self.height);

        for y in start_y..max_y {
            for x in start_x..max_x {
                self.put_pixel(x, y, color);
            }
        }
    }

    /// Draws a rectangular border with specified thickness.
    pub fn draw_rect_border(
        &mut self,
        x: usize,
        y: usize,
        w: usize,
        h: usize,
        thickness: usize,
        color: u32,
    ) {
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
    pub fn draw_char(
        &mut self,
        x: usize,
        y: usize,
        c: char,
        fg_color: u32,
        bg_color: Option<u32>,
    ) {
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
    pub fn draw_string(
        &mut self,
        mut x: usize,
        y: usize,
        text: &str,
        fg_color: u32,
        bg_color: Option<u32>,
    ) {
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

    /// Scrolls the backbuffer up by a number of text lines (each 16 pixels high).
    pub fn scroll_up(&mut self, text_lines: usize, bg_color: u32) {
        let shift_pixels = text_lines * 16;
        if shift_pixels >= self.height {
            self.clear(bg_color);
            return;
        }

        let shift_bytes = shift_pixels * self.pitch;
        let total_bytes = self.height * self.pitch;
        let copy_bytes = total_bytes - shift_bytes;

        unsafe {
            core::ptr::copy(
                self.backbuffer.add(shift_bytes),
                self.backbuffer,
                copy_bytes,
            );

            // Blank the revealed bottom rows
            let bottom_start_y = self.height - shift_pixels;
            for y in bottom_start_y..self.height {
                let row_ptr = self.backbuffer.add(y * self.pitch) as *mut u32;
                let row_words = self.pitch / 4;
                for x in 0..row_words {
                    *row_ptr.add(x) = bg_color;
                }
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
    while FB_LOCK
        .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
        .is_err()
    {
        core::hint::spin_loop();
    }

    unsafe {
        if let Some(ref mut fb) = GLOBAL_FB {
            f(fb);
        }
    }

    FB_LOCK.store(false, Ordering::Release);
}
