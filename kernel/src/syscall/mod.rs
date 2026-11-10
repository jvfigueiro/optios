// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: POSIX Syscall Dispatcher & AMD64 Syscall/Sysretq Architecture
// Date: 2026-11-10

//! POSIX System Call Subsystem for OptiOS.
//! Configures MSRs (IA32_EFER, IA32_STAR, IA32_LSTAR, IA32_FMASK), sets up TSS kernel stack,
//! and dispatches POSIX syscalls:
//! - SYS_READ (0)
//! - SYS_WRITE (1)
//! - SYS_OPEN (2)
//! - SYS_CLOSE (3)
//! - SYS_STAT (4)
//! - SYS_GETPID (39)
//! - SYS_EXIT (60)
//! - SYS_UNAME (63)

pub mod loader;

use crate::arch::x86_64::gdt::{self, KERNEL_CODE_SELECTOR};
use crate::arch::x86_64::msr::{self, EFER_SCE, IA32_EFER, IA32_FMASK, IA32_LSTAR, IA32_STAR};
use crate::drivers::video::console;
use crate::fs::vfs;
use crate::{info, warn};

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXIT: u64 = 60;
pub const SYS_UNAME: u64 = 63;

const USER_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000;

#[repr(C)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

#[repr(align(16))]
struct KernelSyscallStack(#[allow(dead_code)] [u8; 16384]);
static mut KERNEL_SYSCALL_STACK: KernelSyscallStack = KernelSyscallStack([0; 16384]);

pub static mut KERNEL_RSP: u64 = 0;
pub static mut USER_RSP: u64 = 0;

pub static mut KERNEL_SAVED_RSP: u64 = 0;
pub static mut KERNEL_SAVED_RBP: u64 = 0;
pub static mut KERNEL_SAVED_RBX: u64 = 0;
pub static mut KERNEL_SAVED_R12: u64 = 0;
pub static mut KERNEL_SAVED_R13: u64 = 0;
pub static mut KERNEL_SAVED_R14: u64 = 0;
pub static mut KERNEL_SAVED_R15: u64 = 0;

/// Initializes the AMD64 `syscall` / `sysretq` hardware architecture.
pub fn init() {
    unsafe {
        // 1. Calculate top of dedicated kernel stack for syscall handling
        let stack_ptr = (&raw const KERNEL_SYSCALL_STACK) as *const u8;
        let stack_top = stack_ptr as u64 + 16384;
        KERNEL_RSP = stack_top;
        gdt::set_kernel_stack(stack_top);

        // 2. Enable System Call Extensions (SCE) in IA32_EFER
        let efer = msr::rdmsr(IA32_EFER);
        msr::wrmsr(IA32_EFER, efer | EFER_SCE);

        // 3. Configure IA32_STAR:
        // Bits [47:32] = Kernel CS/SS base (0x08 -> CS=0x08, SS=0x10)
        // Bits [63:48] = User CS/SS base (0x13 -> SS=0x1B, CS=0x23 on sysretq)
        let star_val = (0x0013u64 << 48) | ((KERNEL_CODE_SELECTOR as u64) << 32);
        msr::wrmsr(IA32_STAR, star_val);

        // 4. Configure IA32_LSTAR with syscall entry point address
        msr::wrmsr(IA32_LSTAR, syscall_entry as *const () as u64);

        // 5. Configure IA32_FMASK: mask Interrupt Flag (IF, bit 9) and Trap Flag (TF, bit 8)
        msr::wrmsr(IA32_FMASK, 0x300);
    }

    info!("Syscall subsystem initialized: IA32_LSTAR registered, Ring 3 -> Ring 0 stack configured.");
}

/// Low-level naked entry point executed when userland invokes `syscall`.
#[unsafe(naked)]
pub unsafe extern "C" fn syscall_entry() {
    core::arch::naked_asm!(
        // 1. Switch to dedicated kernel stack
        "mov qword ptr [rip + {USER_RSP}], rsp",
        "mov rsp, qword ptr [rip + {KERNEL_RSP}]",

        // 2. Save user registers
        "push rcx", // User RIP
        "push r11", // User RFLAGS
        "push rbp",
        "push rbx",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "push rdi",
        "push rsi",
        "push rdx",
        "push r10",
        "push r8",
        "push r9",

        // 3. Arrange arguments according to C ABI:
        // Input: RAX=nr, RDI=a1, RSI=a2, RDX=a3, R10=a4, R8=a5, R9=a6
        // C ABI: RDI=nr, RSI=a1, RDX=a2, RCX=a3, R8=a4, R9=a5
        "mov r9, r8",
        "mov r8, r10",
        "mov rcx, rdx",
        "mov rdx, rsi",
        "mov rsi, rdi",
        "mov rdi, rax",

        "call {DISPATCH}",

        // 4. Restore user registers (RAX preserved as return value)
        "pop r9",
        "pop r8",
        "pop r10",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbx",
        "pop rbp",
        "pop r11", // User RFLAGS
        "pop rcx", // User RIP

        // 5. Restore user RSP and return to Ring 3
        "mov rsp, qword ptr [rip + {USER_RSP}]",
        "sysretq",

        USER_RSP = sym USER_RSP,
        KERNEL_RSP = sym KERNEL_RSP,
        DISPATCH = sym syscall_dispatch,
    );
}

/// High-level POSIX syscall dispatcher.
#[no_mangle]
pub extern "C" fn syscall_dispatch(
    nr: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    _arg4: u64,
    _arg5: u64,
) -> i64 {
    match nr {
        SYS_READ => sys_read(arg1, arg2 as *mut u8, arg3 as usize),
        SYS_WRITE => sys_write(arg1, arg2 as *const u8, arg3 as usize),
        SYS_OPEN => sys_open(arg1 as *const u8, arg2, arg3),
        SYS_CLOSE => sys_close(arg1),
        SYS_STAT => sys_stat(arg1 as *const u8, arg2 as *mut u8),
        SYS_GETPID => sys_getpid(),
        SYS_EXIT => sys_exit(arg1 as i32),
        SYS_UNAME => sys_uname(arg1 as *mut UtsName),
        _ => {
            warn!("Syscall: Unknown syscall number {}", nr);
            -38 // -ENOSYS
        }
    }
}

fn sys_read(fd: u64, buf: *mut u8, count: usize) -> i64 {
    if buf as u64 >= USER_SPACE_LIMIT || buf as u64 + count as u64 > USER_SPACE_LIMIT {
        return -14; // -EFAULT
    }

    if fd == 0 {
        // Read from keyboard (stdin)
        let mut bytes_read = 0;
        while bytes_read < count {
            if let Some(crate::drivers::keyboard::KeyEvent::Char(ch)) =
                crate::drivers::keyboard::poll_key()
            {
                unsafe {
                    *buf.add(bytes_read) = ch as u8;
                }
                bytes_read += 1;
                break;
            }
            core::hint::spin_loop();
        }
        bytes_read as i64
    } else {
        // Read from VFS node
        let node_index = (fd - 3) as usize;
        let mut slice = unsafe { core::slice::from_raw_parts_mut(buf, count) };
        match vfs::read(node_index, 0, &mut slice) {
            Ok(n) => n as i64,
            Err(e) => -(e as i64),
        }
    }
}

fn sys_write(fd: u64, buf: *const u8, count: usize) -> i64 {
    if buf as u64 >= USER_SPACE_LIMIT || buf as u64 + count as u64 > USER_SPACE_LIMIT {
        return -14; // -EFAULT
    }

    if fd == 1 || fd == 2 {
        let slice = unsafe { core::slice::from_raw_parts(buf, count) };
        if let Ok(s) = core::str::from_utf8(slice) {
            console::print_str(s);
        }
        count as i64
    } else {
        -9 // -EBADF
    }
}

fn sys_open(path_ptr: *const u8, _flags: u64, _mode: u64) -> i64 {
    if path_ptr as u64 >= USER_SPACE_LIMIT {
        return -14; // -EFAULT
    }

    let mut len = 0;
    while len < 256 {
        unsafe {
            if *path_ptr.add(len) == 0 {
                break;
            }
        }
        len += 1;
    }
    if len >= 256 {
        return -36; // -ENAMETOOLONG
    }

    let slice = unsafe { core::slice::from_raw_parts(path_ptr, len) };
    let path = match core::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return -22, // -EINVAL
    };

    match vfs::open(path) {
        Ok(node_idx) => (node_idx + 3) as i64, // Map node index to fd >= 3
        Err(e) => -(e as i64),
    }
}

