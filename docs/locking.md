<!--
// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Kernel Synchronization & Locking Architecture Specification
// Date: 2026-11-09
-->

# OptiOS Locking & Synchronization Architecture

## 1. Overview & Core Invariants

The OptiOS kernel currently operates on a single CPU core with hardware interrupts enabled (`sti`) for timer polling (PIT 100 Hz on IRQ 0) and keyboard scancode ingestion (PS/2 on IRQ 1).

To maintain absolute system stability and prevent deadlocks, the kernel enforces the following fundamental synchronization invariant:

> **Core Synchronization Invariant:**
> **No Interrupt Service Routine (ISR) may acquire a non-IRQ-safe spinlock.**

An ISR that attempts to acquire a spinlock already held by the interrupted thread context on the same CPU core will spin indefinitely, hanging the entire operating system.

---

## 2. Kernel Lock Inventory

| Lock Identifier | Source File | Primitive Type | IRQ-Safe? | Permitted Contexts | Description & Protected Data |
|---|---|---|:---:|---|---|
| **`PMM_LOCK`** | `kernel/src/mm/pmm.rs` | `AtomicBool` Spinlock | **No** | Boot / Thread Context only | Protects the physical frame allocation bitmap and scanner indices. Never called from ISRs. |
| **`VMM_LOCK`** | `kernel/src/mm/vmm.rs` | `AtomicBool` Spinlock | **No** | Boot / Thread Context only | Protects 4-level page table traversals and page frame mappings. Never called from ISRs. |
| **`VFS_TABLE.lock`** | `kernel/src/fs/vfs.rs` | `AtomicBool` Spinlock | **No** | Boot / Thread Context only | Protects the global in-memory node table (`VfsTable`). Never called from ISRs. |
| **`LockedHeap.lock`** | `kernel/src/mm/heap.rs` | `AtomicBool` Spinlock with `CLI`/`STI` | **Yes** | Any context (Thread, ISR, Panic) | Global heap allocator (`FreeListAllocator`). Saves RFLAGS and disables hardware interrupts (`cli`) before acquiring; restores RFLAGS on release. |
| **`SERIAL_LOCK`** | `kernel/src/drivers/serial.rs` | `AtomicBool` Spinlock | **No** | Thread Context, Exception Panic Handlers | Synchronizes multi-byte formatting to the 16550 UART COM1 serial port. |
| **`FB_LOCK`** | `kernel/src/drivers/video/framebuffer.rs` | `AtomicBool` Spinlock | **No** | Thread Context, Exception Panic Handlers | Synchronizes graphical string and rectangle rendering on the emergency linear framebuffer. |

---

## 3. Hardware ISR Conformance Audit

Hardware ISRs registered in the IDT strictly obey the synchronization invariant:

1. **PIT Timer ISR (`irq32` -> `pit::on_tick`):**
   - Touches only an atomic counter (`TICKS.fetch_add(1, Ordering::Relaxed)`).
   - Issues a port write to send EOI to the 8259 PIC (`outb 0x20, 0x20`).
   - Does **not** acquire any spinlock, perform heap allocations, or invoke logging.
2. **Keyboard Controller ISR (`irq33` -> `keyboard::on_irq1`):**
   - Reads port `0x60` (`inb`).
   - Enqueues the scancode into an atomic, lock-free Single-Producer Single-Consumer (SPSC) circular buffer (`SCANCODE_BUFFER`).
   - Sends EOI to the PIC.
   - Does **not** acquire any spinlock, perform heap allocations, or invoke logging.

Because hardware interrupts touch no non-IRQ-safe locks, hardware timer and keyboard interrupts can never deadlock against thread code executing inside `PMM_LOCK`, `VMM_LOCK`, `VFS_TABLE.lock`, `SERIAL_LOCK`, or `FB_LOCK`.

---

## 4. Exception Handler Reentrancy Caveat

### The Problem

CPU architectural exceptions (vectors `0..31`, such as `#PF`, `#GP`, `#UD`, `#DF`) are dispatched to `optios_exception_dispatcher(&ExceptionContext) -> !`.
This dispatcher is a **terminal panic handler** that dumps CPU registers to COM1 serial console via `error!` and renders a graphical crash screen to the framebuffer via `with_framebuffer`.

While hardware interrupts are automatically disabled by the x86-64 Interrupt Gate (`type_attr = 0x8E`, clearing `RFLAGS.IF`), synchronous CPU exceptions occur in the thread context that caused them:

```
[ Thread Context ]
       |
       v Acquires SERIAL_LOCK (e.g. inside info! or serial_println!)
       |
       x --- Synchronous Exception (#PF, #GP, or memory fault) occurs here ---
       |
       v (CPU jumps to exception gate; IF cleared)
[ optios_exception_dispatcher ]
       |
       v Calls error! -> attempts to acquire SERIAL_LOCK
       |
       * DEADLOCK: spins forever on the same CPU core because SERIAL_LOCK was already held.
```

The same caveat applies to `FB_LOCK` if an exception occurs while rendering the framebuffer dashboard.

---

## 5. Architectural Proposal: Trylock / Force-Unlock Fallback

To guarantee that fatal exception telemetry is never silenced by a deadlocked logging or framebuffer spinlock, the following mechanism is proposed for future implementation:

### Proposed Design (Design Proposal Only — Awaiting Approval)

1. **Panic Reentrancy Counter / Owner Flag:**
   Maintain an atomic state `PANIC_ACTIVE: AtomicBool`.
2. **Trylock with Bounded Spin Count:**
   During fatal exception dumps, replace indefinite `while lock.compare_exchange_weak(...)` loops with a bounded trylock:
   - Attempt to acquire `SERIAL_LOCK` normally for up to `100_000` spin iterations.
   - If the lock cannot be acquired within `100_000` cycles (indicating it was held by the faulting thread), log a brief warning character directly to port `0x3F8` without locking, **force-reset the lock** (`SERIAL_LOCK.store(false, Ordering::SeqCst)`), and acquire it for the crash dump.
3. **Emergency Serial Bypass:**
   Provide a raw `serial_write_unlocked(&str)` function that directly outputs bytes to the UART without touching `SERIAL_LOCK`, used exclusively once an unrecoverable exception is confirmed.

> **Status:** This design is documented here as an engineering proposal. In accordance with project policy, **it is not yet implemented** and awaits review and approval from the project maintainer.
