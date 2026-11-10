// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Physical Memory Manager (Bitmap Frame Allocator)
// Date: 2026-11-10

//! Physical Memory Manager (PMM) based on a Bitmap Frame Allocator.
//! Manages 4 KiB physical page frames across usable memory regions.

use core::sync::atomic::{AtomicBool, Ordering};
use limine::memory_map::{Entry, EntryType};
use crate::{info, warn};

pub const PAGE_SIZE: u64 = 4096;

/// 1 MiB lower memory boundary (frames 0..256).
/// Permanently reserved for real mode tables, BDA, EBDA, and SMP AP trampolines.
pub const LOW_MEMORY_RESERVED_LIMIT: u64 = 0x100000;
pub const LOW_MEMORY_RESERVED_FRAMES: usize = (LOW_MEMORY_RESERVED_LIMIT / PAGE_SIZE) as usize; // 256 frames

/// Statistics of the Physical Memory Manager.
#[derive(Debug, Clone, Copy, Default)]
pub struct PmmStats {
    pub total_frames: usize,
    pub free_frames: usize,
    #[allow(dead_code)]
    pub used_frames: usize,
    pub bitmap_phys_addr: u64,
    pub bitmap_size_bytes: usize,
}

impl PmmStats {
    pub fn free_memory_mib(&self) -> u64 {
        (self.free_frames as u64 * PAGE_SIZE) / (1024 * 1024)
    }

    #[allow(dead_code)]
    pub fn total_memory_mib(&self) -> u64 {
        (self.total_frames as u64 * PAGE_SIZE) / (1024 * 1024)
    }
}

pub struct PhysicalMemoryManager {
    bitmap: *mut u8,
    total_frames: usize,
    free_frames: usize,
    bitmap_phys_addr: u64,
    bitmap_size_bytes: usize,
    last_scanned_index: usize,
}

unsafe impl Send for PhysicalMemoryManager {}
unsafe impl Sync for PhysicalMemoryManager {}

static mut PMM: PhysicalMemoryManager = PhysicalMemoryManager {
    bitmap: core::ptr::null_mut(),
    total_frames: 0,
    free_frames: 0,
    bitmap_phys_addr: 0,
    bitmap_size_bytes: 0,
    last_scanned_index: LOW_MEMORY_RESERVED_FRAMES,
};

static PMM_LOCK: AtomicBool = AtomicBool::new(false);

impl PhysicalMemoryManager {
    #[inline]
    fn set_bit(&mut self, frame: usize) {
        let byte_idx = frame / 8;
        let bit_idx = frame % 8;
        unsafe {
            let ptr = self.bitmap.add(byte_idx);
            *ptr |= 1 << bit_idx;
        }
    }

    #[inline]
    fn clear_bit(&mut self, frame: usize) {
        let byte_idx = frame / 8;
        let bit_idx = frame % 8;
        unsafe {
            let ptr = self.bitmap.add(byte_idx);
            *ptr &= !(1 << bit_idx);
        }
    }

    #[inline]
    fn test_bit(&self, frame: usize) -> bool {
        let byte_idx = frame / 8;
        let bit_idx = frame % 8;
        unsafe {
            let byte = *self.bitmap.add(byte_idx);
            ((byte >> bit_idx) & 1) != 0
        }
    }

    /// Allocates a single 4 KiB physical page frame above the 1 MiB reserved threshold.
    pub fn alloc_frame(&mut self) -> Option<u64> {
        let start = self.last_scanned_index.max(LOW_MEMORY_RESERVED_FRAMES);
        for i in 0..self.total_frames {
            let idx = (start + i) % self.total_frames;
            if idx < LOW_MEMORY_RESERVED_FRAMES {
                continue;
            }
            if !self.test_bit(idx) {
                self.set_bit(idx);
                self.free_frames = self.free_frames.saturating_sub(1);
                self.last_scanned_index = idx + 1;
                return Some(idx as u64 * PAGE_SIZE);
            }
        }
        None
    }

