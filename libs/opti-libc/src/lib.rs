// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Pure Userland C/Rust Runtime, POSIX Syscalls & Dynamic Heap Allocator
// Date: 2026-11-10

//! Minimal, pure `#![no_std]` userland standard library for OptiOS.
//! Provides POSIX system call wrappers, print/println macros, command-line argument
//! parsing, and a dynamic memory allocator backed by `sys_brk`.

#![no_std]

use core::alloc::{GlobalAlloc, Layout};
use core::cell::UnsafeCell;
use core::fmt;
use core::sync::atomic::{AtomicBool, Ordering};

pub const SYS_READ: u64 = 0;
pub const SYS_WRITE: u64 = 1;
pub const SYS_OPEN: u64 = 2;
pub const SYS_CLOSE: u64 = 3;
pub const SYS_STAT: u64 = 4;
pub const SYS_LSEEK: u64 = 8;
pub const SYS_BRK: u64 = 12;
pub const SYS_GETPID: u64 = 39;
pub const SYS_EXIT: u64 = 60;
pub const SYS_UNAME: u64 = 63;

pub const SEEK_SET: u64 = 0;
pub const SEEK_CUR: u64 = 1;
pub const SEEK_END: u64 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct UtsName {
    pub sysname: [u8; 65],
    pub nodename: [u8; 65],
    pub release: [u8; 65],
    pub version: [u8; 65],
    pub machine: [u8; 65],
    pub domainname: [u8; 65],
}

impl UtsName {
    pub const fn new() -> Self {
        Self {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        }
    }

    pub fn sysname(&self) -> &str {
        c_str_to_str(&self.sysname)
    }

    pub fn nodename(&self) -> &str {
        c_str_to_str(&self.nodename)
    }

    pub fn release(&self) -> &str {
        c_str_to_str(&self.release)
    }

    pub fn version(&self) -> &str {
        c_str_to_str(&self.version)
    }

    pub fn machine(&self) -> &str {
        c_str_to_str(&self.machine)
    }
}

fn c_str_to_str(buf: &[u8; 65]) -> &str {
    let mut len = 0;
    while len < 65 && buf[len] != 0 {
        len += 1;
    }
    core::str::from_utf8(&buf[..len]).unwrap_or("")
}

// ---------------------------------------------------------------------------
// Low-level Syscall Primitives
// ---------------------------------------------------------------------------

#[inline(always)]
pub unsafe fn syscall0(nr: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inout("rax") nr => ret,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall1(nr: u64, arg1: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inout("rax") nr => ret,
        in("rdi") arg1,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall2(nr: u64, arg1: u64, arg2: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inout("rax") nr => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    ret
}

#[inline(always)]
pub unsafe fn syscall3(nr: u64, arg1: u64, arg2: u64, arg3: u64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inout("rax") nr => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        out("rcx") _,
        out("r11") _,
        options(nostack)
    );
    ret
}

// ---------------------------------------------------------------------------
// POSIX Userland Syscall Wrappers
// ---------------------------------------------------------------------------

pub fn read(fd: u64, buf: &mut [u8]) -> Result<usize, i64> {
    let res = unsafe { syscall3(SYS_READ, fd, buf.as_mut_ptr() as u64, buf.len() as u64) };
    if res >= 0 {
        Ok(res as usize)
    } else {
        Err(res)
    }
}

pub fn write(fd: u64, buf: &[u8]) -> Result<usize, i64> {
    let res = unsafe { syscall3(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64) };
    if res >= 0 {
        Ok(res as usize)
    } else {
        Err(res)
    }
}

pub fn open(path: &str, flags: u64, mode: u64) -> Result<u64, i64> {
    let mut null_term_path = [0u8; 256];
    let len = path.len().min(255);
    null_term_path[..len].copy_from_slice(&path.as_bytes()[..len]);
    null_term_path[len] = 0;

    let res = unsafe { syscall3(SYS_OPEN, null_term_path.as_ptr() as u64, flags, mode) };
    if res >= 0 {
        Ok(res as u64)
    } else {
        Err(res)
    }
}

pub fn close(fd: u64) -> Result<(), i64> {
    let res = unsafe { syscall1(SYS_CLOSE, fd) };
    if res >= 0 {
        Ok(())
    } else {
        Err(res)
    }
}

pub fn stat(path: &str, statbuf: &mut [u8]) -> Result<(), i64> {
    let mut null_term_path = [0u8; 256];
    let len = path.len().min(255);
    null_term_path[..len].copy_from_slice(&path.as_bytes()[..len]);
    null_term_path[len] = 0;

    let res = unsafe { syscall2(SYS_STAT, null_term_path.as_ptr() as u64, statbuf.as_mut_ptr() as u64) };
    if res >= 0 {
        Ok(())
    } else {
        Err(res)
    }
}

pub fn lseek(fd: u64, offset: i64, whence: u64) -> Result<i64, i64> {
    let res = unsafe { syscall3(SYS_LSEEK, fd, offset as u64, whence) };
    if res >= 0 {
        Ok(res)
    } else {
        Err(res)
    }
}

pub fn brk(addr: u64) -> u64 {
    unsafe { syscall1(SYS_BRK, addr) as u64 }
}

pub fn getpid() -> u64 {
    unsafe { syscall0(SYS_GETPID) as u64 }
}

pub fn exit(status: i32) -> ! {
    unsafe {
        syscall1(SYS_EXIT, status as u64);
        loop {
            core::arch::asm!("hlt", options(nomem, nostack));
        }
    }
}

pub fn uname(uts: &mut UtsName) -> Result<(), i64> {
    let res = unsafe { syscall1(SYS_UNAME, uts as *mut UtsName as u64) };
    if res >= 0 {
        Ok(())
    } else {
        Err(res)
    }
}

// ---------------------------------------------------------------------------
// Console I/O Formatting
// ---------------------------------------------------------------------------

pub struct Stdout;

impl fmt::Write for Stdout {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let _ = write(1, s.as_bytes());
        Ok(())
    }
}

