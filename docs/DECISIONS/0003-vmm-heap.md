# 3. Virtual Memory Management (VMM) and Kernel Heap Allocator

* **Date:** 2026-11-09
* **Status:** Accepted
* **Context:** Milestone 3 dynamic memory allocation and fine-grained virtual memory mapping

## Context

While Milestone 2 established physical memory allocation (PMM) and CPU protection (GDT/TSS and IDT), the kernel still operated with static arrays and bootloader page tables. Standard Rust heap data structures (`Box`, `Vec`, `String`, BTreeMap) require a global allocator (`#[global_allocator]`), and fine-grained isolation requires a Virtual Memory Manager (VMM) to map and unmap 4 KiB virtual pages.

## Decisions

1. **4-Level Paging Virtual Memory Manager (VMM):**
   - Implement x86-64 4-level paging traversal (PML4 $\rightarrow$ PDPT $\rightarrow$ PD $\rightarrow$ PT $\rightarrow$ Physical Frame).
   - Use the Higher-Half Direct Map (HHDM) virtual offset to safely inspect and manipulate page table structures in physical memory.
   - Implement `read_cr3()`, `write_cr3()`, `invlpg()`, `map_page()`, and `unmap_page()`.
   - Automatically allocate missing intermediate page tables (PDPT, PD, PT) on demand from the PMM.
2. **Kernel Heap Allocator (`GlobalAlloc`):**
   - Reserve a canonical higher-half virtual window for the kernel heap starting at `0xFFFF_9000_0000_0000` with an initial capacity of 4 MiB (1024 pages).
   - Pre-map these virtual pages to 4 KiB physical frames allocated from the PMM.
   - Implement a deterministic Free-List Block Allocator with alignment support and chunk coalescing to mitigate memory fragmentation.
   - Register the allocator as the Rust `#[global_allocator]`, enabling standard `extern crate alloc;`.
3. **Diagnostic Self-Test:**
   - Execute dynamic allocations using `Box` and `Vec`, verifying data integrity, expansion, and memory reclamation.

## Consequences

- The kernel gains full dynamic allocation capabilities, unlocking idiomatic Rust collections.
- Virtual memory mapping is now fine-grained and independent of bootloader structures.
- Establishes the prerequisite memory foundation for future multitasking (not implemented / planned in R5), process isolation, and dynamic device drivers.
