// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Userland ELF64 Process Spawner & Ring 3 Execution Engine
// Date: 2026-11-11

//! Userland Process Spawner for OptiOS.
//! Loads ELF64 executables from the VFS, maps segments into user-accessible virtual pages,
//! sets up a dedicated 16 KiB user stack with POSIX argc/argv System V ABI conventions,
//! manages dynamic program break (heap), and performs Ring 0 -> Ring 3 transitions.

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

use crate::fs::vfs;
use crate::mm::{pmm, vmm};
use optios_elf_loader::parse_elf;

pub const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_0000;
pub const USER_STACK_PAGES: u64 = 4; // 16 KiB stack

/// Process Control Block tracking memory, heap, and per-process descriptors.
pub struct ProcessControlBlock {
    pub user_pages: Vec<(u64, u64)>, // (vaddr, paddr)
    pub heap_start: u64,
    pub heap_current: u64,
    pub heap_max: u64,
    pub fd_table: super::FileDescriptorTable,
}

pub static mut CURRENT_PROCESS: Option<ProcessControlBlock> = None;

/// Frees all physical frames and unmaps virtual pages allocated to the active process.
pub fn clean_current_process() {
    unsafe {
        let proc_ptr = &raw mut CURRENT_PROCESS;
        let proc_opt = (*proc_ptr).take();
        if let Some(mut proc) = proc_opt {
            // Close all open user descriptors in the per-process table
            for i in 3..super::MAX_USER_FDS {
                if proc.fd_table.entries[i].is_open {
                    let _ = vfs::close(proc.fd_table.entries[i].vfs_fd);
                    proc.fd_table.entries[i].is_open = false;
                }
            }

            for (vaddr, paddr) in proc.user_pages {
                let _ = vmm::unmap_page(vaddr);
                pmm::free_frame(paddr);
            }
        }
    }
}

/// Spawns an ELF64 binary from the given VFS path and executes it in Ring 3.
/// Returns the process exit status upon completion.
pub fn spawn_user_process(path: &str) -> Result<i32, &'static str> {
    spawn_user_process_with_args(path, &[path])
}

