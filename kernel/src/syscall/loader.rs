// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Userland ELF64 Process Spawner & Ring 3 Execution Engine
// Date: 2026-11-10

//! Userland Process Spawner for OptiOS.
//! Loads ELF64 executables from the VFS, maps segments into user-accessible virtual pages,
//! establishes a dedicated 16 KiB user stack, and performs Ring 0 -> Ring 3 transitions.

extern crate alloc;
use alloc::vec;

use crate::fs::vfs;
use crate::mm::{pmm, vmm};
use optios_elf_loader::parse_elf;

pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES: u64 = 4; // 16 KiB stack

/// Spawns an ELF64 binary from the given VFS path and executes it in Ring 3.
/// Returns the process exit status upon completion.
pub fn spawn_user_process(path: &str) -> Result<i32, &'static str> {
    // 1. Stat file from VFS to obtain size
    let stat = vfs::stat(path).map_err(|_| "Executable not found in VFS")?;

    // 2. Open file descriptor from VFS
    let fd = vfs::open(path).map_err(|_| "Executable not found in VFS")?;

    // 3. Read full ELF file from VFS into temporary buffer
    let mut elf_data = vec![0u8; stat.size];
    vfs::read(fd, 0, &mut elf_data).map_err(|_| "Failed to read executable file")?;

    // 4. Parse and strictly validate ELF64 headers and segments
    let elf = parse_elf(&elf_data).map_err(|_| "Invalid or corrupted ELF64 executable")?;

    // 5. Map each PT_LOAD segment into user space
    let page_flags = vmm::flags::PRESENT | vmm::flags::WRITABLE | vmm::flags::USER_ACCESSIBLE;

    for seg in &elf.segments {
        let vaddr_start = seg.vaddr & !0xFFF;
        let vaddr_end = (seg.vaddr + seg.mem_size + 0xFFF) & !0xFFF;

        let mut curr_vaddr = vaddr_start;
        while curr_vaddr < vaddr_end {
            let paddr = match pmm::alloc_frame() {
                Some(p) => p,
                None => return Err("Out of physical memory allocating user segment frame"),
            };

            let frame_ptr = vmm::phys_to_virt::<u8>(paddr);
            unsafe {
                core::ptr::write_bytes(frame_ptr, 0, 4096);
            }

            if let Err(e) = vmm::map_page(curr_vaddr, paddr, page_flags) {
                return Err(e);
            }

            curr_vaddr += 4096;
        }

        // Copy segment file data to destination user memory
        unsafe {
            core::ptr::copy_nonoverlapping(
                seg.file_data.as_ptr(),
                seg.vaddr as *mut u8,
                seg.file_data.len(),
            );
        }
    }

    // 5. Allocate and map 16 KiB User Stack
    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * 4096);
    for i in 0..USER_STACK_PAGES {
        let page_vaddr = stack_base + (i * 4096);
        let paddr = match pmm::alloc_frame() {
            Some(p) => p,
            None => return Err("Out of physical memory allocating user stack frame"),
        };

        let frame_ptr = vmm::phys_to_virt::<u8>(paddr);
        unsafe {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }

        if let Err(e) = vmm::map_page(page_vaddr, paddr, page_flags) {
            return Err(e);
        }
    }

    // 6. Enter Ring 3 and execute user code
    let exit_status = unsafe { enter_user_mode(elf.entry_point, USER_STACK_TOP) };
    crate::arch::x86_64::sti();

    Ok(exit_status)
}

#[unsafe(naked)]
unsafe extern "C" fn enter_user_mode(entry: u64, user_stack: u64) -> i32 {
    core::arch::naked_asm!(
        // Save kernel registers
        "mov qword ptr [rip + {SAVED_RSP}], rsp",
        "mov qword ptr [rip + {SAVED_RBP}], rbp",
        "mov qword ptr [rip + {SAVED_RBX}], rbx",
        "mov qword ptr [rip + {SAVED_R12}], r12",
        "mov qword ptr [rip + {SAVED_R13}], r13",
        "mov qword ptr [rip + {SAVED_R14}], r14",
        "mov qword ptr [rip + {SAVED_R15}], r15",

        // Build iretq stack frame:
        // SS = User Data (0x18 | 3 = 0x1B)
        // RSP = user_stack
        // RFLAGS = 0x202 (IF enabled)
        // CS = User Code (0x20 | 3 = 0x23)
        // RIP = entry
        "push 0x1B",
        "push rsi",
        "push 0x202",
        "push 0x23",
        "push rdi",

        // Load User Data segments
        "mov ax, 0x1B",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",

        "iretq",

        SAVED_RSP = sym super::KERNEL_SAVED_RSP,
        SAVED_RBP = sym super::KERNEL_SAVED_RBP,
        SAVED_RBX = sym super::KERNEL_SAVED_RBX,
        SAVED_R12 = sym super::KERNEL_SAVED_R12,
        SAVED_R13 = sym super::KERNEL_SAVED_R13,
        SAVED_R14 = sym super::KERNEL_SAVED_R14,
        SAVED_R15 = sym super::KERNEL_SAVED_R15,
    );
}
