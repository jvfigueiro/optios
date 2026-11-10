// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Interrupt Descriptor Table (IDT), Exception & Hardware IRQ Dispatcher
// Date: 2026-11-10

//! Interrupt Descriptor Table (IDT), CPU Exception Handling, and Hardware IRQ Dispatching.

use core::arch::asm;
use core::mem::size_of;
use crate::arch::x86_64::gdt::{DOUBLE_FAULT_IST_INDEX, KERNEL_CODE_SELECTOR};
use crate::arch::x86_64::{halt_loop, pic, pit};
use crate::drivers::keyboard;
use crate::drivers::video::framebuffer::with_framebuffer;
use crate::{error, info};

/// x86-64 16-byte Interrupt Descriptor Gate.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    type_attr: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    pub const fn empty() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            type_attr: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Sets the handler address, selector, and attributes for this entry.
    pub fn set_handler(&mut self, handler: usize, ist_index: u8) {
        self.selector = KERNEL_CODE_SELECTOR;
        self.ist = ist_index & 0x07;
        self.type_attr = 0x8E; // Present, Ring 0, 64-bit Interrupt Gate (disables IF on entry)
        self.offset_low = (handler & 0xFFFF) as u16;
        self.offset_mid = ((handler >> 16) & 0xFFFF) as u16;
        self.offset_high = ((handler >> 32) & 0xFFFF_FFFF) as u32;
        self.reserved = 0;
    }
}

/// IDT pointer structure passed to `lidt`.
#[repr(C, packed)]
pub struct IdtPointer {
    pub limit: u16,
    pub base: u64,
}

/// CPU execution context saved during an architectural exception or hardware IRQ.
#[repr(C)]
#[derive(Debug)]
pub struct ExceptionContext {
    // Saved by common stub
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rbp: u64,
    pub rdi: u64,
    pub rsi: u64,
    pub rdx: u64,
    pub rcx: u64,
    pub rbx: u64,
    pub rax: u64,
    // Pushed by specific stub
    pub vector: u64,
    pub error_code: u64,
    // Pushed by hardware on interrupt/exception
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

/// 256-entry Interrupt Descriptor Table.
static mut IDT: [IdtEntry; 256] = [IdtEntry::empty(); 256];

/// Reads the faulting address from the CR2 control register.
#[inline]
pub fn read_cr2() -> u64 {
    let cr2: u64;
    unsafe {
        asm!("mov {}, cr2", out(reg) cr2, options(nomem, nostack, preserves_flags));
    }
    cr2
}

/// Formats a 64-bit unsigned integer as a hexadecimal ASCII string into a buffer.
pub fn u64_to_hex<'a>(val: u64, buf: &'a mut [u8; 18]) -> &'a str {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..16 {
        let shift = (15 - i) * 4;
        let nibble = ((val >> shift) & 0x0F) as usize;
        buf[2 + i] = HEX_CHARS[nibble];
    }
    core::str::from_utf8(buf).unwrap_or("0x0000000000000000")
}

