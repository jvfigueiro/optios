// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Fastfetch-Style Native System Information Tool (optifetch)
// Date: 2026-11-10

//! Native `optifetch` utility running in unprivileged Ring 3 on OptiOS.
//! Dynamically allocates memory via `sys_brk` (alloc crate enabled), reads hardware
//! and system telemetry from `/etc/optios-release`, `/proc/cpuinfo`, `/proc/meminfo`,
//! `/proc/uptime`, and `/proc/disks`, and renders a side-by-side ANSI color dashboard.

#![no_std]
#![no_main]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::panic::PanicInfo;

use optios_libc::{close, exit, open, print, println, read, uname, OptiAllocator, UtsName};

#[global_allocator]
static ALLOCATOR: OptiAllocator = OptiAllocator::new();

fn read_file(path: &str) -> Option<String> {
    let fd = open(path, 0, 0).ok()?;
    let mut buf = [0u8; 512];
    let mut content = Vec::new();
    loop {
        match read(fd, &mut buf) {
            Ok(0) => break,
            Ok(n) => content.extend_from_slice(&buf[..n]),
            Err(_) => break,
        }
    }
    let _ = close(fd);
    String::from_utf8(content).ok()
}

fn parse_line_value<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    for line in text.lines() {
        if line.starts_with(key) {
            let mut parts = line.splitn(2, ':');
            let _k = parts.next()?;
            let v = parts.next()?;
            return Some(v.trim());
        }
    }
    None
}

#[no_mangle]
pub extern "C" fn _start() -> ! {
    let mut uts = UtsName::new();
    let _ = uname(&mut uts);

    // Read telemetry from virtual pseudo-filesystem /proc
    let cpuinfo = read_file("/proc/cpuinfo").unwrap_or_default();
    let meminfo = read_file("/proc/meminfo").unwrap_or_default();
    let uptime_raw = read_file("/proc/uptime").unwrap_or_default();
    let disks = read_file("/proc/disks").unwrap_or_default();

    // Parse CPU model
    let cpu_model = parse_line_value(&cpuinfo, "model name").unwrap_or("x86-64 CPU");

    // Parse Memory
    let mem_total_kb: u64 = parse_line_value(&meminfo, "MemTotal")
        .and_then(|s| s.trim_end_matches(" kB").parse().ok())
        .unwrap_or(512000);
    let mem_free_kb: u64 = parse_line_value(&meminfo, "MemFree")
        .and_then(|s| s.trim_end_matches(" kB").parse().ok())
        .unwrap_or(500000);
    let mem_used_mib = (mem_total_kb.saturating_sub(mem_free_kb)) / 1024;
    let mem_total_mib = mem_total_kb / 1024;

    // Parse Uptime
    let uptime_sec = uptime_raw
        .split_whitespace()
        .next()
        .unwrap_or("0.00");

    // Parse Disks summary
    let mut storage_summary = String::from("SATA SSD (AHCI Read-Only)");
    for line in disks.lines() {
        if line.starts_with("sd") {
            storage_summary = String::from(line);
            break;
        }
    }

    // ASCII Art Logo (OptiOS stylized geometric symbol)
    let logo: [&str; 11] = [
        "\x1b[1;36m       ______       \x1b[0m",
        "\x1b[1;36m      / ____ \\      \x1b[0m",
        "\x1b[1;36m     / / __ \\ \\     \x1b[0m",
        "\x1b[1;34m    / / /  \\ \\ \\    \x1b[0m",
        "\x1b[1;34m   | | | () | | |   \x1b[0m",
        "\x1b[1;34m    \\ \\ \\__/ / /    \x1b[0m",
        "\x1b[1;36m     \\ \\____/ /     \x1b[0m",
        "\x1b[1;36m      \\______/      \x1b[0m",
        "\x1b[1;97m       OptiOS       \x1b[0m",
        "                    ",
        "                    ",
    ];

    // Info lines
    let info_lines = [
        format!("\x1b[1;36mroot\x1b[0m@\x1b[1;36moptios\x1b[0m"),
        format!("\x1b[1;34m--------------------------------------------------\x1b[0m"),
        format!("\x1b[1;36mOS:\x1b[0m           {} (x86_64 UEFI)", uts.sysname()),
        format!("\x1b[1;36mKernel:\x1b[0m       {}", uts.version()),
        format!("\x1b[1;36mUptime:\x1b[0m       {} seconds", uptime_sec),
        format!("\x1b[1;36mCPU:\x1b[0m          {}", cpu_model),
        format!("\x1b[1;36mMotherboard:\x1b[0m  Q35 / EDK II OVMF UEFI"),
        format!("\x1b[1;36mMemory:\x1b[0m       {} MiB / {} MiB ({}%)", mem_used_mib, mem_total_mib, (mem_used_mib * 100) / mem_total_mib.max(1)),
        format!("\x1b[1;36mStorage:\x1b[0m      {}", storage_summary),
        format!("\x1b[1;36mTerminal:\x1b[0m     /dev/tty0 (160x50 at 1280x800 Double Buffered)"),
        format!(
            "\x1b[1;36mPalette:\x1b[0m      \x1b[40m   \x1b[41m   \x1b[42m   \x1b[43m   \x1b[44m   \x1b[45m   \x1b[46m   \x1b[47m   \x1b[0m \x1b[100m   \x1b[104m   \x1b[106m   \x1b[107m   \x1b[0m"
        ),
    ];

    print!("\n");
    for i in 0..11 {
        let left = logo[i];
        let right = &info_lines[i];
        println!("  {}  {}", left, right);
    }
    print!("\n");

    exit(0);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print!("optifetch: fatal panic encountered!\n");
    exit(127);
}