fn sys_close(_fd: u64) -> i64 {
    0
}

fn sys_stat(path_ptr: *const u8, _statbuf: *mut u8) -> i64 {
    if path_ptr as u64 >= USER_SPACE_LIMIT {
        return -14; // -EFAULT
    }

    let mut len = 0;
    while len < 256 {
        unsafe {
            if *path_ptr.add(len) == 0 {
                break;
            }
        }
        len += 1;
    }

    let slice = unsafe { core::slice::from_raw_parts(path_ptr, len) };
    let path = match core::str::from_utf8(slice) {
        Ok(s) => s,
        Err(_) => return -22,
    };

    match vfs::stat(path) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn sys_getpid() -> i64 {
    1
}

fn sys_uname(buf: *mut UtsName) -> i64 {
    if buf as u64 >= USER_SPACE_LIMIT {
        return -14; // -EFAULT
    }

    unsafe {
        let uts = &mut *buf;
        copy_cstr(&mut uts.sysname, b"OptiOS");
        copy_cstr(&mut uts.nodename, b"optios");
        copy_cstr(&mut uts.release, b"1.0.0");
        copy_cstr(&mut uts.version, b"Milestone 7 (Ring 3 Userland)");
        copy_cstr(&mut uts.machine, b"x86_64");
        copy_cstr(&mut uts.domainname, b"local");
    }
    0
}

fn copy_cstr(dst: &mut [u8; 65], src: &[u8]) {
    dst.fill(0);
    let len = src.len().min(64);
    dst[..len].copy_from_slice(&src[..len]);
}

/// Terminate the user process and return cleanly to kernel execution context.
fn sys_exit(status: i32) -> i64 {
    unsafe {
        exit_to_kernel(status);
    }
}

/// Restores the saved kernel stack and registers, jumping back to the shell execution flow.
#[unsafe(naked)]
pub unsafe extern "C" fn exit_to_kernel(status: i32) -> ! {
    core::arch::naked_asm!(
        // Restore kernel stack and callee-saved registers
        "mov rsp, qword ptr [rip + {SAVED_RSP}]",
        "mov rbp, qword ptr [rip + {SAVED_RBP}]",
        "mov rbx, qword ptr [rip + {SAVED_RBX}]",
        "mov r12, qword ptr [rip + {SAVED_R12}]",
        "mov r13, qword ptr [rip + {SAVED_R13}]",
        "mov r14, qword ptr [rip + {SAVED_R14}]",
        "mov r15, qword ptr [rip + {SAVED_R15}]",

        // Restore kernel segment registers
        "mov ax, 0x10",
        "mov ds, ax",
        "mov es, ax",
        "mov fs, ax",
        "mov gs, ax",
        "mov ss, ax",

        // Re-enable interrupts in kernel mode
        "sti",

        // Set return value in EAX from status (passed in EDI)
        "mov eax, edi",
        "ret",

        SAVED_RSP = sym KERNEL_SAVED_RSP,
        SAVED_RBP = sym KERNEL_SAVED_RBP,
        SAVED_RBX = sym KERNEL_SAVED_RBX,
        SAVED_R12 = sym KERNEL_SAVED_R12,
        SAVED_R13 = sym KERNEL_SAVED_R13,
        SAVED_R14 = sym KERNEL_SAVED_R14,
        SAVED_R15 = sym KERNEL_SAVED_R15,
    );
}
