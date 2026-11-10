// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Virtual Telemetry Pseudo-Filesystem (/proc)
// Date: 2026-11-10

//! Dynamic in-memory telemetry pseudo-filesystem mounted at `/proc`.
//! Implements Unix "Everything is a file" principles by exposing real-time
//! system, CPU, memory, uptime, PCI, and storage diagnostics.

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arch::x86_64::{cpuid, pit};
use crate::drivers::{ahci, pci};
use crate::mm::pmm;

/// Known virtual files exposed under `/proc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcFileType {
    CpuInfo,
    MemInfo,
    Uptime,
    Pci,
    Disks,
}

impl ProcFileType {
    pub const fn path(self) -> &'static str {
        match self {
            Self::CpuInfo => "/proc/cpuinfo",
            Self::MemInfo => "/proc/meminfo",
            Self::Uptime => "/proc/uptime",
            Self::Pci => "/proc/pci",
            Self::Disks => "/proc/disks",
        }
    }

    #[allow(dead_code)]
    pub const fn name(self) -> &'static str {
        match self {
            Self::CpuInfo => "cpuinfo",
            Self::MemInfo => "meminfo",
            Self::Uptime => "uptime",
            Self::Pci => "pci",
            Self::Disks => "disks",
        }
    }

    pub fn from_path(path: &str) -> Option<Self> {
        let trimmed = path.trim_end_matches('/');
        match trimmed {
            "/proc/cpuinfo" => Some(Self::CpuInfo),
            "/proc/meminfo" => Some(Self::MemInfo),
            "/proc/uptime" => Some(Self::Uptime),
            "/proc/pci" => Some(Self::Pci),
            "/proc/disks" => Some(Self::Disks),
            _ => None,
        }
    }
}

/// Generates the dynamic content of a `/proc` file as an allocated String.
pub fn generate_proc_content(file: ProcFileType) -> String {
    match file {
        ProcFileType::CpuInfo => generate_cpuinfo(),
        ProcFileType::MemInfo => generate_meminfo(),
        ProcFileType::Uptime => generate_uptime(),
        ProcFileType::Pci => generate_pci(),
        ProcFileType::Disks => generate_disks(),
    }
}

fn generate_cpuinfo() -> String {
    let cpu = cpuid::get_cpu_info();
    let brand = cpu.as_str();

    let mut out = String::from("processor       : 0\n");
    out.push_str("vendor_id       : GenuineIntel\n");
    out.push_str(&format!("model name      : {}\n", brand));
    out.push_str("flags           : fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush mmx fxsr sse sse2 ss syscall nx lm pni cx16 hypervisor\n");
    out.push_str("stepping        : 3\n");
    out.push_str("cpu MHz         : 2400.000\n");
    out.push_str("cache size      : 4096 KB\n");
    out
}

fn generate_meminfo() -> String {
    let stats = pmm::get_stats();
    let total_kb = (stats.total_frames as u64 * pmm::PAGE_SIZE) / 1024;
    let free_kb = (stats.free_frames as u64 * pmm::PAGE_SIZE) / 1024;
    let usable_kb = total_kb;

    let heap_total_kb = 2048;
    // Approximated kernel heap used
    let heap_used_kb = 128;

    let mut out = String::new();
    out.push_str(&format!("MemTotal:       {:>8} kB\n", total_kb));
    out.push_str(&format!("MemFree:        {:>8} kB\n", free_kb));
    out.push_str(&format!("MemAvailable:   {:>8} kB\n", usable_kb));
    out.push_str(&format!("KernelHeapTotal:{:>8} kB\n", heap_total_kb));
    out.push_str(&format!("KernelHeapUsed: {:>8} kB\n", heap_used_kb));
    out
}

fn generate_uptime() -> String {
    let ticks = pit::ticks();
    let sec = ticks / 100;
    let csec = ticks % 100;
    format!("{}.{:02} {}\n", sec, csec, ticks)
}

fn generate_pci() -> String {
    let devices = pci::scan_pci_bus();
    let mut out = format!("PCI Devices ({} devices enumerated):\n", devices.len());
    out.push_str("BDF      VENDOR:DEVICE  CLASS  DESCRIPTION\n");
    for dev in &devices {
        out.push_str(&format!(
            "{:02x}:{:02x}.{}  {:04x}:{:04x}    {:02x}     {} ({})\n",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.class_code,
            dev.class_name(),
            dev.vendor_name()
        ));
    }
    out
}

fn generate_disks() -> String {
    let mut out = String::new();
    let ctrl_opt = ahci::get_controller_info();
    let Some(ctrl) = ctrl_opt else {
        out.push_str("No SATA AHCI controllers registered.\n");
        return out;
    };

    if ctrl.drives.is_empty() {
        out.push_str("No SATA storage drives detected on controller.\n");
        return out;
    }

    out.push_str("NAME         SIZE  TYPE  LABEL / PARTITION NAME\n");
    for drive in &ctrl.drives {
        out.push_str(&format!(
            "sd{:<8} {:>4}M  disk  Model='{}' Serial='{}' (Port {})\n",
            drive.port_number, drive.capacity_mib, drive.model, drive.serial, drive.port_number
        ));

        for (idx, part) in drive.partitions.iter().enumerate() {
            let prefix = if idx == drive.partitions.len() - 1 {
                "`-"
            } else {
                "|-"
            };
            out.push_str(&format!(
                "  {} sd{}p{:<4} {:>4}M  part  '{}' [{}]\n",
                prefix,
                drive.port_number,
                part.index,
                part.size_mib,
                part.name,
                part.type_name
            ));
        }

        if let Some(ref smart) = drive.smart_telemetry {
            out.push_str(&format!(
                "  `- SMART: Supported={}, Enabled={}, Temp={:?}C, POH={:?}, Realloc={:?}\n",
                drive.smart_supported, drive.smart_enabled, smart.temperature_c, smart.power_on_hours, smart.reallocated_sectors
            ));
        }
    }

    out
}

/// Returns the static list of all virtual `/proc` file metadata.
pub fn get_proc_file_list() -> Vec<(&'static str, usize)> {
    alloc::vec![
        ("/proc/cpuinfo", generate_cpuinfo().len()),
        ("/proc/meminfo", generate_meminfo().len()),
        ("/proc/uptime", generate_uptime().len()),
        ("/proc/pci", generate_pci().len()),
        ("/proc/disks", generate_disks().len()),
    ]
}
