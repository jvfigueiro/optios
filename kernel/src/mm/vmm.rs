// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Virtual Memory Manager (4-Level Paging & Uncached MMIO Mapping)
// Date: 2026-11-10

//! Virtual Memory Manager (VMM) and x86-64 4-Level Paging for OptiOS.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::mm::pmm;
use crate::{info, warn};

#[allow(dead_code)]
pub mod flags {
    pub const PRESENT: u64 = 1 << 0;
    pub const WRITABLE: u64 = 1 << 1;
    pub const USER_ACCESSIBLE: u64 = 1 << 2;
    pub const WRITE_THROUGH: u64 = 1 << 3;
    pub const CACHE_DISABLE: u64 = 1 << 4;
    pub const ACCESSED: u64 = 1 << 5;
    pub const DIRTY: u64 = 1 << 6;
    pub const HUGE_PAGE: u64 = 1 << 7;
    pub const GLOBAL: u64 = 1 << 8;
    pub const NO_EXECUTE: u64 = 1 << 63;
}

const ENTRY_PHYS_MASK: u64 = 0x000F_FFFF_FFFF_F000;

/// Canonical higher-half virtual base dedicated to uncached hardware MMIO mappings.
pub const MMIO_VIRT_BASE: u64 = 0xFFFF_D000_0000_0000;

/// Represents a 4 KiB aligned page table containing 512 entries of 64 bits each.
#[repr(C, align(4096))]
pub struct PageTable {
    pub entries: [u64; 512],
}

impl PageTable {
    #[allow(dead_code)]
    pub const fn empty() -> Self {
        Self { entries: [0; 512] }
    }

    pub fn zero(&mut self) {
        self.entries = [0; 512];
    }
}

static mut HHDM_OFFSET: u64 = 0;
static VMM_LOCK: AtomicBool = AtomicBool::new(false);

/// Reads the active PML4 physical address from the CR3 register.
#[inline]
pub fn read_cr3() -> u64 {
    let cr3: u64;
    unsafe {
        core::arch::asm!("mov {}, cr3", out(reg) cr3, options(nomem, nostack, preserves_flags));
    }
    cr3 & ENTRY_PHYS_MASK
}

/// Invalidates the TLB entry for the specified virtual address.
#[inline]
pub fn invlpg(vaddr: u64) {
    unsafe {
        core::arch::asm!("invlpg [{}]", in(reg) vaddr, options(nostack, preserves_flags));
    }
}

/// Translates a physical address to a virtual pointer via the HHDM offset.
#[inline]
pub fn phys_to_virt<T>(paddr: u64) -> *mut T {
    let offset = unsafe { HHDM_OFFSET };
    (paddr + offset) as *mut T
}

/// Initializes the Virtual Memory Manager with the verified HHDM virtual offset.
pub fn init(hhdm: u64) {
    unsafe {
        HHDM_OFFSET = hhdm;
    }
    let cr3 = read_cr3();
    info!("VMM initialized: Active PML4 located at physical 0x{:016x}.", cr3);
}

/// Helper function to traverse or create the next page table level.
fn get_or_create_next_table(entry: &mut u64, is_user: bool) -> Option<*mut PageTable> {
    let user_flag = if is_user { flags::USER_ACCESSIBLE } else { 0 };
    if (*entry & flags::PRESENT) != 0 {
        *entry |= user_flag;
        let next_phys = *entry & ENTRY_PHYS_MASK;
        Some(phys_to_virt::<PageTable>(next_phys))
    } else {
        // Allocate a new physical frame from PMM for the intermediate table
        let frame = pmm::alloc_frame()?;
        let table_ptr = phys_to_virt::<PageTable>(frame);
        unsafe {
            (*table_ptr).zero();
        }
        // Intermediate tables inherit Present + Writable (+ User if requested)
        *entry = frame | flags::PRESENT | flags::WRITABLE | user_flag;
        Some(table_ptr)
    }
}

