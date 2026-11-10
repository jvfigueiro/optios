// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: IrqSafe Kernel Heap Allocator (GlobalAlloc Free-List)
// Date: 2026-11-10

//! Kernel Heap Allocator implementing Rust's `GlobalAlloc`.
//! Uses an IrqSafe Free-List Allocator with adjacent chunk coalescing.

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicBool, Ordering};
use crate::mm::pmm::{self, PAGE_SIZE};
use crate::mm::vmm::{self, flags};
use crate::{info, warn};

pub const HEAP_START: u64 = 0xFFFF_C000_0000_0000;
pub const HEAP_SIZE: usize = 2 * 1024 * 1024; // 2 MiB initial capacity

/// Saves RFLAGS (IF bit) and disables hardware interrupts (CLI) to prevent ISR deadlocks.
#[inline]
fn save_and_disable_irq() -> bool {
    let rflags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            "cli",
            out(reg) rflags,
            options(nomem, preserves_flags)
        );
    }
    (rflags & (1 << 9)) != 0
}

/// Restores the interrupt flag (STI) if interrupts were previously enabled.
#[inline]
fn restore_irq(was_enabled: bool) {
    if was_enabled {
        unsafe {
            core::arch::asm!("sti", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Header placed at the beginning of each free block.
#[repr(C)]
struct FreeBlock {
    size: usize,
    next: Option<*mut FreeBlock>,
}

pub struct FreeListAllocator {
    head: Option<*mut FreeBlock>,
    pub allocated_bytes: usize,
    pub total_bytes: usize,
}

unsafe impl Send for FreeListAllocator {}
unsafe impl Sync for FreeListAllocator {}

impl FreeListAllocator {
    pub const fn empty() -> Self {
        Self {
            head: None,
            allocated_bytes: 0,
            total_bytes: 0,
        }
    }

    /// Initializes the allocator with a raw memory region.
    pub unsafe fn init(&mut self, start: usize, size: usize) {
        let block_ptr = start as *mut FreeBlock;
        (*block_ptr).size = size;
        (*block_ptr).next = None;
        self.head = Some(block_ptr);
        self.total_bytes = size;
        self.allocated_bytes = 0;
    }

    /// Allocates memory satisfying the requested Layout.
    pub unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let align = layout.align().max(core::mem::align_of::<FreeBlock>());
        let header_size = core::mem::size_of::<FreeBlock>();
        let needed_size = layout.size().max(header_size);

        let mut prev: Option<*mut FreeBlock> = None;
        let mut curr = self.head;

        while let Some(block) = curr {
            let block_addr = block as usize;
            // Align the payload address
            let payload_addr = (block_addr + header_size + align - 1) & !(align - 1);
            let total_needed = (payload_addr - block_addr) + needed_size;

            if (*block).size >= total_needed {
                let remaining = (*block).size - total_needed;

                if remaining >= header_size + 16 {
                    // Split the block
                    let new_block_addr = block_addr + total_needed;
                    let new_block = new_block_addr as *mut FreeBlock;
                    (*new_block).size = remaining;
                    (*new_block).next = (*block).next;

                    if let Some(p) = prev {
                        (*p).next = Some(new_block);
                    } else {
                        self.head = Some(new_block);
                    }
                } else {
                    // Consume the entire block
                    if let Some(p) = prev {
                        (*p).next = (*block).next;
                    } else {
                        self.head = (*block).next;
                    }
                }

                // Store allocation size right before payload for tracking
                let alloc_prefix = (payload_addr - core::mem::size_of::<usize>()) as *mut usize;
                *alloc_prefix = total_needed;

                self.allocated_bytes += total_needed;
                return payload_addr as *mut u8;
            }

            prev = Some(block);
            curr = (*block).next;
        }

        core::ptr::null_mut()
    }

    /// Deallocates memory and coalesces adjacent free blocks.
    pub unsafe fn dealloc(&mut self, ptr: *mut u8, _layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let payload_addr = ptr as usize;
        let alloc_prefix = (payload_addr - core::mem::size_of::<usize>()) as *mut usize;
        let total_size = *alloc_prefix;

        let block_addr = payload_addr - (payload_addr % core::mem::align_of::<FreeBlock>()).max(core::mem::size_of::<FreeBlock>());
        let freed_block = block_addr as *mut FreeBlock;
        (*freed_block).size = total_size;
        (*freed_block).next = None;

        self.allocated_bytes = self.allocated_bytes.saturating_sub(total_size);

        // Insert into free list in sorted order by address
        if self.head.is_none() || (self.head.unwrap() as usize) > block_addr {
            (*freed_block).next = self.head;
            self.head = Some(freed_block);
        } else {
            let mut curr = self.head.unwrap();
            while let Some(next_block) = (*curr).next {
                if (next_block as usize) > block_addr {
                    break;
                }
                curr = next_block;
            }
            (*freed_block).next = (*curr).next;
            (*curr).next = Some(freed_block);
        }

        // Coalesce adjacent free blocks
        let mut curr_opt = self.head;
        while let Some(curr) = curr_opt {
            if let Some(next) = (*curr).next {
                let curr_end = (curr as usize) + (*curr).size;
                if curr_end == (next as usize) {
                    (*curr).size += (*next).size;
                    (*curr).next = (*next).next;
                    continue; // Check again for next adjacent block
                }
            }
            curr_opt = (*curr).next;
        }
    }
}

pub struct LockedHeap {
    inner: UnsafeCell<FreeListAllocator>,
    lock: AtomicBool,
}

unsafe impl Sync for LockedHeap {}

unsafe impl GlobalAlloc for LockedHeap {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let irq_enabled = save_and_disable_irq();
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        let heap_ptr = self.inner.get();
        let ptr = (*heap_ptr).alloc(layout);
        self.lock.store(false, Ordering::Release);
        restore_irq(irq_enabled);
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        let irq_enabled = save_and_disable_irq();
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }
        let heap_ptr = self.inner.get();
        (*heap_ptr).dealloc(ptr, layout);
        self.lock.store(false, Ordering::Release);
        restore_irq(irq_enabled);
    }
}

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap {
    inner: UnsafeCell::new(FreeListAllocator::empty()),
    lock: AtomicBool::new(false),
};

/// Returns (allocated_bytes, total_bytes) of the kernel heap in an IrqSafe manner.
pub fn get_stats() -> (usize, usize) {
    let irq_enabled = save_and_disable_irq();
    while ALLOCATOR.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }
    let stats = unsafe {
        let heap_ptr = ALLOCATOR.inner.get();
        ((*heap_ptr).allocated_bytes, (*heap_ptr).total_bytes)
    };
    ALLOCATOR.lock.store(false, Ordering::Release);
    restore_irq(irq_enabled);
    stats
}

