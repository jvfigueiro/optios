// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Userland Diagnostic Utility (sysinfo) for Ring 3 Verification
// Date: 2026-11-10

//! Standalone Ring 3 userland diagnostic executable for OptiOS.
//! Executed by the kernel via `libs/elf-loader` and tests POSIX syscalls:
//! - SYS_WRITE (1)
//! - SYS_GETPID (39)
//! - SYS_UNAME (63)
//! - SYS_EXIT (60)

#![no_std]
#![no_main]

use core::panic::PanicInfo;

#[repr(C)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl UtsName {
    pub const fn zeroed() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }
}

#[inline(always)]
unsafe fn syscall1(nr: u64, arg1: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr as i64 => ret,
        in("rdi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

#[inline(always)]
unsafe fn syscall3(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") nr as i64 => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack, preserves_flags)
    );
    ret
}

pub fn print_str(s: &str) {
    unsafe {
        syscall3(1, 1, s.as_ptr() as u64, s.len() as u64);
    }
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    print_str("[sysinfo] Hello from Userland (Ring 3)!\n");

    let pid = unsafe { syscall1(39, 0) };
    print_str("[sysinfo] Process PID: ");
    if pid >= 0 {
        print_num(pid as u64);
    } else {
        print_str("unknown");
    }
    print_str("\n");

    let mut uts = UtsName::zeroed();
    let res = unsafe { syscall1(63, &mut uts as *mut UtsName as u64) };
    if res == 0 {
        print_str("[sysinfo] System Name: ");
        print_cstr(&uts.sysname);
        print_str("\n[sysinfo] Release: ");
        print_cstr(&uts.release);
        print_str("\n[sysinfo] Version: ");
        print_cstr(&uts.version);
        print_str("\n[sysinfo] Machine: ");
        print_cstr(&uts.machine);
        print_str("\n");
    }

    print_str("[sysinfo] Syscall execution succeeded. Exiting cleanly via sys_exit(0)...\n");
    unsafe {
        syscall1(60, 0);
    }

    loop {}
}

fn print_num(mut n: u64) {
    if n == 0 {
        print_str("0");
        return;
    }
    let mut buf = [0u8; 20];
    let mut i = 0;
    while n > 0 {
        buf[i] = b'0' + (n % 10) as u8;
        n /= 10;
        i += 1;
    }
    for j in 0..i / 2 {
        buf.swap(j, i - 1 - j);
    }
    if let Ok(s) = core::str::from_utf8(&buf[..i]) {
        print_str(s);
    }
}

fn print_cstr(bytes: &[u8]) {
    let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
    if let Ok(s) = core::str::from_utf8(&bytes[..len]) {
        print_str(s);
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print_str("[sysinfo] PANIC in userland!\n");
    unsafe {
        syscall1(60, 1);
    }
    loop {}
}
