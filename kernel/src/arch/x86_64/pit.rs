// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: 8253/8254 Programmable Interval Timer (PIT) 100 Hz Driver
// Date: 2026-11-09

//! 8253/8254 Programmable Interval Timer (PIT) driver.
//! Configured at 100 Hz (10 ms per tick) on Channel 0 (IRQ 0 / Vector 32).

use core::sync::atomic::{AtomicU64, Ordering};
use crate::arch::x86_64::io::outb;
use crate::info;

const PIT_CHANNEL0_DATA: u16 = 0x40;
const PIT_CMD_PORT: u16 = 0x43;
const PIT_BASE_FREQUENCY: u32 = 1_193_182;

pub const TIMER_HZ: u32 = 100; // 100 ticks per second (10 ms resolution)

static TICKS: AtomicU64 = AtomicU64::new(0);

/// Configures PIT Channel 0 to generate periodic interrupts at `TIMER_HZ` (100 Hz).
pub fn init() {
    let divisor = (PIT_BASE_FREQUENCY / TIMER_HZ) as u16;

    unsafe {
        // Channel 0, lobyte/hibyte access, Mode 3 (Square Wave Generator), Binary
        outb(PIT_CMD_PORT, 0x36);
        outb(PIT_CHANNEL0_DATA, (divisor & 0xFF) as u8);
        outb(PIT_CHANNEL0_DATA, (divisor >> 8) as u8);
    }

    info!(
        "PIT Timer initialized: {} Hz ({} ms tick interval, divisor {}).",
        TIMER_HZ,
        1000 / TIMER_HZ,
        divisor
    );
}

/// Called by the IRQ 0 interrupt handler on every timer tick.
#[inline]
pub fn on_tick() {
    TICKS.fetch_add(1, Ordering::Relaxed);
}

/// Returns the total number of PIT ticks elapsed since timer initialization.
#[inline]
pub fn ticks() -> u64 {
    TICKS.load(Ordering::Relaxed)
}

/// Returns the system uptime in milliseconds.
#[inline]
pub fn uptime_ms() -> u64 {
    ticks() * (1000 / TIMER_HZ as u64)
}

/// Returns the system uptime in whole seconds.
#[inline]
pub fn uptime_seconds() -> u64 {
    ticks() / (TIMER_HZ as u64)
}