/// Initializes the kernel heap by allocating physical frames from PMM and mapping them via VMM.
pub fn init() {
    let pages_count = HEAP_SIZE / PAGE_SIZE as usize;

    for i in 0..pages_count {
        let virt_addr = HEAP_START + (i as u64 * PAGE_SIZE);
        let phys_frame = pmm::alloc_frame().expect("Kernel Heap: PMM out of memory while pre-mapping heap!");
        vmm::map_page(
            virt_addr,
            phys_frame,
            flags::PRESENT | flags::WRITABLE | flags::NO_EXECUTE,
        )
        .expect("Kernel Heap: Failed to map virtual heap page!");
    }

    unsafe {
        let heap_ptr = ALLOCATOR.inner.get();
        (*heap_ptr).init(HEAP_START as usize, HEAP_SIZE);
    }

    info!(
        "Kernel Heap initialized (IrqSafe): {} KiB mapped at virtual 0x{:016x}.",
        HEAP_SIZE / 1024,
        HEAP_START
    );
}

/// Diagnostic self-test for dynamic memory allocation using Rust `alloc` collections.
#[allow(dead_code)]
pub fn test_heap_allocation() -> bool {
    extern crate alloc;
    use alloc::boxed::Box;
    use alloc::string::String;
    use alloc::vec::Vec;

    // 1. Box test
    let test_box = Box::new(0xDEAD_BEEF_CAFE_BABEu64);
    if *test_box != 0xDEAD_BEEF_CAFE_BABEu64 {
        warn!("Heap Test: Box value corrupted!");
        return false;
    }

    // 2. Dynamic Vec test
    let mut test_vec = Vec::new();
    for i in 0..128 {
        test_vec.push(i * 3);
    }
    for (idx, &val) in test_vec.iter().enumerate() {
        if val != (idx as u32 * 3) {
            warn!("Heap Test: Vec element mismatch at index {}", idx);
            return false;
        }
    }

    // 3. String allocation
    let test_str = String::from("OptiOS Dynamic String on Kernel Heap");
    if test_str != "OptiOS Dynamic String on Kernel Heap" {
        warn!("Heap Test: String content mismatch!");
        return false;
    }

    info!(
        "Heap Test: Successfully verified Box, Vec (128 elements), and dynamic String allocations."
    );
    true
}
