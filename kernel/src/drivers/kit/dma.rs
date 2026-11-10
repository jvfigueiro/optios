// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Direct Memory Access (DMA) Contiguous Buffer Abstraction
// Date: 2026-11-10

//! Physically contiguous, uncached DMA memory allocation abstraction.
//! Provides hardware-ready physical base addresses and safe virtual CPU pointers.

use crate::mm::{pmm, vmm};

/// A physically contiguous, uncached DMA buffer allocated for hardware controllers.
#[allow(dead_code)]
pub struct DmaBuffer {
    phys_addr: u64,
    virt_addr: u64,
    size: usize,
    page_count: usize,
}

#[allow(dead_code)]
impl DmaBuffer {
    /// Allocates a 4 KiB physically contiguous DMA page mapped into kernel virtual address space.
    pub fn allocate_4k() -> Option<Self> {
        let phys = pmm::alloc_frame()?;
        let virt = vmm::phys_to_virt::<u8>(phys) as u64;

        unsafe {
            core::ptr::write_bytes(virt as *mut u8, 0, 4096);
        }

        Some(Self {
            phys_addr: phys,
            virt_addr: virt,
            size: 4096,
            page_count: 1,
        })
    }

    /// Returns the hardware physical address for programming DMA descriptor registers.
    #[inline]
    pub fn phys_addr(&self) -> u64 {
        self.phys_addr
    }

    /// Returns the kernel virtual address for CPU read/write access.
    #[inline]
    pub fn virt_addr(&self) -> u64 {
        self.virt_addr
    }

    /// Returns the size in bytes of the DMA buffer.
    #[inline]
    pub fn size(&self) -> usize {
        self.size
    }

    /// Returns the number of physical 4 KiB pages comprising this buffer.
    #[inline]
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Returns an immutable byte slice view of the buffer memory.
    #[inline]
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.virt_addr as *const u8, self.size) }
    }

    /// Returns a mutable byte slice view of the buffer memory.
    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.virt_addr as *mut u8, self.size) }
    }

    /// Returns an immutable typed pointer to the buffer memory.
    #[inline]
    pub fn as_ptr<T>(&self) -> *const T {
        self.virt_addr as *const T
    }

    /// Returns a mutable typed pointer to the buffer memory.
    #[inline]
    pub fn as_mut_ptr<T>(&mut self) -> *mut T {
        self.virt_addr as *mut T
    }

    /// Zeroes the entire contents of the DMA buffer.
    #[inline]
    pub fn zero(&mut self) {
        unsafe {
            core::ptr::write_bytes(self.virt_addr as *mut u8, 0, self.size);
        }
    }
}