/// Rust exception dispatcher invoked by low-level assembly stubs for CPU faults (vectors 0..31).
#[no_mangle]
pub extern "C" fn optios_exception_dispatcher(ctx: &ExceptionContext) -> ! {
    let cr2 = if ctx.vector == 14 { read_cr2() } else { 0 };

    let exception_name = match ctx.vector {
        0 => "#DE (Divide Error)",
        1 => "#DB (Debug)",
        2 => "#NMI (Non-Maskable Interrupt)",
        3 => "#BP (Breakpoint)",
        4 => "#OF (Overflow)",
        5 => "#BR (BOUND Range Exceeded)",
        6 => "#UD (Invalid Opcode)",
        7 => "#NM (Device Not Available)",
        8 => "#DF (Double Fault)",
        10 => "#TS (Invalid TSS)",
        11 => "#NP (Segment Not Present)",
        12 => "#SS (Stack-Segment Fault)",
        13 => "#GP (General Protection Fault)",
        14 => "#PF (Page Fault)",
        16 => "#MF (x87 Floating-Point Error)",
        17 => "#AC (Alignment Check)",
        18 => "#MC (Machine Check)",
        19 => "#XM (SIMD Floating-Point Exception)",
        _ => "Unknown Exception",
    };

    // 1. Output diagnostic dump to serial console
    error!("========================================================");
    error!("FATAL CPU EXCEPTION DUMP: {}", exception_name);
    error!("========================================================");
    error!("Vector:     0x{:02x} ({})", ctx.vector, ctx.vector);
    error!("Error Code: 0x{:016x}", ctx.error_code);
    error!("RIP:        0x{:016x}", ctx.rip);
    error!("CS:         0x{:04x}", ctx.cs);
    error!("RFLAGS:     0x{:016x}", ctx.rflags);
    error!("RSP:        0x{:016x}", ctx.rsp);
    error!("SS:         0x{:04x}", ctx.ss);
    error!("RAX:        0x{:016x}   RBX: 0x{:016x}", ctx.rax, ctx.rbx);
    error!("RCX:        0x{:016x}   RDX: 0x{:016x}", ctx.rcx, ctx.rdx);
    error!("RSI:        0x{:016x}   RDI: 0x{:016x}", ctx.rsi, ctx.rdi);
    error!("RBP:        0x{:016x}   R8:  0x{:016x}", ctx.rbp, ctx.r8);
    error!("R9:         0x{:016x}   R10: 0x{:016x}", ctx.r9, ctx.r10);
    error!("R11:        0x{:016x}   R12: 0x{:016x}", ctx.r11, ctx.r12);
    error!("R13:        0x{:016x}   R14: 0x{:016x}", ctx.r13, ctx.r14);
    error!("R15:        0x{:016x}", ctx.r15);
    if ctx.vector == 14 {
        error!("CR2:        0x{:016x} (Faulting Virtual Address)", cr2);
    }
    error!("========================================================");
    error!("System halted safely (CLI; HLT).");

    // 2. Render Graphical Exception Dump Screen to Framebuffer
    with_framebuffer(|fb| {
        let dump_bg = 0x007f1d1d;     // Deep crimson red
        let box_bg = 0x00450a0a;      // Dark maroon
        let border_col = 0x00dc2626;  // Vivid red
        let text_white = 0x00ffffff;
        let text_yellow = 0x00fef08a;
        let text_cyan = 0x0038bdf8;

        let w = (fb.width - 40).min(960);
        let h = 380;
        let x = 20;
        let y = 30;

        fb.fill_rect(x, y, w, h, box_bg);
        fb.draw_rect_border(x, y, w, h, 3, border_col);

        // Header bar
        fb.fill_rect(x, y, w, 40, dump_bg);
        fb.draw_string(x + 16, y + 12, "KERNEL EXCEPTION PANIC DUMP", text_white, Some(dump_bg));

        let mut row = y + 54;
        fb.draw_string(x + 20, row, "Exception:", text_yellow, None);
        fb.draw_string(x + 140, row, exception_name, text_white, None);

        let mut hex_buf = [0u8; 18];

        row += 24;
        fb.draw_string(x + 20, row, "RIP:", text_cyan, None);
        fb.draw_string(x + 80, row, u64_to_hex(ctx.rip, &mut hex_buf), text_white, None);
        fb.draw_string(x + 300, row, "RSP:", text_cyan, None);
        fb.draw_string(x + 360, row, u64_to_hex(ctx.rsp, &mut hex_buf), text_white, None);
        fb.draw_string(x + 580, row, "RFLAGS:", text_cyan, None);
        fb.draw_string(x + 660, row, u64_to_hex(ctx.rflags, &mut hex_buf), text_white, None);

        row += 24;
        fb.draw_string(x + 20, row, "Error Code:", text_cyan, None);
        fb.draw_string(x + 140, row, u64_to_hex(ctx.error_code, &mut hex_buf), text_white, None);
        if ctx.vector == 14 {
            fb.draw_string(x + 400, row, "CR2 (Fault Addr):", text_yellow, None);
            fb.draw_string(x + 580, row, u64_to_hex(cr2, &mut hex_buf), text_yellow, None);
        }

        row += 24;
        fb.draw_string(x + 20, row, "RAX:", text_cyan, None);
        fb.draw_string(x + 80, row, u64_to_hex(ctx.rax, &mut hex_buf), text_white, None);
        fb.draw_string(x + 300, row, "RBX:", text_cyan, None);
        fb.draw_string(x + 360, row, u64_to_hex(ctx.rbx, &mut hex_buf), text_white, None);
        fb.draw_string(x + 580, row, "RCX:", text_cyan, None);
        fb.draw_string(x + 660, row, u64_to_hex(ctx.rcx, &mut hex_buf), text_white, None);

        row += 24;
        fb.draw_string(x + 20, row, "RDX:", text_cyan, None);
        fb.draw_string(x + 80, row, u64_to_hex(ctx.rdx, &mut hex_buf), text_white, None);
        fb.draw_string(x + 300, row, "RSI:", text_cyan, None);
        fb.draw_string(x + 360, row, u64_to_hex(ctx.rsi, &mut hex_buf), text_white, None);
        fb.draw_string(x + 580, row, "RDI:", text_cyan, None);
        fb.draw_string(x + 660, row, u64_to_hex(ctx.rdi, &mut hex_buf), text_white, None);

        row += 24;
        fb.draw_string(x + 20, row, "RBP:", text_cyan, None);
        fb.draw_string(x + 80, row, u64_to_hex(ctx.rbp, &mut hex_buf), text_white, None);
        fb.draw_string(x + 300, row, "R8: ", text_cyan, None);
        fb.draw_string(x + 360, row, u64_to_hex(ctx.r8, &mut hex_buf), text_white, None);
        fb.draw_string(x + 580, row, "R9: ", text_cyan, None);
        fb.draw_string(x + 660, row, u64_to_hex(ctx.r9, &mut hex_buf), text_white, None);

        // Footer message
        let footer_y = y + h - 36;
        fb.fill_rect(x + 2, footer_y, w - 4, 34, 0x00260404);
        fb.draw_string(
            x + 16,
            footer_y + 10,
            "* CPU halted via CLI; HLT. Inspect serial telemetry (COM1) for debugging.",
            text_yellow,
            None,
        );
    });

    halt_loop();
}