/// Pre-allocates intermediate page tables (PDPT, PD, PT) for userland address spaces
/// so that dynamic process loading does not consume kernel paging frames at runtime.
pub fn preallocate_user_tables(virt_addrs: &[u64]) {
    while VMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let pml4_phys = read_cr3();
    let pml4_virt = phys_to_virt::<PageTable>(pml4_phys);

    unsafe {
        let pml4 = &mut *pml4_virt;
        for &virt in virt_addrs {
            let pml4_idx = ((virt >> 39) & 0x1FF) as usize;
            let pdpt_idx = ((virt >> 30) & 0x1FF) as usize;
            let pd_idx = ((virt >> 21) & 0x1FF) as usize;

            if let Some(pdpt_ptr) = get_or_create_next_table(&mut pml4.entries[pml4_idx], true) {
                let pdpt = &mut *pdpt_ptr;
                if let Some(pd_ptr) = get_or_create_next_table(&mut pdpt.entries[pdpt_idx], true) {
                    let pd = &mut *pd_ptr;
                    let _ = get_or_create_next_table(&mut pd.entries[pd_idx], true);
                }
            }
        }
    }

    VMM_LOCK.store(false, Ordering::Release);
}

/// Maps a 4 KiB virtual page to a physical frame with specified flags.
pub fn map_page(virt_addr: u64, phys_addr: u64, page_flags: u64) -> Result<(), &'static str> {
    while VMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let pml4_phys = read_cr3();
    let pml4_virt = phys_to_virt::<PageTable>(pml4_phys);

    let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;

    let is_user = (page_flags & flags::USER_ACCESSIBLE) != 0;

    let res = unsafe {
        let pml4 = &mut *pml4_virt;
        let pdpt_ptr = match get_or_create_next_table(&mut pml4.entries[pml4_idx], is_user) {
            Some(p) => p,
            None => {
                VMM_LOCK.store(false, Ordering::Release);
                return Err("VMM: Out of memory allocating PDPT intermediate table");
            }
        };

        let pdpt = &mut *pdpt_ptr;
        let pd_ptr = match get_or_create_next_table(&mut pdpt.entries[pdpt_idx], is_user) {
            Some(p) => p,
            None => {
                VMM_LOCK.store(false, Ordering::Release);
                return Err("VMM: Out of memory allocating PD intermediate table");
            }
        };

        let pd = &mut *pd_ptr;
        let pt_ptr = match get_or_create_next_table(&mut pd.entries[pd_idx], is_user) {
            Some(p) => p,
            None => {
                VMM_LOCK.store(false, Ordering::Release);
                return Err("VMM: Out of memory allocating PT intermediate table");
            }
        };

        let pt = &mut *pt_ptr;
        pt.entries[pt_idx] = (phys_addr & ENTRY_PHYS_MASK) | page_flags;

        invlpg(virt_addr);
        Ok(())
    };

    VMM_LOCK.store(false, Ordering::Release);
    res
}