    /// Frees a previously allocated 4 KiB physical page frame.
    #[allow(dead_code)]
    pub fn free_frame(&mut self, phys_addr: u64) {
        let frame = (phys_addr / PAGE_SIZE) as usize;
        if frame >= LOW_MEMORY_RESERVED_FRAMES && frame < self.total_frames && self.test_bit(frame) {
            self.clear_bit(frame);
            self.free_frames += 1;
            if frame < self.last_scanned_index {
                self.last_scanned_index = frame;
            }
        }
    }

    /// Returns current PMM statistics.
    pub fn stats(&self) -> PmmStats {
        PmmStats {
            total_frames: self.total_frames,
            free_frames: self.free_frames,
            used_frames: self.total_frames.saturating_sub(self.free_frames),
            bitmap_phys_addr: self.bitmap_phys_addr,
            bitmap_size_bytes: self.bitmap_size_bytes,
        }
    }
}

/// Initializes the Physical Memory Manager using the Limine memory map and HHDM offset.
pub fn init(entries: &[&Entry], hhdm_offset: u64) {
    // 1. Calculate highest physical address across all physical memory regions
    // This correctly dimensions the bitmap to span memory holes (e.g., PCI apertures)
    let mut highest_addr: u64 = 0;
    for entry in entries {
        match entry.entry_type {
            EntryType::USABLE
            | EntryType::BOOTLOADER_RECLAIMABLE
            | EntryType::EXECUTABLE_AND_MODULES
            | EntryType::ACPI_RECLAIMABLE
            | EntryType::ACPI_NVS => {
                let end = entry.base + entry.length;
                if end > highest_addr {
                    highest_addr = end;
                }
            }
            _ => {}
        }
    }

    let total_frames = ((highest_addr + PAGE_SIZE - 1) / PAGE_SIZE) as usize;
    let bitmap_size = (total_frames + 7) / 8;
    // Align bitmap size to 4096-byte boundaries
    let bitmap_size_aligned = ((bitmap_size + (PAGE_SIZE as usize - 1)) / PAGE_SIZE as usize) * PAGE_SIZE as usize;

    // 2. Find a usable region sufficiently large to hold the bitmap strictly above 1 MiB
    let mut chosen_region: Option<(u64, u64)> = None;
    for entry in entries {
        if entry.entry_type == EntryType::USABLE
            && entry.base >= LOW_MEMORY_RESERVED_LIMIT
            && entry.length >= bitmap_size_aligned as u64
        {
            chosen_region = Some((entry.base, entry.length));
            break;
        }
    }

    let (bitmap_phys, _) = chosen_region.expect("PMM: Failed to find memory region >= 1 MiB for frame allocation bitmap!");
    let bitmap_virt = (bitmap_phys + hhdm_offset) as *mut u8;

    unsafe {
        // Initially mark every frame as 1 (reserved / occupied)
        core::ptr::write_bytes(bitmap_virt, 0xFF, bitmap_size_aligned);

        let pmm_raw = &raw mut PMM;
        *pmm_raw = PhysicalMemoryManager {
            bitmap: bitmap_virt,
            total_frames,
            free_frames: 0,
            bitmap_phys_addr: bitmap_phys,
            bitmap_size_bytes: bitmap_size_aligned,
            last_scanned_index: LOW_MEMORY_RESERVED_FRAMES,
        };

        // 3. Mark all usable regions as free (0).
        // INVARIANT: The initramfs archive (loaded by Limine as a boot module) and the kernel
        // itself are mapped in memory regions classified by Limine as EntryType::EXECUTABLE_AND_MODULES.
        // Because the PMM initializes the entire bitmap to 1s (all frames reserved) and ONLY clears
        // bits for regions of type EntryType::USABLE, all boot modules (including initramfs) remain
        // non-allocatable by the PMM by design throughout kernel execution.
        for entry in entries {
            if entry.entry_type == EntryType::USABLE {
                let start_frame = (entry.base / PAGE_SIZE) as usize;
                let frame_count = (entry.length / PAGE_SIZE) as usize;
                for f in 0..frame_count {
                    let frame = start_frame + f;
                    if frame < total_frames {
                        (*pmm_raw).clear_bit(frame);
                        (*pmm_raw).free_frames += 1;
                    }
                }
            }
        }

        // 4. Force reservation of lower 1 MiB (frames 0..256) permanently
        for f in 0..LOW_MEMORY_RESERVED_FRAMES.min(total_frames) {
            if !(*pmm_raw).test_bit(f) {
                (*pmm_raw).set_bit(f);
                (*pmm_raw).free_frames = (*pmm_raw).free_frames.saturating_sub(1);
            }
        }

        // 5. Reserve frames occupied by the bitmap itself
        let bitmap_start_frame = (bitmap_phys / PAGE_SIZE) as usize;
        let bitmap_frames = bitmap_size_aligned / PAGE_SIZE as usize;
        for f in 0..bitmap_frames {
            let frame = bitmap_start_frame + f;
            if frame < total_frames && !(*pmm_raw).test_bit(frame) {
                (*pmm_raw).set_bit(frame);
                (*pmm_raw).free_frames = (*pmm_raw).free_frames.saturating_sub(1);
            }
        }

        // Position scanning pointer right after the bitmap
        (*pmm_raw).last_scanned_index = bitmap_start_frame + bitmap_frames;
    }

    let stats = get_stats();
    info!(
        "PMM initialized: {} total frames ({} free frames, {} MiB usable).",
        stats.total_frames,
        stats.free_frames,
        stats.free_memory_mib()
    );
    info!(
        "PMM Bitmap: Located at physical 0x{:016x} (size: {} KiB).",
        stats.bitmap_phys_addr,
        stats.bitmap_size_bytes / 1024
    );
    info!(
        "PMM Security: Lower 1 MiB (first 256 frames) permanently reserved."
    );
}

