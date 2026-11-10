// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: x86-64 CPUID Brand String & Vendor Identification
// Date: 2026-11-09

//! CPUID-based CPU identification for x86-64 architecture.

use core::arch::x86_64::__cpuid;

/// Identified CPU brand or vendor information.
pub struct CpuInfo {
    pub brand: [u8; 49], // 48-byte ASCII string + guaranteed null terminator
    pub len: usize,
}

impl CpuInfo {
    /// Returns a safe string slice of the CPU brand name.
    pub fn as_str(&self) -> &str {
        match core::str::from_utf8(&self.brand[..self.len]) {
            Ok(s) => s.trim(),
            Err(_) => "Unknown x86-64 CPU",
        }
    }
}

/// Identifies the CPU by executing CPUID instructions.
/// Prioritizes the Extended Brand String (CPUID 0x80000002..=0x80000004).
/// Falls back to the basic Vendor ID (CPUID 0x0) if extended leaves are unavailable.
pub fn get_cpu_info() -> CpuInfo {
    let mut info = CpuInfo {
        brand: [0u8; 49],
        len: 0,
    };

    // Check if extended CPUID functions are available
    let ext_leaf = __cpuid(0x8000_0000);
    if ext_leaf.eax >= 0x8000_0004 {
        let mut idx = 0;
        for leaf in 0x8000_0002..=0x8000_0004 {
            let res = __cpuid(leaf);
            for reg in [res.eax, res.ebx, res.ecx, res.edx] {
                let bytes = reg.to_le_bytes();
                for b in bytes {
                    if idx < 48 {
                        info.brand[idx] = b;
                        idx += 1;
                    }
                }
            }
        }

        // Trim trailing zeros or spaces
        while idx > 0 && (info.brand[idx - 1] == 0 || info.brand[idx - 1] == b' ') {
            idx -= 1;
        }
        info.len = idx;
        return info;
    }

    // Fallback: Vendor ID from CPUID leaf 0
    let base = __cpuid(0);
    let mut idx = 0;
    for reg in [base.ebx, base.edx, base.ecx] {
        let bytes = reg.to_le_bytes();
        for b in bytes {
            if idx < 48 {
                info.brand[idx] = b;
                idx += 1;
            }
        }
    }
    info.len = idx;
    info
}
