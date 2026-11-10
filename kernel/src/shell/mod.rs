// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Fullscreen Interactive Diagnostic Terminal (opti-sh)
// Date: 2026-11-10

//! Fullscreen Interactive Diagnostic Terminal (`opti-sh`) for OptiOS.
//! Provides command-line hardware diagnostics, line editing, command history,
//! execution of built-in inspection commands, and execution of Ring 3 ELF64 userland binaries.

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arch::x86_64::cpuid::get_cpu_info;
use crate::arch::x86_64::pit;
use crate::drivers::ahci;
use crate::drivers::keyboard::KeyEvent;
use crate::drivers::pci;
use crate::drivers::video::console;
use crate::firmware::smbios;
use crate::fs::vfs;
use crate::info;
use crate::mm::{heap, pmm};

pub const PROMPT: &str = "optios - # ";

/// Interactive terminal shell state.
pub struct Shell {
    pub input_buffer: String,
    pub last_command: String,
}

impl Shell {
    pub fn new() -> Self {
        Self {
            input_buffer: String::new(),
            last_command: String::new(),
        }
    }

    /// Initializes the shell terminal session with the MOTD and prompt.
    pub fn init_session(&self) {
        console::print_str("\nOptiOS 1.0\n");
        console::print_str("Copyright (C) 2026 Joao Figueiro. All Rights Reserved.\n\n");
        console::print_str(PROMPT);
    }