#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => {
        let _ = <$crate::Stdout as core::fmt::Write>::write_fmt(&mut $crate::Stdout, format_args!($($arg)*));
    };
}

#[macro_export]
macro_rules! println {
    () => {
        $crate::print!("\n");
    };
    ($($arg:tt)*) => {
        let _ = <$crate::Stdout as core::fmt::Write>::write_fmt(&mut $crate::Stdout, format_args!($($arg)*));
        $crate::print!("\n");
    };
}

// ---------------------------------------------------------------------------
// Global Memory Allocator (sys_brk Bump Allocator)
// ---------------------------------------------------------------------------

struct AllocatorState {
    heap_start: usize,
    heap_current: usize,
    heap_end: usize,
}

pub struct OptiAllocator {
    state: UnsafeCell<AllocatorState>,
    lock: AtomicBool,
}

unsafe impl Sync for OptiAllocator {}

impl OptiAllocator {
    pub const fn new() -> Self {
        Self {
            state: UnsafeCell::new(AllocatorState {
                heap_start: 0,
                heap_current: 0,
                heap_end: 0,
            }),
            lock: AtomicBool::new(false),
        }
    }
}

unsafe impl GlobalAlloc for OptiAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        while self.lock.compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
            core::hint::spin_loop();
        }

        let state = &mut *self.state.get();

        // Initialize heap break on first allocation
        if state.heap_start == 0 {
            let initial_brk = brk(0) as usize;
            state.heap_start = initial_brk;
            state.heap_current = initial_brk;
            state.heap_end = initial_brk;
        }

        let align = layout.align();
        let size = layout.size();
        let alloc_start = (state.heap_current + align - 1) & !(align - 1);
        let alloc_end = alloc_start + size;

        if alloc_end > state.heap_end {
            // Need to grow heap via sys_brk
            let needed = alloc_end - state.heap_end;
            let chunk_size = ((needed + 0xFFFF) & !0xFFFF).max(65536); // allocate in >= 64 KiB increments
            let new_break = state.heap_end + chunk_size;
            let result_brk = brk(new_break as u64) as usize;

            if result_brk < new_break {
                // Out of memory
                self.lock.store(false, Ordering::Release);
                return core::ptr::null_mut();
            }

            state.heap_end = result_brk;
        }

        state.heap_current = alloc_end;
        self.lock.store(false, Ordering::Release);

        alloc_start as *mut u8
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {
        // Bump allocator: individual deallocations are no-ops;
        // entire heap is reclaimed automatically by kernel upon sys_exit.
    }
}

// ---------------------------------------------------------------------------
// Argument Extraction Helpers
// ---------------------------------------------------------------------------

/// Parses C-style argc and argv pointers passed from kernel.
pub unsafe fn extract_args(argc: u64, argv: *const *const u8) -> &'static [&'static str] {
    static mut ARGS_BUF: [&str; 16] = [""; 16];
    let count = (argc as usize).min(16);
    let mut actual_count = 0;

    for i in 0..count {
        let ptr = *argv.add(i);
        if ptr.is_null() {
            break;
        }
        let mut len = 0;
        while *ptr.add(len) != 0 {
            len += 1;
        }
        let slice = core::slice::from_raw_parts(ptr, len);
        if let Ok(s) = core::str::from_utf8(slice) {
            let buf_ptr = &raw mut ARGS_BUF;
            (*buf_ptr)[actual_count] = s;
            actual_count += 1;
        }
    }

    let buf_ptr = &raw const ARGS_BUF;
    let buf_ref: &[&str; 16] = &*buf_ptr;
    &buf_ref[..actual_count]
}