/// Unmaps a 4 KiB virtual page and invalidates the corresponding TLB entry.
#[allow(dead_code)]
pub fn unmap_page(virt_addr: u64) -> Result<(), &'static str> {
    while VMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let pml4_phys = read_cr3();
    let pml4_virt = phys_to_virt::<PageTable>(pml4_phys);

    let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;

    let res = unsafe {
        let pml4 = &mut *pml4_virt;
        if (pml4.entries[pml4_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return Err("VMM: Page not mapped (PML4 entry missing)");
        }

        let pdpt = &mut *phys_to_virt::<PageTable>(pml4.entries[pml4_idx] & ENTRY_PHYS_MASK);
        if (pdpt.entries[pdpt_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return Err("VMM: Page not mapped (PDPT entry missing)");
        }

        let pd = &mut *phys_to_virt::<PageTable>(pdpt.entries[pdpt_idx] & ENTRY_PHYS_MASK);
        if (pd.entries[pd_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return Err("VMM: Page not mapped (PD entry missing)");
        }

        let pt = &mut *phys_to_virt::<PageTable>(pd.entries[pd_idx] & ENTRY_PHYS_MASK);
        pt.entries[pt_idx] = 0;

        invlpg(virt_addr);
        Ok(())
    };

    VMM_LOCK.store(false, Ordering::Release);
    res
}

/// Translates a virtual address to its physical backing address.
#[allow(dead_code)]
pub fn virt_to_phys(virt_addr: u64) -> Option<u64> {
    while VMM_LOCK.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    let pml4_phys = read_cr3();
    let pml4_virt = phys_to_virt::<PageTable>(pml4_phys);

    let pml4_idx = ((virt_addr >> 39) & 0x1FF) as usize;
    let pdpt_idx = ((virt_addr >> 30) & 0x1FF) as usize;
    let pd_idx = ((virt_addr >> 21) & 0x1FF) as usize;
    let pt_idx = ((virt_addr >> 12) & 0x1FF) as usize;
    let page_offset = virt_addr & 0xFFF;

    let res = unsafe {
        let pml4 = &*pml4_virt;
        if (pml4.entries[pml4_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return None;
        }

        let pdpt = &*phys_to_virt::<PageTable>(pml4.entries[pml4_idx] & ENTRY_PHYS_MASK);
        if (pdpt.entries[pdpt_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return None;
        }
        if (pdpt.entries[pdpt_idx] & flags::HUGE_PAGE) != 0 {
            let frame_phys = pdpt.entries[pdpt_idx] & 0x000F_FFFF_C000_0000;
            VMM_LOCK.store(false, Ordering::Release);
            return Some(frame_phys | (virt_addr & 0x3FFF_FFFF));
        }

        let pd = &*phys_to_virt::<PageTable>(pdpt.entries[pdpt_idx] & ENTRY_PHYS_MASK);
        if (pd.entries[pd_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return None;
        }
        if (pd.entries[pd_idx] & flags::HUGE_PAGE) != 0 {
            let frame_phys = pd.entries[pd_idx] & 0x000F_FFFF_FFE0_0000;
            VMM_LOCK.store(false, Ordering::Release);
            return Some(frame_phys | (virt_addr & 0x001F_FFFF));
        }

        let pt = &*phys_to_virt::<PageTable>(pd.entries[pd_idx] & ENTRY_PHYS_MASK);
        if (pt.entries[pt_idx] & flags::PRESENT) == 0 {
            VMM_LOCK.store(false, Ordering::Release);
            return None;
        }

        let frame_phys = pt.entries[pt_idx] & ENTRY_PHYS_MASK;
        Some(frame_phys | page_offset)
    };

    VMM_LOCK.store(false, Ordering::Release);
    res
}

/// Maps a physical MMIO register range into the uncached MMIO virtual space (`MMIO_VIRT_BASE`)
/// using `PRESENT | WRITABLE | WRITE_THROUGH | CACHE_DISABLE | NO_EXECUTE` (PWT + PCD).
pub fn map_mmio(phys_addr: u64, size: usize) -> Result<u64, &'static str> {
    let start_phys = phys_addr & !0xFFF;
    let end_phys = (phys_addr.saturating_add(size as u64).saturating_add(0xFFF)) & !0xFFF;
    let mmio_flags = flags::PRESENT
        | flags::WRITABLE
        | flags::WRITE_THROUGH
        | flags::CACHE_DISABLE
        | flags::NO_EXECUTE;

    let mut page_phys = start_phys;
    while page_phys < end_phys {
        let page_virt = MMIO_VIRT_BASE + page_phys;
        map_page(page_virt, page_phys, mmio_flags)?;
        page_phys += 4096;
    }

    Ok(MMIO_VIRT_BASE + phys_addr)
}

/// Executes a self-test of virtual page mapping, writing, reading, and translation.
#[allow(dead_code)]
pub fn test_vmm_mapping() -> bool {
    // Pick an unused canonical higher-half test address
    let test_virt: u64 = 0xFFFF_A000_0000_0000;
    let test_frame = match pmm::alloc_frame() {
        Some(f) => f,
        None => {
            warn!("VMM Test: PMM out of memory for test frame!");
            return false;
        }
    };

    if let Err(err) = map_page(test_virt, test_frame, flags::PRESENT | flags::WRITABLE | flags::NO_EXECUTE) {
        warn!("VMM Test: map_page failed: {}", err);
        pmm::free_frame(test_frame);
        return false;
    }

    // Verify virtual-to-physical translation
    let translated = virt_to_phys(test_virt);
    if translated != Some(test_frame) {
        warn!(
            "VMM Test: Translation mismatch! Expected 0x{:016x}, got {:?}",
            test_frame, translated
        );
        let _ = unmap_page(test_virt);
        pmm::free_frame(test_frame);
        return false;
    }

    // Test write and read through the newly mapped virtual address
    unsafe {
        let ptr = test_virt as *mut u64;
        ptr.write_volatile(0xCAFE_BABE_DEAD_BEEF);
        let val = ptr.read_volatile();
        if val != 0xCAFE_BABE_DEAD_BEEF {
            warn!("VMM Test: Memory write/read mismatch!");
            let _ = unmap_page(test_virt);
            pmm::free_frame(test_frame);
            return false;
        }
    }

    // Clean up test page
    let _ = unmap_page(test_virt);
    pmm::free_frame(test_frame);

    info!("VMM Test: Successfully mapped, verified, and unmapped test page.");
    true
}