/// Allocates a single 4 KiB physical page frame with synchronization.
pub fn alloc_frame() -> Option<u64> {
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let pmm_raw = &raw mut PMM;
    let frame = unsafe { (*pmm_raw).alloc_frame() };
    PMM_LOCK.store(false, Ordering::Release);
    frame
}

/// Frees a 4 KiB physical page frame with synchronization.
#[allow(dead_code)]
pub fn free_frame(phys_addr: u64) {
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let pmm_raw = &raw mut PMM;
    unsafe { (*pmm_raw).free_frame(phys_addr) };
    PMM_LOCK.store(false, Ordering::Release);
}

/// Returns true if the frame containing `phys_addr` is currently marked as reserved (bit = 1).
#[allow(dead_code)]
pub fn is_frame_reserved(phys_addr: u64) -> bool {
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let pmm_raw = &raw const PMM;
    let frame = (phys_addr / PAGE_SIZE) as usize;
    let is_res = unsafe {
        if frame < (*pmm_raw).total_frames {
            (*pmm_raw).test_bit(frame)
        } else {
            true
        }
    };
    PMM_LOCK.store(false, Ordering::Release);
    is_res
}

/// Retrieves current PMM statistics with synchronization.
pub fn get_stats() -> PmmStats {
    while PMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let pmm_raw = &raw const PMM;
    let stats = unsafe { (*pmm_raw).stats() };
    PMM_LOCK.store(false, Ordering::Release);
    stats
}

/// Performs a diagnostic allocation test of 3 consecutive frames.
#[allow(dead_code)]
pub fn test_frame_allocation() -> bool {
    let f1 = alloc_frame();
    let f2 = alloc_frame();
    let f3 = alloc_frame();

    let success = match (f1, f2, f3) {
        (Some(a1), Some(a2), Some(a3)) => {
            info!("PMM Test: Successfully allocated 3 test frames:");
            info!("          Frame 1: 0x{:016x}", a1);
            info!("          Frame 2: 0x{:016x}", a2);
            info!("          Frame 3: 0x{:016x}", a3);

            // Assert that none of the allocated frames are in the reserved lower 1 MiB
            if a1 < LOW_MEMORY_RESERVED_LIMIT || a2 < LOW_MEMORY_RESERVED_LIMIT || a3 < LOW_MEMORY_RESERVED_LIMIT {
                warn!("PMM Test FAILED: Allocated frame inside reserved lower 1 MiB!");
                return false;
            }

            // Free the test frames
            free_frame(a1);
            free_frame(a2);
            free_frame(a3);
            info!("PMM Test: 3 test frames freed cleanly.");
            true
        }
        _ => {
            warn!("PMM Test: Failed to allocate 3 test frames!");
            false
        }
    };

    success
}
