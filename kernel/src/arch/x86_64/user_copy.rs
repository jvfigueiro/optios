// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Safe User Space Memory Copy Primitives (copy_from_user, copy_to_user, SMAP/SMEP)
// Date: 2026-11-10

//! Safe user memory access routines with SMAP (Supervisor Mode Access Prevention)
//! and SMEP (Supervisor Mode Execution Prevention) integration.

use core::sync::atomic::{AtomicBool, Ordering};
use crate::posix::errno::Errno;

pub const USER_SPACE_LIMIT: u64 = 0x0000_8000_0000_0000;

static SMAP_ENABLED: AtomicBool = AtomicBool::new(false);
static SMEP_ENABLED: AtomicBool = AtomicBool::new(false);

/// Enables SMEP (CR4 bit 20) and SMAP (CR4 bit 21) if supported by the CPU.
pub fn init_smep_smap() {
    let max_leaf = core::arch::x86_64::__cpuid(0).eax;
    if max_leaf >= 7 {
        let cpuid_7 = core::arch::x86_64::__cpuid_count(7, 0);
        let has_smep = (cpuid_7.ebx & (1 << 7)) != 0;
        let has_smap = (cpuid_7.ebx & (1 << 20)) != 0;

        unsafe {
            let mut cr4 = read_cr4();
            if has_smep {
                cr4 |= 1 << 20;
                SMEP_ENABLED.store(true, Ordering::SeqCst);
            }
            if has_smap {
                cr4 |= 1 << 21;
                SMAP_ENABLED.store(true, Ordering::SeqCst);
            }
            write_cr4(cr4);
        }

        crate::info!(
            "CPU Security: SMEP={}, SMAP={}",
            SMEP_ENABLED.load(Ordering::Relaxed),
            SMAP_ENABLED.load(Ordering::Relaxed)
        );
    }
}

/// Returns true if SMAP hardware enforcement is active.
pub fn is_smap_enabled() -> bool {
    SMAP_ENABLED.load(Ordering::Relaxed)
}

/// Returns true if SMEP hardware enforcement is active.
#[allow(dead_code)]
pub fn is_smep_enabled() -> bool {
    SMEP_ENABLED.load(Ordering::Relaxed)
}

/// Reads the CR4 control register.
#[inline(always)]
pub unsafe fn read_cr4() -> u64 {
    let cr4: u64;
    core::arch::asm!("mov {}, cr4", out(reg) cr4, options(nomem, nostack, preserves_flags));
    cr4
}

/// Writes a new value to the CR4 control register.
#[inline(always)]
pub unsafe fn write_cr4(val: u64) {
    core::arch::asm!("mov cr4, {}", in(reg) val, options(nomem, nostack, preserves_flags));
}

/// Sets the Alignment Check (AC) flag in RFLAGS, temporarily permitting Ring 0
/// access to user-accessible memory pages under SMAP.
#[inline(always)]
pub unsafe fn stac() {
    core::arch::asm!("stac", options(nomem, nostack, preserves_flags));
}

/// Clears the Alignment Check (AC) flag in RFLAGS, restoring strict SMAP protection.
#[inline(always)]
pub unsafe fn clac() {
    core::arch::asm!("clac", options(nomem, nostack, preserves_flags));
}

/// Safely copies `dst.len()` bytes from user virtual memory `user_src` to kernel buffer `dst`.
/// Validates pointer bounds, checks for 64-bit integer overflow, and uses `stac`/`clac`
/// around the transfer if SMAP is enabled.
pub fn copy_from_user(dst: &mut [u8], user_src: *const u8) -> Result<(), Errno> {
    let len = dst.len();
    if len == 0 {
        return Ok(());
    }
    let src_addr = user_src as u64;
    if src_addr >= USER_SPACE_LIMIT {
        return Err(Errno::EFAULT);
    }
    let end_addr = src_addr.checked_add(len as u64).ok_or(Errno::EFAULT)?;
    if end_addr > USER_SPACE_LIMIT {
        return Err(Errno::EFAULT);
    }

    unsafe {
        if is_smap_enabled() {
            stac();
        }
        core::ptr::copy_nonoverlapping(user_src, dst.as_mut_ptr(), len);
        if is_smap_enabled() {
            clac();
        }
    }
    Ok(())
}

/// Safely copies `src.len()` bytes from kernel buffer `src` to user virtual memory `user_dst`.
/// Validates pointer bounds, checks for 64-bit integer overflow, and uses `stac`/`clac`
/// around the transfer if SMAP is enabled.
pub fn copy_to_user(user_dst: *mut u8, src: &[u8]) -> Result<(), Errno> {
    let len = src.len();
    if len == 0 {
        return Ok(());
    }
    let dst_addr = user_dst as u64;
    if dst_addr >= USER_SPACE_LIMIT {
        return Err(Errno::EFAULT);
    }
    let end_addr = dst_addr.checked_add(len as u64).ok_or(Errno::EFAULT)?;
    if end_addr > USER_SPACE_LIMIT {
        return Err(Errno::EFAULT);
    }

    unsafe {
        if is_smap_enabled() {
            stac();
        }
        core::ptr::copy_nonoverlapping(src.as_ptr(), user_dst, len);
        if is_smap_enabled() {
            clac();
        }
    }
    Ok(())
}

/// Safely copies a null-terminated UTF-8 string up to `max_len` from user virtual memory.
pub fn copy_cstr_from_user(user_src: *const u8, max_len: usize) -> Result<alloc::string::String, Errno> {
    let src_addr = user_src as u64;
    if src_addr >= USER_SPACE_LIMIT {
        return Err(Errno::EFAULT);
    }

    unsafe {
        if is_smap_enabled() {
            stac();
        }
        let mut len = 0;
        while len < max_len {
            let curr = src_addr.checked_add(len as u64).ok_or(Errno::EFAULT)?;
            if curr >= USER_SPACE_LIMIT {
                if is_smap_enabled() {
                    clac();
                }
                return Err(Errno::EFAULT);
            }
            if *user_src.add(len) == 0 {
                break;
            }
            len += 1;
        }

        if len >= max_len {
            if is_smap_enabled() {
                clac();
            }
            return Err(Errno::ENAMETOOLONG);
        }

        let slice = core::slice::from_raw_parts(user_src, len);
        let s = core::str::from_utf8(slice).map_err(|_| {
            if is_smap_enabled() {
                clac();
            }
            Errno::EINVAL
        })?;
        let res = alloc::string::String::from(s);
        if is_smap_enabled() {
            clac();
        }
        Ok(res)
    }
}
