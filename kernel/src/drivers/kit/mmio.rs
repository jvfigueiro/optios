// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Memory-Mapped I/O (MMIO) Region Safe Abstraction
// Date: 2026-11-10

//! Safe Memory-Mapped I/O (MMIO) region wrapper ensuring bounds checking
//! and volatile read/write semantics.

use core::ptr::{read_volatile, write_volatile};

/// Safe wrapper around a memory-mapped I/O register window.
/// Enforces range boundary checks on all reads and writes, and uses
/// volatile semantics to guarantee ordering and avoid compiler elision.
#[derive(Debug, Clone, Copy)]
pub struct MmioRegion {
    base: u64,
    size: usize,
}

#[allow(dead_code)]
impl MmioRegion {
    /// Creates a new MMIO region for the given virtual base address and size.
    ///
    /// # Safety
    /// The caller must ensure that `base..base + size` is mapped to valid, uncached MMIO space.
    pub const unsafe fn new(base: u64, size: usize) -> Self {
        Self { base, size }
    }

    /// Returns the virtual base address of the MMIO region.
    #[inline]
    pub const fn base(&self) -> u64 {
        self.base
    }

    /// Returns the size in bytes of the MMIO region.
    #[inline]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// Creates a bounded sub-region within this MMIO window.
    pub fn sub_region(&self, offset: usize, sub_size: usize) -> Self {
        assert!(
            offset + sub_size <= self.size,
            "MmioRegion: sub-region out of bounds (offset=0x{:x}, sub_size=0x{:x}, size=0x{:x})",
            offset, sub_size, self.size
        );
        unsafe { Self::new(self.base + offset as u64, sub_size) }
    }

    /// Reads an 8-bit unsigned integer from the specified byte offset.
    #[inline]
    pub fn read_u8(&self, offset: usize) -> u8 {
        assert!(offset + 1 <= self.size, "MmioRegion::read_u8 out of bounds");
        unsafe { read_volatile((self.base + offset as u64) as *const u8) }
    }

    /// Writes an 8-bit unsigned integer to the specified byte offset.
    #[inline]
    pub fn write_u8(&self, offset: usize, val: u8) {
        assert!(offset + 1 <= self.size, "MmioRegion::write_u8 out of bounds");
        unsafe { write_volatile((self.base + offset as u64) as *mut u8, val) }
    }

    /// Reads a 16-bit unsigned integer from the specified aligned byte offset.
    #[inline]
    pub fn read_u16(&self, offset: usize) -> u16 {
        assert!(offset + 2 <= self.size, "MmioRegion::read_u16 out of bounds");
        assert!(offset % 2 == 0, "MmioRegion::read_u16 unaligned offset 0x{:x}", offset);
        unsafe { read_volatile((self.base + offset as u64) as *const u16) }
    }

    /// Writes a 16-bit unsigned integer to the specified aligned byte offset.
    #[inline]
    pub fn write_u16(&self, offset: usize, val: u16) {
        assert!(offset + 2 <= self.size, "MmioRegion::write_u16 out of bounds");
        assert!(offset % 2 == 0, "MmioRegion::write_u16 unaligned offset 0x{:x}", offset);
        unsafe { write_volatile((self.base + offset as u64) as *mut u16, val) }
    }

    /// Reads a 32-bit unsigned integer from the specified aligned byte offset.
    #[inline]
    pub fn read_u32(&self, offset: usize) -> u32 {
        assert!(
            offset + 4 <= self.size,
            "MmioRegion::read_u32 out of bounds (offset=0x{:x}, size=0x{:x})",
            offset, self.size
        );
        assert!(offset % 4 == 0, "MmioRegion::read_u32 unaligned offset 0x{:x}", offset);
        unsafe { read_volatile((self.base + offset as u64) as *const u32) }
    }

    /// Writes a 32-bit unsigned integer to the specified aligned byte offset.
    #[inline]
    pub fn write_u32(&self, offset: usize, val: u32) {
        assert!(
            offset + 4 <= self.size,
            "MmioRegion::write_u32 out of bounds (offset=0x{:x}, size=0x{:x})",
            offset, self.size
        );
        assert!(offset % 4 == 0, "MmioRegion::write_u32 unaligned offset 0x{:x}", offset);
        unsafe { write_volatile((self.base + offset as u64) as *mut u32, val) }
    }

    /// Reads a 64-bit unsigned integer from the specified aligned byte offset.
    #[inline]
    pub fn read_u64(&self, offset: usize) -> u64 {
        assert!(offset + 8 <= self.size, "MmioRegion::read_u64 out of bounds");
        assert!(offset % 8 == 0, "MmioRegion::read_u64 unaligned offset 0x{:x}", offset);
        unsafe { read_volatile((self.base + offset as u64) as *const u64) }
    }

    /// Writes a 64-bit unsigned integer to the specified aligned byte offset.
    #[inline]
    pub fn write_u64(&self, offset: usize, val: u64) {
        assert!(offset + 8 <= self.size, "MmioRegion::write_u64 out of bounds");
        assert!(offset % 8 == 0, "MmioRegion::write_u64 unaligned offset 0x{:x}", offset);
        unsafe { write_volatile((self.base + offset as u64) as *mut u64, val) }
    }
}