/// Hardware IRQ dispatcher invoked by assembly stubs for external interrupts (vectors 32..47).
/// Must never allocate on the heap or acquire non-IRQ-safe locks.
#[no_mangle]
pub extern "C" fn optios_irq_dispatcher(ctx: &ExceptionContext) {
    match ctx.vector {
        32 => {
            // IRQ 0: Programmable Interval Timer (100 Hz)
            pit::on_tick();
            pic::send_eoi(0);
        }
        33 => {
            // IRQ 1: PS/2 Keyboard Controller
            keyboard::on_irq1();
            pic::send_eoi(1);
        }
        44 => {
            // IRQ 12: PS/2 Mouse Controller
            crate::drivers::mouse::on_irq12();
            pic::send_eoi(12);
        }
        v if v >= 32 && v < 48 => {
            pic::send_eoi((v - 32) as u8);
        }
        _ => {}
    }
}

// Low-level assembly entry stubs
extern "C" {
    fn isr0();  // #DE Divide Error
    fn isr6();  // #UD Invalid Opcode
    fn isr8();  // #DF Double Fault (uses IST 1)
    fn isr13(); // #GP General Protection Fault
    fn isr14(); // #PF Page Fault
    fn irq32(); // IRQ 0: System Timer
    fn irq33(); // IRQ 1: PS/2 Keyboard
    fn irq44(); // IRQ 12: PS/2 Mouse
}

core::arch::global_asm!(
    r#"
    .macro ISR_NOERRCODE num
    .global isr\num
    isr\num:
        push 0          // Dummy error code
        push \num       // Vector number
        jmp isr_common
    .endm

    .macro ISR_ERRCODE num
    .global isr\num
    isr\num:
        push \num       // Vector number (error code already pushed by CPU)
        jmp isr_common
    .endm

    .macro IRQ_STUB vec
    .global irq\vec
    irq\vec:
        push 0          // Dummy error code
        push \vec       // Vector number
        jmp irq_common
    .endm

    ISR_NOERRCODE 0
    ISR_NOERRCODE 6
    ISR_ERRCODE   8
    ISR_ERRCODE   13
    ISR_ERRCODE   14

    IRQ_STUB      32
    IRQ_STUB      33
    IRQ_STUB      44

    isr_common:
        cld
        push rax
        push rbx
        push rcx
        push rdx
        push rsi
        push rdi
        push rbp
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        mov rdi, rsp
        call optios_exception_dispatcher
        cli
        hlt

    irq_common:
        cld
        push rax
        push rbx
        push rcx
        push rdx
        push rsi
        push rdi
        push rbp
        push r8
        push r9
        push r10
        push r11
        push r12
        push r13
        push r14
        push r15

        mov rdi, rsp
        call optios_irq_dispatcher

        pop r15
        pop r14
        pop r13
        pop r12
        pop r11
        pop r10
        pop r9
        pop r8
        pop rbp
        pop rdi
        pop rsi
        pop rdx
        pop rcx
        pop rbx
        pop rax
        add rsp, 16
        iretq
    "#
);

/// Initializes and loads the x86-64 Interrupt Descriptor Table (IDT).
pub fn init() {
    unsafe {
        let idt_ptr_raw = &raw mut IDT;
        // Architectural CPU Exception Gates (0..31)
        (*idt_ptr_raw)[0].set_handler(isr0 as *const () as usize, 0);
        (*idt_ptr_raw)[6].set_handler(isr6 as *const () as usize, 0);
        // Vector 8 (#DF) explicitly configured with IST 1
        (*idt_ptr_raw)[8].set_handler(isr8 as *const () as usize, DOUBLE_FAULT_IST_INDEX as u8);
        (*idt_ptr_raw)[13].set_handler(isr13 as *const () as usize, 0);
        (*idt_ptr_raw)[14].set_handler(isr14 as *const () as usize, 0);

        // External Hardware Interrupt Gates (32..47)
        (*idt_ptr_raw)[32].set_handler(irq32 as *const () as usize, 0);
        (*idt_ptr_raw)[33].set_handler(irq33 as *const () as usize, 0);
        (*idt_ptr_raw)[44].set_handler(irq44 as *const () as usize, 0);

        let idt_ptr = IdtPointer {
            limit: (size_of::<[IdtEntry; 256]>() - 1) as u16,
            base: idt_ptr_raw as u64,
        };

        asm!("lidt [{}]", in(reg) &idt_ptr, options(readonly, nostack, preserves_flags));
    }
    info!("IDT initialized: Exception gates (#DE, #UD, #DF, #GP, #PF) & IRQ gates (32, 33, 44) installed.");
}