    /// Handles keyboard events from the PS/2 ring buffer.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key {
            KeyEvent::Char(c) => {
                if self.input_buffer.len() < 120 {
                    self.input_buffer.push(c);
                    let mut s = [0u8; 4];
                    let enc = c.encode_utf8(&mut s);
                    console::print_str(enc);
                }
            }
            KeyEvent::Backspace => {
                if self.input_buffer.pop().is_some() {
                    console::print_str("\x08");
                }
            }
            KeyEvent::ArrowUp => {
                if !self.last_command.is_empty() {
                    // Erase current line in console
                    for _ in 0..self.input_buffer.len() {
                        console::print_str("\x08");
                    }
                    self.input_buffer = self.last_command.clone();
                    console::print_str(&self.input_buffer);
                }
            }
            KeyEvent::Enter => {
                console::print_str("\n");
                let cmd_line = core::mem::take(&mut self.input_buffer);
                self.execute_line(&cmd_line);
                console::print_str(PROMPT);
            }
            _ => {}
        }
    }

    /// Prints a line of text to the fullscreen console and serial port.
    pub fn println(&self, line: &str) {
        console::print_str(line);
        console::print_str("\n");
    }

    /// Parses and executes a command line.
    pub fn execute_line(&mut self, line: &str) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return;
        }

        self.last_command = String::from(trimmed);
        info!("opti-sh: Executing command '{}'", trimmed);

        let mut parts = trimmed.split_whitespace();
        let cmd = parts.next().unwrap_or("");
        let args: Vec<&str> = parts.collect();

        match cmd {
            "help" => self.cmd_help(),
            "uname" | "ver" => self.cmd_uname(&args),
            "ls" => self.cmd_ls(&args),
            "cat" => self.cmd_cat(&args),
            "stat" => self.cmd_stat(&args),
            "lspci" => self.cmd_lspci(),
            "lsblk" => self.cmd_lsblk(),
            "smart" => self.cmd_smart(&args),
            "free" | "meminfo" => self.cmd_free(),
            "smbios" => self.cmd_smbios(),
            "dmesg" => self.cmd_dmesg(),
            "clear" => console::clear(),
            "uptime" => self.cmd_uptime(),
            _ => self.cmd_external(cmd, &args),
        }
    }

    fn cmd_help(&self) {
        self.println("Available opti-sh commands:");
        self.println("  help            - Display this list of available commands");
        self.println("  uname [-a]      - Display kernel release, architecture and build info");
        self.println("  ver             - Alias for 'uname'");
        self.println("  ls [path]       - List files and directories in VFS (default: /)");
        self.println("  cat <path>      - Read and display file contents from VFS");
        self.println("  stat <path>     - Display POSIX metadata for a VFS node");
        self.println("  lspci           - Enumerate PCI/PCIe hardware inventory");
        self.println("  lsblk           - List AHCI disks and discovered MBR/GPT partitions");
        self.println("  smart [port]    - Display S.M.A.R.T. health telemetry for an AHCI port");
        self.println("  free / meminfo  - Display real-time PMM and Kernel Heap usage statistics");
        self.println("  smbios          - Display BIOS/UEFI firmware and system motherboard info");
        self.println("  dmesg           - Display kernel boot log and hardware detection messages");
        self.println("  clear           - Clear terminal screen");
        self.println("  uptime          - Display system uptime and PIT timer tick count");
        self.println("  <prog>          - Execute userland ELF64 executable from VFS (e.g. sysinfo)");
    }

    fn cmd_uname(&self, _args: &[&str]) {
        self.println("OptiOS optios 1.0.0 Milestone 7 (Ring 3 Userland) x86_64 2026-11-10");
    }

    fn cmd_dmesg(&self) {
        let logs = console::get_dmesg();
        for line in logs {
            self.println(&line);
        }
    }

    fn cmd_ls(&self, args: &[&str]) {
        let path = if !args.is_empty() { args[0] } else { "/" };
        match vfs::list_dir(path) {
            Ok(entries) => {
                if entries.is_empty() {
                    self.println("(empty directory)");
                } else {
                    for entry in entries {
                        let line = format!(
                            "[{}] {:>8} bytes | mode {:04o} | {}",
                            entry.file_type.as_str(),
                            entry.size,
                            entry.mode,
                            entry.path
                        );
                        self.println(&line);
                    }
                }
            }
            Err(e) => {
                let err = format!("ls: cannot access '{}': {}", path, e);
                self.println(&err);
            }
        }
    }

    fn cmd_cat(&self, args: &[&str]) {
        if args.is_empty() {
            self.println("cat: missing file operand");
            return;
        }
        let path = args[0];
        match vfs::open(path) {
            Ok(fd) => {
                let mut buf = alloc::vec![0u8; 4096];
                match vfs::read(fd, 0, &mut buf) {
                    Ok(bytes_read) => {
                        if let Ok(content) = core::str::from_utf8(&buf[..bytes_read]) {
                            for line in content.lines() {
                                self.println(line);
                            }
                        } else {
                            self.println("(binary file content omitted)");
                        }
                    }
                    Err(e) => {
                        let err = format!("cat: error reading '{}': {}", path, e);
                        self.println(&err);
                    }
                }
            }
            Err(e) => {
                let err = format!("cat: '{}': {}", path, e);
                self.println(&err);
            }
        }
    }

    fn cmd_stat(&self, args: &[&str]) {
        if args.is_empty() {
            self.println("stat: missing file operand");
            return;
        }
        let path = args[0];
        match vfs::stat(path) {
            Ok(st) => {
                self.println(&format!("  File: {}", st.path));
                self.println(&format!("  Size: {} bytes", st.size));
                self.println(&format!("  Type: {}", st.file_type.as_str()));
                self.println(&format!("  Mode: {:04o}", st.mode));
            }
            Err(e) => {
                let err = format!("stat: cannot stat '{}': {}", path, e);
                self.println(&err);
            }
        }
    }

    fn cmd_lspci(&self) {
        let devs = pci::scan_pci_bus();
        self.println(&format!(
            "PCI Scanner: {} device(s) enumerated:",
            devs.len()
        ));
        for d in devs {
            let line = format!(
                "[{:02x}:{:02x}.{}] ID: {:04x}:{:04x} | {} ({})",
                d.bus,
                d.device,
                d.function,
                d.vendor_id,
                d.device_id,
                d.class_name(),
                d.vendor_name()
            );
            self.println(&line);
        }
    }

    fn cmd_lsblk(&self) {
        let ctrl_opt = ahci::get_controller_info();
        let Some(ctrl) = ctrl_opt else {
            self.println("lsblk: AHCI controller not found");
            return;
        };

        if ctrl.drives.is_empty() {
            self.println("lsblk: No SATA drives detected");
            return;
        }

        self.println("NAME         SIZE  TYPE  LABEL / PARTITION NAME");
        for drive in &ctrl.drives {
            let disk_line = format!(
                "sd{:<8} {:>4}M  disk  Model='{}' (Port {})",
                drive.port_number, drive.capacity_mib, drive.model, drive.port_number
            );
            self.println(&disk_line);

            if drive.partitions.is_empty() {
                self.println("  `- (no partition table)");
            } else {
                for (idx, part) in drive.partitions.iter().enumerate() {
                    let branch = if idx == drive.partitions.len() - 1 {
                        "`-"
                    } else {
                        "|-"
                    };
                    let part_name = format!("sd{}p{}", drive.port_number, part.index);
                    let part_line = format!(
                        "  {} {:<6} {:>4}M  part  '{}' [{}]",
                        branch, part_name, part.size_mib, part.name, part.type_name
                    );
                    self.println(&part_line);
                }
            }
        }
    }

    fn cmd_smart(&self, args: &[&str]) {
        let target_port: u8 = if !args.is_empty() {
            args[0].parse().unwrap_or(0)
        } else {
            0
        };

        let ctrl_opt = ahci::get_controller_info();
        let Some(ctrl) = ctrl_opt else {
            self.println("smart: AHCI controller not found");
            return;
        };

        let drive_opt = ctrl.drives.iter().find(|d| d.port_number == target_port);
        let Some(drive) = drive_opt else {
            self.println(&format!(
                "smart: No drive found on AHCI Port {}",
                target_port
            ));
            return;
        };

        self.println(&format!(
            "=== S.M.A.R.T. TELEMETRY: Port {} ('{}') ===",
            target_port, drive.model
        ));
        self.println(&format!("Serial:               {}", drive.serial));
        self.println(&format!("Firmware Revision:    {}", drive.firmware));
        self.println(&format!(
            "LBA48 Total Sectors:  {} ({} MiB)",
            drive.total_sectors_lba48, drive.capacity_mib
        ));
        self.println(&format!(
            "S.M.A.R.T. Supported: {}",
            drive.smart_supported
        ));
        self.println(&format!(
            "S.M.A.R.T. Enabled:   {}",
            drive.smart_enabled
        ));

        if let Some(ref tel) = drive.smart_telemetry {
            self.println(&format!("Data Structure Rev:   {}", tel.revision));
            let temp_str = tel
                .temperature_c
                .map(|t| format!("{} C", t))
                .unwrap_or_else(|| String::from("N/A"));
            let poh_str = tel
                .power_on_hours
                .map(|h| format!("{} hours", h))
                .unwrap_or_else(|| String::from("N/A"));
            let cyc_str = tel
                .power_cycles
                .map(|c| format!("{}", c))
                .unwrap_or_else(|| String::from("N/A"));
            let real_str = tel
                .reallocated_sectors
                .map(|r| format!("{}", r))
                .unwrap_or_else(|| String::from("0"));

            self.println(&format!("Drive Temperature:    {}", temp_str));
            self.println(&format!("Power-On Hours:       {}", poh_str));
            self.println(&format!("Power Cycles Count:   {}", cyc_str));
            self.println(&format!("Reallocated Sectors:  {}", real_str));
            self.println(&format!(
                "Attributes Collected: {} attribute(s)",
                tel.attributes.len()
            ));
        } else {
            self.println("S.M.A.R.T. raw attribute table not available for this port.");
        }
    }

    fn cmd_free(&self) {
        let stats = pmm::get_stats();
        let total_kib = (stats.total_frames * 4096) / 1024;
        let free_kib = (stats.free_frames * 4096) / 1024;
        let used_kib = (stats.used_frames * 4096) / 1024;

        let (heap_used, heap_total) = heap::get_stats();
        let heap_free = heap_total.saturating_sub(heap_used);

        self.println("               total        used        free");
        self.println(&format!(
            "Mem (PMM):     {:>7} KiB  {:>7} KiB  {:>7} KiB ({} frames free)",
            total_kib, used_kib, free_kib, stats.free_frames
        ));
        self.println(&format!(
            "Heap (Alloc):  {:>7} KiB  {:>7} KiB  {:>7} KiB",
            heap_total / 1024,
            heap_used / 1024,
            heap_free / 1024
        ));
    }

    fn cmd_smbios(&self) {
        let smbios_opt = smbios::get_info();
        let Some(smbios_info) = smbios_opt else {
            self.println("smbios: SMBIOS table not detected");
            return;
        };

        self.println(&format!(
            "SMBIOS Table: Version {}.{}",
            smbios_info.major_version, smbios_info.minor_version
        ));
        self.println(&format!("BIOS Vendor:  {}", smbios_info.bios_vendor));
        self.println(&format!("BIOS Version: {}", smbios_info.bios_version));
        self.println(&format!(
            "Release Date: {}",
            smbios_info.bios_release_date
        ));
        self.println(&format!(
            "System Mfr:   {}",
            smbios_info.sys_manufacturer
        ));
        self.println(&format!(
            "Product Name: {}",
            smbios_info.sys_product_name
        ));
        let cpu_info = get_cpu_info();
        self.println(&format!("Processor:    {}", cpu_info.as_str()));
    }

    fn cmd_uptime(&self) {
        let uptime_s = pit::uptime_seconds();
        let ticks = pit::ticks();
        self.println(&format!(
            "up {} seconds ({} PIT timer ticks elapsed at 100 Hz)",
            uptime_s, ticks
        ));
    }

    fn cmd_external(&self, cmd: &str, _args: &[&str]) {
        let target_path = if cmd.starts_with('/') {
            String::from(cmd)
        } else {
            format!("/bin/{}", cmd)
        };

        if vfs::open(&target_path).is_ok() {
            self.println(&format!(
                "[opti-sh] Executing ELF64 binary '{}' in Ring 3 (Userland)...",
                target_path
            ));
            match crate::syscall::loader::spawn_user_process(&target_path) {
                Ok(status) => {
                    if status != 0 {
                        self.println(&format!("[opti-sh] Process exited with status {}", status));
                    }
                }
                Err(err) => {
                    self.println(&format!("opti-sh: failed to execute '{}': {}", target_path, err));
                }
            }
        } else {
            self.println(&format!(
                "opti-sh: {}: command not found. Type 'help' for available commands.",
                cmd
            ));
        }
    }
}
