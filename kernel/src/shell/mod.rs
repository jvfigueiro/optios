// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Interactive Diagnostic Shell (opti-sh)
// Date: 2026-11-10

//! Interactive Diagnostic Shell (`opti-sh`) for OptiOS.
//! Provides interactive command-line diagnostics, line editing, command history,
//! and integration with VFS, AHCI, PCI, SMBIOS, PMM, and Kernel Heap telemetry.

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use crate::arch::x86_64::cpuid::get_cpu_info;
use crate::arch::x86_64::pit;
use crate::drivers::ahci;
use crate::drivers::keyboard::KeyEvent;
use crate::drivers::pci;
use crate::drivers::video::framebuffer::FramebufferDriver;
use crate::firmware::smbios;
use crate::fs::vfs;
use crate::info;
use crate::mm::{heap, pmm};

pub const PROMPT: &str = "optios# ";
pub const MAX_SCROLLBACK_LINES: usize = 120;

/// Interactive diagnostic shell instance.
pub struct Shell {
    pub input_buffer: String,
    pub last_command: String,
    pub scrollback: Vec<String>,
}

impl Shell {
    pub fn new() -> Self {
        let mut sh = Self {
            input_buffer: String::new(),
            last_command: String::new(),
            scrollback: Vec::new(),
        };
        sh.scrollback
            .push(String::from("OptiOS Diagnostic Shell (opti-sh v0.6.0)"));
        sh.scrollback.push(String::from(
            "Type 'help' for a list of diagnostic commands, or switch tabs with F1..F5.",
        ));
        sh.scrollback.push(String::new());
        sh
    }

    /// Handles keyboard events when the Shell tab is active.
    pub fn handle_key(&mut self, key: KeyEvent) {
        match key {
            KeyEvent::Char(c) => {
                if self.input_buffer.len() < 80 {
                    self.input_buffer.push(c);
                }
            }
            KeyEvent::Backspace => {
                self.input_buffer.pop();
            }
            KeyEvent::ArrowUp => {
                if !self.last_command.is_empty() {
                    self.input_buffer = self.last_command.clone();
                }
            }
            KeyEvent::Enter => {
                let cmd_line = self.input_buffer.clone();
                self.input_buffer.clear();
                self.execute_line(&cmd_line);
            }
            _ => {}
        }
    }

    /// Appends a line to the shell scrollback buffer.
    pub fn println(&mut self, line: &str) {
        self.scrollback.push(String::from(line));
        if self.scrollback.len() > MAX_SCROLLBACK_LINES {
            self.scrollback.remove(0);
        }
    }