/// Spawns an ELF64 binary with command-line arguments and executes it in Ring 3.
pub fn spawn_user_process_with_args(path: &str, args: &[&str]) -> Result<i32, &'static str> {
    // 0. Enforce VFS mount execution policy (noexec)
    let mount_flags = vfs::get_mount_flags(path);
    if (mount_flags & vfs::MNT_NOEXEC) != 0 {
        return Err("Execution prohibited by mount policy (noexec)");
    }

    // Clean any remnant process resources
    clean_current_process();

    // 1. Stat file from VFS to obtain size
    let stat = vfs::stat(path).map_err(|_| "Executable not found in VFS")?;

    // 2. Open file descriptor from VFS
    let fd = vfs::open(path).map_err(|_| "Executable not found in VFS")?;

    // 3. Read full ELF file from VFS into temporary buffer
    let mut elf_data = vec![0u8; stat.size];
    vfs::read(fd, 0, &mut elf_data).map_err(|_| "Failed to read executable file")?;
    let _ = vfs::close(fd);

    // 4. Parse and strictly validate ELF64 headers and segments
    let elf = parse_elf(&elf_data).map_err(|_| "Invalid or corrupted ELF64 executable")?;

    let mut user_pages: Vec<(u64, u64)> = Vec::new();
    let mut max_vaddr = 0u64;

    for seg in &elf.segments {
        let end = seg.vaddr + seg.mem_size;
        if end > max_vaddr {
            max_vaddr = end;
        }
    }

    // 5. Map each PT_LOAD segment into user space with strict W^X enforcement
    for seg in &elf.segments {
        let is_exec = (seg.flags & optios_elf_loader::PF_X) != 0;
        let is_write = (seg.flags & optios_elf_loader::PF_W) != 0;

        if is_exec && is_write {
            clean_current_process();
            return Err("ELF segment violates W^X policy (simultaneously writable and executable)");
        }

        let mut seg_page_flags = vmm::flags::PRESENT | vmm::flags::USER_ACCESSIBLE;
        if is_write {
            seg_page_flags |= vmm::flags::WRITABLE;
        }
        if !is_exec {
            seg_page_flags |= vmm::flags::NO_EXECUTE;
        }

        let vaddr_start = seg.vaddr & !0xFFF;
        let vaddr_end = (seg.vaddr + seg.mem_size + 0xFFF) & !0xFFF;

        let mut curr_vaddr = vaddr_start;
        while curr_vaddr < vaddr_end {
            let paddr = match pmm::alloc_frame() {
                Some(p) => p,
                None => {
                    clean_current_process();
                    return Err("Out of physical memory allocating user segment frame");
                }
            };

            let frame_ptr = vmm::phys_to_virt::<u8>(paddr);
            unsafe {
                core::ptr::write_bytes(frame_ptr, 0, 4096);
            }

            // Copy file data intersecting with this 4 KiB frame directly via HHDM
            let file_data_end = seg.vaddr.saturating_add(seg.file_data.len() as u64);
            if curr_vaddr < file_data_end && (curr_vaddr + 4096) > seg.vaddr {
                let page_offset = if seg.vaddr > curr_vaddr {
                    (seg.vaddr - curr_vaddr) as usize
                } else {
                    0
                };
                let file_offset = if curr_vaddr > seg.vaddr {
                    (curr_vaddr - seg.vaddr) as usize
                } else {
                    0
                };
                let bytes_in_page = 4096 - page_offset;
                let bytes_in_file = seg.file_data.len().saturating_sub(file_offset);
                let copy_len = core::cmp::min(bytes_in_page, bytes_in_file);
                if copy_len > 0 {
                    unsafe {
                        core::ptr::copy_nonoverlapping(
                            seg.file_data.as_ptr().add(file_offset),
                            frame_ptr.add(page_offset),
                            copy_len,
                        );
                    }
                }
            }

            if let Err(e) = vmm::map_page(curr_vaddr, paddr, seg_page_flags) {
                pmm::free_frame(paddr);
                clean_current_process();
                return Err(e);
            }

            user_pages.push((curr_vaddr, paddr));
            curr_vaddr += 4096;
        }
    }

    // 6. Define dynamic heap boundaries (page-aligned right after highest PT_LOAD segment)
    let heap_start = (max_vaddr + 0xFFF) & !0xFFF;
    let heap_current = heap_start;
    let heap_max = heap_start + (16 * 1024 * 1024); // 16 MiB user heap limit

    // 7. Allocate and map 16 KiB User Stack (W^X enforced: Non-Executable)
    let stack_page_flags = vmm::flags::PRESENT
        | vmm::flags::WRITABLE
        | vmm::flags::USER_ACCESSIBLE
        | vmm::flags::NO_EXECUTE;
    let stack_base = USER_STACK_TOP - (USER_STACK_PAGES * 4096);
    for i in 0..USER_STACK_PAGES {
        let page_vaddr = stack_base + (i * 4096);
        let paddr = match pmm::alloc_frame() {
            Some(p) => p,
            None => {
                clean_current_process();
                return Err("Out of physical memory allocating user stack frame");
            }
        };

        let frame_ptr = vmm::phys_to_virt::<u8>(paddr);
        unsafe {
            core::ptr::write_bytes(frame_ptr, 0, 4096);
        }

        if let Err(e) = vmm::map_page(page_vaddr, paddr, stack_page_flags) {
            pmm::free_frame(paddr);
            clean_current_process();
            return Err(e);
        }

        user_pages.push((page_vaddr, paddr));
    }

    // 8. Setup POSIX System V AMD64 ABI argc/argv layout on User Stack
    unsafe {
        if crate::arch::x86_64::user_copy::is_smap_enabled() {
            crate::arch::x86_64::user_copy::stac();
        }
    }

    let mut sp = USER_STACK_TOP;
    let mut arg_ptrs: Vec<u64> = Vec::new();

    // Place argument string data (null-terminated)
    for arg in args {
        let bytes = arg.as_bytes();
        let len = bytes.len() + 1;
        sp -= len as u64;
        unsafe {
            core::ptr::copy_nonoverlapping(bytes.as_ptr(), sp as *mut u8, bytes.len());
            *((sp + bytes.len() as u64) as *mut u8) = 0;
        }
        arg_ptrs.push(sp);
    }

    // 16-byte align before pointer tables
    sp &= !0x0F;

    // NULL sentinel terminating argv array
    sp -= 8;
    unsafe {
        *(sp as *mut u64) = 0;
    }

    // Push argv pointers in reverse order
    for &ptr in arg_ptrs.iter().rev() {
        sp -= 8;
        unsafe {
            *(sp as *mut u64) = ptr;
        }
    }
    let argv_ptr = sp;

    // Push argc
    sp -= 8;
    unsafe {
        *(sp as *mut u64) = args.len() as u64;
    }

    // Maintain 16-byte alignment
    if (sp & 0x0F) != 0 {
        sp -= 8;
    }

    unsafe {
        if crate::arch::x86_64::user_copy::is_smap_enabled() {
            crate::arch::x86_64::user_copy::clac();
        }
    }

    // 9. Register active process tracking
    unsafe {
        CURRENT_PROCESS = Some(ProcessControlBlock {
            user_pages,
            heap_start,
            heap_current,
            heap_max,
            fd_table: super::FileDescriptorTable::new(),
        });
    }

    // 10. Enter Ring 3 and execute user code
    let exit_status = unsafe {
        enter_user_mode(
            elf.entry_point,
            sp,
            args.len() as u64,
            argv_ptr,
        )
    };
    crate::arch::x86_64::sti();

    // 11. Clean all allocated frames and unmap user pages
    clean_current_process();

    Ok(exit_status)
}

#[unsafe(naked)]
unsafe extern "C" fn enter_user_mode(
    entry: u64,
    user_stack: u64,
    argc: u64,
    argv: u64,
) -> i32 {
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
        // RSP = user_stack (passed in rsi)
        // RFLAGS = 0x202 (IF enabled)
        // CS = User Code (0x20 | 3 = 0x23)
        // RIP = entry (passed in rdi)
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

        // Pass argc in RDI and argv in RSI for userland _start
        "mov rdi, rdx",
        "mov rsi, rcx",
        "xor rdx, rdx",
        "xor rcx, rcx",

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
