// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Time and Timer Abstraction Primitives
// Date: 2026-11-09

//! Time subsystem and polling wait primitives for OptiOS.

use crate::arch::x86_64::pit;
use optios_ahci_proto::AhciError;

/// Returns the current monotonic PIT ticks (10 ms per tick at 100 Hz).
#[inline]
pub fn ticks() -> u64 {
    pit::ticks()
}

/// Returns elapsed time in milliseconds since `start_ticks`.
#[inline]
pub fn elapsed_ms(start_ticks: u64) -> u64 {
    ticks().saturating_sub(start_ticks) * 10
}

/// Checks whether CPU hardware interrupts are enabled (RFLAGS IF bit 9 == 1).
#[inline]
pub fn are_interrupts_enabled() -> bool {
    let rflags: u64;
    unsafe {
        core::arch::asm!(
            "pushfq",
            "pop {}",
            out(reg) rflags,
            options(nomem, preserves_flags)
        );
    }
    (rflags & (1 << 9)) != 0
}

/// Reads the current value of the CPU Time Stamp Counter (TSC).
#[inline]
pub fn read_tsc() -> u64 {
    let lo: u32;
    let hi: u32;
    unsafe {
        core::arch::asm!(
            "rdtsc",
            out("eax") lo,
            out("edx") hi,
            options(nomem, preserves_flags)
        );
    }
    ((hi as u64) << 32) | (lo as u64)
}

/// Waits until `cond` returns true or until `timeout_ms` milliseconds elapse.
/// Spins exclusively using `core::hint::spin_loop()` (x86 `pause`), NEVER `hlt`.
///
/// Ensures:
/// - Hardware interrupts (IF=1) are enabled via `debug_assert!`
/// - Bounded spin stall guard returns `Err(AhciError::TimerStalled)` if ticks do not advance
/// - `timeout_ticks = ceil(timeout_ms / 10) + 1` ensures real elapsed time is never shorter than requested
pub fn wait_until<F>(timeout_ms: u64, mut cond: F) -> Result<(), AhciError>
where
    F: FnMut() -> bool,
{
    debug_assert!(
        are_interrupts_enabled(),
        "time::wait_until requires hardware interrupts (IF=1) enabled for PIT ticks"
    );

    let start = ticks();
    let mut last_tick = start;
    let mut spins_since_tick: u64 = 0;
    const STALL_SPIN_LIMIT: u64 = 20_000_000;

    // timeout_ticks = ceil(timeout_ms / 10) + 1 so real elapsed time is never shorter than requested
    let timeout_ticks = (timeout_ms + 9) / 10 + 1;

    while !cond() {
        let cur = ticks();
        if cur != last_tick {
            last_tick = cur;
            spins_since_tick = 0;
        } else {
            spins_since_tick = spins_since_tick.saturating_add(1);
            if spins_since_tick >= STALL_SPIN_LIMIT {
                return Err(AhciError::TimerStalled);
            }
        }

        if cur.saturating_sub(start) >= timeout_ticks {
            return Err(AhciError::Timeout);
        }
        core::hint::spin_loop();
    }
    Ok(())
}
