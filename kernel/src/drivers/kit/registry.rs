// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Unified Driver Registry & Diagnostic Introspection
// Date: 2026-11-10

//! Central registry of hardware drivers and subsystem diagnostic providers.
//! Tracks provenance, licensing, operational status, and health counters.

extern crate alloc;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

/// Provenance origin of a device driver.
#[derive(Debug, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DriverOrigin {
    /// Developed specifically for OptiOS.
    Native,
    /// Imported from an upstream project (e.g. "FreeBSD 14.1", "Linux 6.6").
    Imported(&'static str),
}

/// Metadata describing a driver registered with the kernel.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct DriverMetadata {
    pub name: &'static str,
    pub spdx_license: &'static str,
    pub origin: DriverOrigin,
    pub version: &'static str,
}

/// Trait implemented by drivers providing live diagnostic telemetry.
pub trait Diagnosable: Send + Sync {
    /// Short health status indicator (e.g. "OK", "Degraded", "Failed", "Idle").
    fn health_status(&self) -> &'static str;
    /// Detailed diagnostic metrics and operational counters.
    fn diagnostics(&self) -> String;
}

/// Entry stored in the central driver registry.
pub struct RegisteredDriver {
    pub meta: DriverMetadata,
    pub diagnostic: Option<&'static dyn Diagnosable>,
}

static REGISTRY_LOCK: AtomicBool = AtomicBool::new(false);
static mut REGISTERED_DRIVERS: Vec<RegisteredDriver> = Vec::new();

/// Registers a driver in the kernel inventory.
pub fn register_driver(meta: DriverMetadata, diagnostic: Option<&'static dyn Diagnosable>) {
    while REGISTRY_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        let list_ptr = &raw mut REGISTERED_DRIVERS;
        (*list_ptr).push(RegisteredDriver { meta, diagnostic });
    }

    REGISTRY_LOCK.store(false, Ordering::Release);
}

/// Formats the current registry into a tabular `/proc/drivers` text document.
pub fn format_proc_drivers() -> String {
    use core::fmt::Write;
    let mut out = String::new();

    let _ = writeln!(out, "{:<14} {:<12} {:<18} {:<8} {}", "DRIVER", "ORIGIN", "LICENSE", "STATUS", "DIAGNOSTICS");
    let _ = writeln!(out, "{:<14} {:<12} {:<18} {:<8} {}", "------", "------", "-------", "------", "-----------");

    while REGISTRY_LOCK.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed).is_err() {
        core::hint::spin_loop();
    }

    unsafe {
        let list_ptr = &raw const REGISTERED_DRIVERS;
        for entry in &(*list_ptr) {
            let origin_str = match &entry.meta.origin {
                DriverOrigin::Native => "Native",
                DriverOrigin::Imported(src) => src,
            };
            let status = entry.diagnostic.map(|d| d.health_status()).unwrap_or("OK");
            let diag = entry.diagnostic.map(|d| d.diagnostics()).unwrap_or_else(|| String::from("Running"));

            let _ = writeln!(
                out,
                "{:<14} {:<12} {:<18} {:<8} {}",
                entry.meta.name, origin_str, entry.meta.spdx_license, status, diag
            );
        }
    }

    REGISTRY_LOCK.store(false, Ordering::Release);
    out
}
