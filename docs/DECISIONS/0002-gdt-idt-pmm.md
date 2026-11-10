# 2. Kernel GDT/TSS, IDT Exception Handling, and Bitmap Physical Memory Manager

* **Date:** 2026-11-09
* **Status:** Accepted
* **Context:** Milestone 2 CPU protection, fault isolation, and physical frame management

## Context

After bootstrapping via Limine, the kernel initially runs under the bootloader's provisional GDT and with interrupts disabled without an active IDT. Any CPU fault (e.g., page fault, invalid opcode, divide by zero) would cause an unhandled crash or silent triple fault. Furthermore, kernel memory management requires a deterministic allocator for 4 KiB physical frames before dynamic paging or heap allocation can be initialized.

## Decisions

1. **Dedicated Kernel GDT & TSS:**
   - Replace the bootloader's GDT with a dedicated OptiOS Global Descriptor Table containing descriptors for 64-bit Kernel Code, Kernel Data, User Code, User Data, and a 16-byte TSS descriptor.
   - Configure IST 1 (Interrupt Stack Table) in the TSS with a dedicated static emergency stack (16 KiB) to guarantee that Double Faults (`#DF`) always execute on a known valid stack.
2. **IDT & Exception Dispatching:**
   - Implement a 64-bit Interrupt Descriptor Table registering dedicated handlers for CPU architectural exceptions (`#DE`, `#UD`, `#DF`, `#GP`, `#PF`).
   - `#PF` captures the faulting virtual address from the `CR2` control register.
   - Fatal exceptions trigger an Emergency Diagnostic Screen dumped to both serial console and graphical framebuffer before halting the processor (`cli; hlt`).
3. **Bitmap Physical Memory Manager (PMM):**
   - Implement a 4 KiB page-frame allocator backed by a bit array (bitmap).
   - Place the bitmap in the first sufficiently large usable physical memory region provided by Limine, accessed via the HHDM virtual offset.
   - Calculate usable physical RAM accurately, strictly excluding reserved, MMIO, and framebuffer regions.

## Consequences

- The kernel gains fault containment and detailed diagnostic crash dumps instead of triple-fault reboots.
- Physical memory can now be reliably allocated and freed page by page (4 KiB granularity).
- Sets the foundation for Milestone 3 (Virtual Memory Management / Paging and Kernel Heap Allocator).