    /// Parses and executes a command line.
    pub fn execute_line(&mut self, line: &str) {
        let prompt_line = format!("{}{}", PROMPT, line);
        self.println(&prompt_line);

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
            "clear" => self.scrollback.clear(),
            "uptime" => self.cmd_uptime(),
            _ => {
                let err_msg = format!(
                    "opti-sh: {}: command not found. Type 'help' for available commands.",
                    cmd
                );
                self.println(&err_msg);
            }
        }
    }

    fn cmd_help(&mut self) {
        self.println("Available opti-sh commands:");
        self.println("  help            - Display this list of available commands");
        self.println("  uname [-a]      - Display kernel name, architecture and build date");
        self.println("  ver             - Alias for 'uname'");
        self.println("  ls [path]       - List files and directories in VFS (default: /)");
        self.println("  cat <path>      - Read and display file contents from VFS");
        self.println("  stat <path>     - Display POSIX metadata for a VFS node");
        self.println("  lspci           - Enumerate PCI/PCIe hardware inventory");
        self.println("  lsblk           - List AHCI disks and discovered MBR/GPT partitions");
        self.println("  smart [port]    - Display S.M.A.R.T. health telemetry for an AHCI port");
        self.println("  free / meminfo  - Display real-time PMM and Kernel Heap usage statistics");
        self.println("  smbios          - Display BIOS/UEFI firmware and system motherboard info");
        self.println("  clear           - Clear terminal scrollback buffer");
        self.println("  uptime          - Display system uptime and PIT timer tick count");
    }

    fn cmd_uname(&mut self, _args: &[&str]) {
        self.println("OptiOS 0.6.0-alpha (x86_64 UEFI) #1 Tue Nov 10 2026 x86_64 GNU/None");
    }

    fn cmd_ls(&mut self, args: &[&str]) {
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

    fn cmd_cat(&mut self, args: &[&str]) {
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

    fn cmd_stat(&mut self, args: &[&str]) {
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

    fn cmd_lspci(&mut self) {
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

    fn cmd_lsblk(&mut self) {
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

    fn cmd_smart(&mut self, args: &[&str]) {
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

    fn cmd_free(&mut self) {
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

    fn cmd_smbios(&mut self) {
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

    fn cmd_uptime(&mut self) {
        let uptime_s = pit::uptime_seconds();
        let ticks = pit::ticks();
        self.println(&format!(
            "up {} seconds ({} PIT timer ticks elapsed at 100 Hz)",
            uptime_s, ticks
        ));
    }

    /// Renders the interactive shell inside the specified bounding box.
    pub fn render(&self, fb: &mut FramebufferDriver, x: usize, y: usize, w: usize, h: usize) {
        let term_bg = 0x000a0f1d;
        let border_color = 0x0038bdf8;
        let text_dim = 0x0094a3b8;
        let text_white = 0x00f8fafc;
        let text_green = 0x004ade80;
        let title_bg = 0x001e293b;

        // Terminal background and border
        fb.fill_rect(x, y, w, h, term_bg);
        fb.draw_rect_border(x, y, w, h, 2, border_color);

        // Title bar
        fb.fill_rect(x + 2, y + 2, w - 4, 24, title_bg);
        fb.draw_string(
            x + 12,
            y + 6,
            "OptiOS Diagnostic Terminal (opti-sh v0.6.0)",
            text_white,
            Some(title_bg),
        );
        fb.draw_string(
            x + w - 260,
            y + 6,
            "[F1..F5: Tabs | Up: History]",
            text_dim,
            Some(title_bg),
        );

        let content_y = y + 32;
        let content_h = h.saturating_sub(64);
        let line_height = 18;
        let max_visible = content_h / line_height;

        let total_lines = self.scrollback.len();
        let start_line = if total_lines > max_visible {
            total_lines - max_visible
        } else {
            0
        };

        // Render scrollback lines
        for (i, line_idx) in (start_line..total_lines).enumerate() {
            let ly = content_y + (i * line_height);
            let line_str = &self.scrollback[line_idx];
            if line_str.starts_with(PROMPT) {
                fb.draw_string(x + 10, ly, PROMPT, text_green, Some(term_bg));
                fb.draw_string(
                    x + 10 + PROMPT.len() * 8,
                    ly,
                    &line_str[PROMPT.len()..],
                    text_white,
                    Some(term_bg),
                );
            } else if line_str.starts_with("Available")
                || line_str.starts_with("===")
                || line_str.starts_with("NAME")
            {
                fb.draw_string(x + 10, ly, line_str, 0x00facc15, Some(term_bg));
            } else if line_str.starts_with("  File:") || line_str.starts_with("  Size:") {
                fb.draw_string(x + 10, ly, line_str, 0x0038bdf8, Some(term_bg));
            } else {
                fb.draw_string(x + 10, ly, line_str, text_dim, Some(term_bg));
            }
        }

        // Render prompt and active input buffer at the bottom
        let prompt_y = y + h - 26;
        fb.fill_rect(x + 2, prompt_y - 2, w - 4, 24, 0x000f172a);
        fb.draw_string(x + 10, prompt_y, PROMPT, text_green, Some(0x000f172a));

        let input_x = x + 10 + (PROMPT.len() * 8);
        fb.draw_string(
            input_x,
            prompt_y,
            &self.input_buffer,
            text_white,
            Some(0x000f172a),
        );

        // Cursor
        let cursor_x = input_x + (self.input_buffer.len() * 8);
        fb.fill_rect(cursor_x, prompt_y, 8, 16, text_green);
    }
}
