// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Kernel Main Entry Point (_start) & Multi-Tab Interactive Diagnostic UI
// Date: 2026-11-09

//! OptiOS Kernel Main Entry Point (_start) — Milestone 5:
//! POSIX Foundation, USTAR Initramfs VFS, Read-Only SATA AHCI Driver & 4-Tab Dashboard.

#![no_std]
#![no_main]

extern crate alloc;

mod arch;
mod boot;
mod drivers;
mod firmware;
mod fs;
mod logger;
mod mm;
mod posix;
pub mod time;

use alloc::string::String;
use alloc::vec::Vec;
use core::panic::PanicInfo;
use arch::{halt_loop, hlt, sti};
use arch::x86_64::cpuid::get_cpu_info;
use arch::x86_64::{gdt, idt, pic, pit};
use boot::limine::{
    get_framebuffer, get_hhdm_offset, get_initramfs_module, get_memory_map,
    get_smbios_entry_points, is_base_revision_supported,
};
use drivers::ahci::{self, AhciControllerInfo};
use drivers::keyboard::{self, KeyEvent};
use drivers::pci::{scan_pci_bus, PciDevice};
use drivers::serial::SERIAL1;
use drivers::video::framebuffer::{init_global_framebuffer, with_framebuffer, FramebufferDriver};
use firmware::smbios::{self, SmbiosInfo};
use fs::vfs::{self, VfsStat};
use limine::memory_map::{Entry, EntryType};
use mm::memmap::{parse_memory_map, MemoryStats};
use mm::{heap, pmm, vmm};

/// Active tab in the interactive diagnostic dashboard.
#[derive(Clone, Copy, PartialEq, Eq)]
enum DashboardTab {
    SystemAndSmbios = 1,
    PciInventory = 2,
    MemoryAndHeap = 3,
    StorageAndVfs = 4,
}

impl DashboardTab {
    fn next(self) -> Self {
        match self {
            Self::SystemAndSmbios => Self::PciInventory,
            Self::PciInventory => Self::MemoryAndHeap,
            Self::MemoryAndHeap => Self::StorageAndVfs,
            Self::StorageAndVfs => Self::SystemAndSmbios,
        }
    }

    fn prev(self) -> Self {
        match self {
            Self::SystemAndSmbios => Self::StorageAndVfs,
            Self::PciInventory => Self::SystemAndSmbios,
            Self::MemoryAndHeap => Self::PciInventory,
            Self::StorageAndVfs => Self::MemoryAndHeap,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Self::SystemAndSmbios => "System & SMBIOS",
            Self::PciInventory => "PCI Hardware Inventory",
            Self::MemoryAndHeap => "Memory Map & Heap Telemetry",
            Self::StorageAndVfs => "Storage (AHCI) & VFS (Initramfs)",
        }
    }
}

/// Converts a u64 value to an ASCII decimal string slice into a fixed-size buffer.
fn u64_to_str<'a>(mut val: u64, buf: &'a mut [u8; 20]) -> &'a str {
    if val == 0 {
        buf[0] = b'0';
        return core::str::from_utf8(&buf[..1]).unwrap_or("0");
    }
    let mut i = 20;
    while val > 0 && i > 0 {
        i -= 1;
        buf[i] = b'0' + (val % 10) as u8;
        val /= 10;
    }
    core::str::from_utf8(&buf[i..]).unwrap_or("?")
}

/// Formats a 64-bit unsigned integer as a hexadecimal ASCII string into a buffer.
fn u64_to_hex<'a>(val: u64, buf: &'a mut [u8; 18]) -> &'a str {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..16 {
        let shift = (15 - i) * 4;
        let nibble = ((val >> shift) & 0x0F) as usize;
        buf[2 + i] = HEX_CHARS[nibble];
    }
    core::str::from_utf8(buf).unwrap_or("0x0000000000000000")
}

/// Formats a 32-bit unsigned integer as an 8-digit hexadecimal ASCII string into a buffer.
fn u32_to_hex8<'a>(val: u32, buf: &'a mut [u8; 10]) -> &'a str {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = b'0';
    buf[1] = b'x';
    for i in 0..8 {
        let shift = (7 - i) * 4;
        let nibble = ((val >> shift) & 0x0F) as usize;
        buf[2 + i] = HEX_CHARS[nibble];
    }
    core::str::from_utf8(buf).unwrap_or("0x00000000")
}

/// Formats a 16-bit unsigned integer as a 4-digit hexadecimal ASCII string into a buffer.
fn u16_to_hex4<'a>(val: u16, buf: &'a mut [u8; 4]) -> &'a str {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    for i in 0..4 {
        let shift = (3 - i) * 4;
        let nibble = ((val >> shift) & 0x0F) as usize;
        buf[i] = HEX_CHARS[nibble];
    }
    core::str::from_utf8(buf).unwrap_or("0000")
}

/// Formats an 8-bit unsigned integer as a 2-digit hexadecimal ASCII string into a buffer.
fn u8_to_hex2<'a>(val: u8, buf: &'a mut [u8; 2]) -> &'a str {
    const HEX_CHARS: &[u8; 16] = b"0123456789ABCDEF";
    buf[0] = HEX_CHARS[((val >> 4) & 0x0F) as usize];
    buf[1] = HEX_CHARS[(val & 0x0F) as usize];
    core::str::from_utf8(buf).unwrap_or("00")
}

/// Renders only the top-right uptime clock widget on the header bar.
fn render_header_clock(fb: &mut FramebufferDriver, ticks: u64) {
    let banner_bg = 0x001e293b;
    let text_white = 0x00f8fafc;
    let text_cyan = 0x0038bdf8;
    let text_green = 0x004ade80;
    let text_dim = 0x0094a3b8;

    let clock_x = fb.width.saturating_sub(420);
    fb.fill_rect(clock_x, 8, 400, 32, banner_bg);

    let secs = pit::uptime_seconds();
    let tenths = (ticks % 100) / 10;

    let mut buf_s = [0u8; 20];
    let mut buf_t = [0u8; 20];
    let mut buf_k = [0u8; 20];
    let str_s = u64_to_str(secs, &mut buf_s);
    let str_t = u64_to_str(tenths, &mut buf_t);
    let str_k = u64_to_str(ticks, &mut buf_k);

    let mut cx = clock_x;
    fb.draw_string(cx, 16, "IRQ0: ", text_dim, Some(banner_bg));
    cx += 6 * 8;
    fb.draw_string(cx, 16, "100Hz", text_green, Some(banner_bg));
    cx += 5 * 8;
    fb.draw_string(cx, 16, " | Uptime: ", text_dim, Some(banner_bg));
    cx += 11 * 8;
    fb.draw_string(cx, 16, str_s, text_cyan, Some(banner_bg));
    cx += str_s.len() * 8;
    fb.draw_string(cx, 16, ".", text_cyan, Some(banner_bg));
    cx += 8;
    fb.draw_string(cx, 16, str_t, text_cyan, Some(banner_bg));
    cx += str_t.len() * 8;
    fb.draw_string(cx, 16, "s (", text_dim, Some(banner_bg));
    cx += 3 * 8;
    fb.draw_string(cx, 16, str_k, text_white, Some(banner_bg));
    cx += str_k.len() * 8;
    fb.draw_string(cx, 16, " ticks)", text_dim, Some(banner_bg));
}

/// Renders the entire dashboard for the selected tab.
#[allow(clippy::too_many_arguments)]
fn render_dashboard(
    fb: &mut FramebufferDriver,
    active_tab: DashboardTab,
    cpu_name: &str,
    cr3_val: u64,
    mem_stats: &MemoryStats,
    pmm_stats: &pmm::PmmStats,
    pmm_test_ok: bool,
    vmm_test_ok: bool,
    heap_test_ok: bool,
    smbios: &SmbiosInfo,
    pci_devices: &[PciDevice],
    mem_entries_opt: Option<&[&Entry]>,
    ahci_info: &AhciControllerInfo,
    vfs_entries: &[VfsStat],
    optios_release_content: &str,
    ticks: u64,
) {
    let bg_color = 0x000f172a;
    let banner_bg = 0x001e293b;
    let accent_color = 0x003b82f6;
    let card_bg = 0x00172033;
    let text_white = 0x00f8fafc;
    let text_cyan = 0x0038bdf8;
    let text_green = 0x004ade80;
    let text_yellow = 0x00fef08a;
    let text_dim = 0x0094a3b8;
    let border_color = 0x00334155;

    // Clear content area (between top header and bottom status bar)
    fb.fill_rect(0, 49, fb.width, fb.height.saturating_sub(97), bg_color);

    // Render Tab Navigation Bar right below header (y = 58..92)
    let nav_y = 58;
    let tabs = [
        (DashboardTab::SystemAndSmbios, "[1 / F1] System & SMBIOS"),
        (DashboardTab::PciInventory, "[2 / F2] PCI Hardware Inventory"),
        (DashboardTab::MemoryAndHeap, "[3 / F3] Memory Map & Heap"),
        (DashboardTab::StorageAndVfs, "[4 / F4] Storage (AHCI) & VFS"),
    ];

    let mut tab_x = 36;
    for (tab, label) in tabs {
        let is_active = tab == active_tab;
        let tab_w = (label.len() * 8) + 28;
        let tab_bg = if is_active { accent_color } else { 0x001e293b };
        let tab_fg = if is_active { text_white } else { text_dim };
        let tab_border = if is_active { text_cyan } else { border_color };

        fb.fill_rect(tab_x, nav_y, tab_w, 32, tab_bg);
        fb.draw_rect_border(tab_x, nav_y, tab_w, 32, 1, tab_border);
        fb.draw_string(tab_x + 14, nav_y + 8, label, tab_fg, Some(tab_bg));

        tab_x += tab_w + 12;
    }

    let box_x = 36;
    let box_w = (fb.width - 72).min(1208);

    match active_tab {
        DashboardTab::SystemAndSmbios => {
            // Top Card: CPU, Interrupts & System Core
            let box_y = 104;
            let box_h = 260;
            fb.fill_rect(box_x, box_y, box_w, box_h, card_bg);
            fb.draw_rect_border(box_x, box_y, box_w, box_h, 2, border_color);

            fb.draw_string(
                box_x + 20,
                box_y + 16,
                "[ CPU ARCHITECTURE, INTERRUPTS (PIC/PIT) & INPUT ]",
                accent_color,
                None,
            );

            let mut ry = box_y + 48;
            fb.draw_string(box_x + 20, ry, "Processor (CPUID):", text_dim, None);
            fb.draw_string(box_x + 200, ry, cpu_name, text_cyan, None);

            ry += 28;
            fb.draw_string(box_x + 20, ry, "CPU Protection:", text_dim, None);
            fb.draw_string(
                box_x + 200,
                ry,
                "Kernel GDT/TSS & IDT Active (Double Fault IST 1 Loaded)",
                text_green,
                None,
            );

            ry += 28;
            fb.draw_string(box_x + 20, ry, "IRQ Controller:", text_dim, None);
            fb.draw_string(
                box_x + 200,
                ry,
                "Dual 8259 PIC Remapped to Vectors 32..47 (Hardware STI Enabled)",
                text_white,
                None,
            );

            ry += 28;
            fb.draw_string(box_x + 20, ry, "System Timer:", text_dim, None);
            fb.draw_string(
                box_x + 200,
                ry,
                "8253/8254 PIT @ 100 Hz (10 ms tick resolution on IRQ 0 / Vec 32)",
                text_yellow,
                None,
            );

            ry += 28;
            fb.draw_string(box_x + 20, ry, "Keyboard Input:", text_dim, None);
            fb.draw_string(
                box_x + 200,
                ry,
                "PS/2 8042 Controller Active on IRQ 1 / Vec 33 (Lock-Free Ring Buffer)",
                text_green,
                None,
            );

            ry += 28;
            fb.draw_string(box_x + 20, ry, "Display Mode:", text_dim, None);
            let mut buf_w = [0u8; 20];
            let mut buf_h = [0u8; 20];
            let str_w = u64_to_str(fb.width as u64, &mut buf_w);
            let str_h = u64_to_str(fb.height as u64, &mut buf_h);
            let mut dx = box_x + 200;
            fb.draw_string(dx, ry, str_w, text_white, None);
            dx += str_w.len() * 8;
            fb.draw_string(dx, ry, "x", text_dim, None);
            dx += 8;
            fb.draw_string(dx, ry, str_h, text_white, None);
            dx += str_h.len() * 8;
            fb.draw_string(dx, ry, " (32 bpp, UEFI GOP | QEMU Q35 Chipset)", text_dim, None);

            // Bottom Card: SMBIOS / DMI Firmware Information
            let sm_y = box_y + box_h + 18;
            let sm_h = 260;
            fb.fill_rect(box_x, sm_y, box_w, sm_h, card_bg);
            fb.draw_rect_border(box_x, sm_y, box_w, sm_h, 2, border_color);

            let mut buf_maj = [0u8; 20];
            let mut buf_min = [0u8; 20];
            let mut buf_cnt = [0u8; 20];
            let s_maj = u64_to_str(smbios.major_version as u64, &mut buf_maj);
            let s_min = u64_to_str(smbios.minor_version as u64, &mut buf_min);
            let s_cnt = u64_to_str(smbios.structure_count as u64, &mut buf_cnt);

            let mut sx = box_x + 20;
            fb.draw_string(sx, sm_y + 16, "[ SMBIOS / DMI FIRMWARE INVENTORY - v", accent_color, None);
            sx += 37 * 8;
            fb.draw_string(sx, sm_y + 16, s_maj, text_yellow, None);
            sx += s_maj.len() * 8;
            fb.draw_string(sx, sm_y + 16, ".", text_yellow, None);
            sx += 8;
            fb.draw_string(sx, sm_y + 16, s_min, text_yellow, None);
            sx += s_min.len() * 8;
            fb.draw_string(sx, sm_y + 16, " (", accent_color, None);
            sx += 2 * 8;
            fb.draw_string(sx, sm_y + 16, s_cnt, text_cyan, None);
            sx += s_cnt.len() * 8;
            fb.draw_string(sx, sm_y + 16, " STRUCTURES) ]", accent_color, None);

            let mut sry = sm_y + 50;
            fb.draw_string(box_x + 20, sry, "BIOS Vendor (Type 0):", text_dim, None);
            fb.draw_string(box_x + 230, sry, &smbios.bios_vendor, text_white, None);

            sry += 28;
            fb.draw_string(box_x + 20, sry, "BIOS Version & Date:", text_dim, None);
            let mut bvx = box_x + 230;
            fb.draw_string(bvx, sry, &smbios.bios_version, text_cyan, None);
            bvx += smbios.bios_version.len() * 8;
            fb.draw_string(bvx, sry, "  |  Release Date: ", text_dim, None);
            bvx += 19 * 8;
            fb.draw_string(bvx, sry, &smbios.bios_release_date, text_yellow, None);

            sry += 28;
            fb.draw_string(box_x + 20, sry, "System Manufacturer:", text_dim, None);
            fb.draw_string(box_x + 230, sry, &smbios.sys_manufacturer, text_white, None);

            sry += 28;
            fb.draw_string(box_x + 20, sry, "System Product (Type 1):", text_dim, None);
            let mut spx = box_x + 230;
            fb.draw_string(spx, sry, &smbios.sys_product_name, text_green, None);
            spx += smbios.sys_product_name.len() * 8;
            fb.draw_string(spx, sry, "  |  Version: ", text_dim, None);
            spx += 14 * 8;
            fb.draw_string(spx, sry, &smbios.sys_version, text_white, None);

            sry += 28;
            fb.draw_string(box_x + 20, sry, "Baseboard (Type 2):", text_dim, None);
            let mut bbx = box_x + 230;
            fb.draw_string(bbx, sry, &smbios.board_manufacturer, text_white, None);
            bbx += smbios.board_manufacturer.len() * 8;
            fb.draw_string(bbx, sry, " - ", text_dim, None);
            bbx += 3 * 8;
            fb.draw_string(bbx, sry, &smbios.board_product, text_cyan, None);

            let foot_y = sm_y + sm_h - 38;
            fb.fill_rect(box_x + 2, foot_y, box_w - 4, 36, 0x000c1322);
            fb.draw_string(
                box_x + 16,
                foot_y + 10,
                "* Milestone 4 Operational: Live Event Loop (STI + HLT) processing PIT & Keyboard IRQs.",
                text_green,
                None,
            );
        }

        DashboardTab::PciInventory => {
            let pci_box_y = 104;
            let pci_box_h = fb.height.saturating_sub(pci_box_y + 56);

            fb.fill_rect(box_x, pci_box_y, box_w, pci_box_h, card_bg);
            fb.draw_rect_border(box_x, pci_box_y, box_w, pci_box_h, 2, border_color);

            let mut pci_title_buf = [0u8; 20];
            let str_pci_count = u64_to_str(pci_devices.len() as u64, &mut pci_title_buf);
            fb.draw_string(
                box_x + 20,
                pci_box_y + 16,
                "[ FULL PCIe / Q35 BUS INVENTORY (0..=255) - ",
                accent_color,
                None,
            );
            let mut tx = box_x + 20 + 44 * 8;
            fb.draw_string(tx, pci_box_y + 16, str_pci_count, text_yellow, None);
            tx += str_pci_count.len() * 8;
            fb.draw_string(tx, pci_box_y + 16, " FUNCTIONS DETECTED ]", accent_color, None);

            let table_header_y = pci_box_y + 46;
            fb.draw_string(box_x + 20, table_header_y, "SLOT", text_dim, None);
            fb.draw_string(box_x + 104, table_header_y, "VENDOR:DEV", text_dim, None);
            fb.draw_string(box_x + 208, table_header_y, "PIF", text_dim, None);
            fb.draw_string(box_x + 256, table_header_y, "PRIMARY BAR", text_dim, None);
            fb.draw_string(box_x + 390, table_header_y, "DEVICE CLASS / FUNCTION", text_dim, None);
            fb.draw_string(box_x + 700, table_header_y, "HARDWARE VENDOR", text_dim, None);

            fb.fill_rect(box_x + 20, table_header_y + 20, box_w - 40, 1, border_color);

            let mut pci_row_y = table_header_y + 28;
            for (i, dev) in pci_devices.iter().enumerate() {
                if pci_row_y + 22 > pci_box_y + pci_box_h - 14 {
                    break;
                }

                if i % 2 == 1 {
                    fb.fill_rect(box_x + 10, pci_row_y - 2, box_w - 20, 20, 0x00131b2e);
                }

                // Slot: BB:DD.F
                let mut buf_b = [0u8; 2];
                let mut buf_d = [0u8; 2];
                let mut buf_f = [0u8; 20];
                let s_b = u8_to_hex2(dev.bus, &mut buf_b);
                let s_d = u8_to_hex2(dev.device, &mut buf_d);
                let s_f = u64_to_str(dev.function as u64, &mut buf_f);

                let mut sx = box_x + 20;
                fb.draw_string(sx, pci_row_y, s_b, text_cyan, None);
                sx += 16;
                fb.draw_string(sx, pci_row_y, ":", text_dim, None);
                sx += 8;
                fb.draw_string(sx, pci_row_y, s_d, text_cyan, None);
                sx += 16;
                fb.draw_string(sx, pci_row_y, ".", text_dim, None);
                sx += 8;
                fb.draw_string(sx, pci_row_y, s_f, text_cyan, None);

                // Vendor & Device IDs
                let mut buf_vid = [0u8; 4];
                let mut buf_did = [0u8; 4];
                let s_vid = u16_to_hex4(dev.vendor_id, &mut buf_vid);
                let s_did = u16_to_hex4(dev.device_id, &mut buf_did);

                let mut idx = box_x + 104;
                fb.draw_string(idx, pci_row_y, s_vid, text_yellow, None);
                idx += 32;
                fb.draw_string(idx, pci_row_y, ":", text_dim, None);
                idx += 8;
                fb.draw_string(idx, pci_row_y, s_did, text_yellow, None);

                // ProgIF
                let mut buf_pif = [0u8; 2];
                let s_pif = u8_to_hex2(dev.prog_if, &mut buf_pif);
                fb.draw_string(box_x + 208, pci_row_y, s_pif, text_dim, None);

                // Primary BAR
                if let Some((bar_idx, bar_val)) = dev.primary_bar() {
                    let mut buf_bi = [0u8; 20];
                    let mut buf_bv = [0u8; 10];
                    let s_bi = u64_to_str(bar_idx as u64, &mut buf_bi);
                    let s_bv = u32_to_hex8(bar_val, &mut buf_bv);
                    let mut bx = box_x + 256;
                    fb.draw_string(bx, pci_row_y, "B", text_dim, None);
                    bx += 8;
                    fb.draw_string(bx, pci_row_y, s_bi, text_dim, None);
                    bx += 8;
                    fb.draw_string(bx, pci_row_y, ":", text_dim, None);
                    bx += 8;
                    fb.draw_string(bx, pci_row_y, s_bv, text_green, None);
                } else {
                    fb.draw_string(box_x + 256, pci_row_y, "None", text_dim, None);
                }

                // Class Name & Vendor Name
                fb.draw_string(box_x + 390, pci_row_y, dev.class_name(), text_white, None);
                fb.draw_string(box_x + 700, pci_row_y, dev.vendor_name(), text_dim, None);

                pci_row_y += 24;
            }
        }

        DashboardTab::MemoryAndHeap => {
            // Top Card: PMM, VMM & Heap Summary
            let box_y = 104;
            let box_h = 210;
            fb.fill_rect(box_x, box_y, box_w, box_h, card_bg);
            fb.draw_rect_border(box_x, box_y, box_w, box_h, 2, border_color);

            fb.draw_string(
                box_x + 20,
                box_y + 16,
                "[ PHYSICAL MEMORY (PMM), PAGING (VMM) & KERNEL HEAP TELEMETRY ]",
                accent_color,
                None,
            );

            let mut ry = box_y + 48;
            fb.draw_string(box_x + 20, ry, "Physical RAM:", text_dim, None);
            let mut buf_tot = [0u8; 20];
            let mut buf_use = [0u8; 20];
            let mut buf_ff = [0u8; 20];
            let mut buf_tf = [0u8; 20];
            let str_tot = u64_to_str(mem_stats.total_ram_mib(), &mut buf_tot);
            let str_use = u64_to_str(mem_stats.usable_mib(), &mut buf_use);
            let str_ff = u64_to_str(pmm_stats.free_frames as u64, &mut buf_ff);
            let str_tf = u64_to_str(pmm_stats.total_frames as u64, &mut buf_tf);

            let mut mx = box_x + 180;
            fb.draw_string(mx, ry, str_use, text_green, None);
            mx += str_use.len() * 8;
            fb.draw_string(mx, ry, " MiB usable / ", text_white, None);
            mx += 14 * 8;
            fb.draw_string(mx, ry, str_tot, text_white, None);
            mx += str_tot.len() * 8;
            fb.draw_string(mx, ry, " MiB installed (", text_dim, None);
            mx += 16 * 8;
            fb.draw_string(mx, ry, str_ff, text_yellow, None);
            mx += str_ff.len() * 8;
            fb.draw_string(mx, ry, " / ", text_dim, None);
            mx += 24;
            fb.draw_string(mx, ry, str_tf, text_white, None);
            mx += str_tf.len() * 8;
            fb.draw_string(mx, ry, " frames free)", text_dim, None);

            ry += 28;
            fb.draw_string(box_x + 20, ry, "PMM Security:", text_dim, None);
            if pmm_test_ok {
                fb.draw_string(
                    box_x + 180,
                    ry,
                    "Low Memory (<1 MiB, frames 0..255) permanently reserved | Bitmap @ 0x00100000",
                    text_green,
                    None,
                );
            }

            ry += 28;
            fb.draw_string(box_x + 20, ry, "VMM Paging:", text_dim, None);
            let mut hex_cr3 = [0u8; 18];
            let str_cr3 = u64_to_hex(cr3_val, &mut hex_cr3);
            let mut vx = box_x + 180;
            fb.draw_string(vx, ry, "4-Level Paging Active | CR3: ", text_white, None);
            vx += 29 * 8;
            fb.draw_string(vx, ry, str_cr3, text_cyan, None);
            if vmm_test_ok {
                fb.draw_string(vx + 20 * 8, ry, "(Self-Test PASS)", text_green, None);
            }

            ry += 28;
            fb.draw_string(box_x + 20, ry, "Kernel Heap:", text_dim, None);
            let (heap_used, heap_total) = heap::get_stats();
            let mut buf_hu = [0u8; 20];
            let mut buf_ht = [0u8; 20];
            let str_hu = u64_to_str(heap_used as u64, &mut buf_hu);
            let str_ht = u64_to_str((heap_total / 1024) as u64, &mut buf_ht);
            let mut hx = box_x + 180;
            fb.draw_string(hx, ry, str_hu, text_yellow, None);
            hx += str_hu.len() * 8;
            fb.draw_string(hx, ry, " bytes used / ", text_white, None);
            hx += 14 * 8;
            fb.draw_string(hx, ry, str_ht, text_green, None);
            hx += str_ht.len() * 8;
            fb.draw_string(hx, ry, " KiB total (IrqSafe Free-List @ 0xFFFFC00000000000)", text_dim, None);
            if !heap_test_ok {
                fb.draw_string(hx, ry, " [FAIL]", 0x00ef4444, None);
            }

            // Bottom Card: UEFI Memory Map Table
            let mm_y = box_y + box_h + 16;
            let mm_h = fb.height.saturating_sub(mm_y + 56);
            fb.fill_rect(box_x, mm_y, box_w, mm_h, card_bg);
            fb.draw_rect_border(box_x, mm_y, box_w, mm_h, 2, border_color);

            fb.draw_string(
                box_x + 20,
                mm_y + 16,
                "[ UEFI PHYSICAL MEMORY MAP REGIONS (FIRST 11 ENTRIES) ]",
                accent_color,
                None,
            );

            let th_y = mm_y + 44;
            fb.draw_string(box_x + 20, th_y, "INDEX", text_dim, None);
            fb.draw_string(box_x + 90, th_y, "PHYSICAL BASE", text_dim, None);
            fb.draw_string(box_x + 260, th_y, "LENGTH (KiB)", text_dim, None);
            fb.draw_string(box_x + 420, th_y, "REGION CLASSIFICATION", text_dim, None);
            fb.fill_rect(box_x + 20, th_y + 18, box_w - 40, 1, border_color);

            if let Some(entries) = mem_entries_opt {
                let mut mry = th_y + 24;
                for (i, entry) in entries.iter().take(11).enumerate() {
                    if mry + 20 > mm_y + mm_h - 10 {
                        break;
                    }
                    let mut buf_i = [0u8; 20];
                    let mut buf_base = [0u8; 18];
                    let mut buf_len = [0u8; 20];
                    let s_i = u64_to_str(i as u64, &mut buf_i);
                    let s_base = u64_to_hex(entry.base, &mut buf_base);
                    let s_len = u64_to_str(entry.length / 1024, &mut buf_len);

                    let (type_str, type_col) = match entry.entry_type {
                        EntryType::USABLE => ("Usable System RAM", text_green),
                        EntryType::RESERVED => ("Reserved (Firmware / MMIO)", text_dim),
                        EntryType::ACPI_RECLAIMABLE => ("ACPI Reclaimable", text_cyan),
                        EntryType::ACPI_NVS => ("ACPI NVS Storage", text_yellow),
                        EntryType::BAD_MEMORY => ("Bad Physical Memory", 0x00ef4444),
                        EntryType::BOOTLOADER_RECLAIMABLE => ("Bootloader Reclaimable", text_cyan),
                        EntryType::EXECUTABLE_AND_MODULES => ("Kernel & Boot Modules", text_yellow),
                        EntryType::FRAMEBUFFER => ("UEFI GOP Framebuffer MMIO", text_white),
                        _ => ("Other Region", text_dim),
                    };

                    fb.draw_string(box_x + 20, mry, s_i, text_dim, None);
                    fb.draw_string(box_x + 90, mry, s_base, text_white, None);
                    fb.draw_string(box_x + 260, mry, s_len, text_cyan, None);
                    fb.draw_string(box_x + 420, mry, type_str, type_col, None);
                    mry += 20;
                }
            }
        }

        DashboardTab::StorageAndVfs => {
            // Top Card: SATA AHCI 1.0 Controller & Disk Inventory (Read-Only + S.M.A.R.T.)
            let ahci_y = 104;
            let ahci_h = 250;
            fb.fill_rect(box_x, ahci_y, box_w, ahci_h, card_bg);
            fb.draw_rect_border(box_x, ahci_y, box_w, ahci_h, 2, border_color);

            fb.draw_string(
                box_x + 20,
                ahci_y + 18,
                "[ SATA AHCI 1.0 STORAGE CONTROLLER & S.M.A.R.T. DISK TELEMETRY (READ-ONLY) ]",
                accent_color,
                None,
            );

            let ay = ahci_y + 46;
            fb.draw_string(box_x + 20, ay, "AHCI HBA Status:", text_dim, None);
            if ahci_info.present {
                let mut b_bus = [0u8; 2];
                let mut b_dev = [0u8; 2];
                let mut b_fn = [0u8; 20];
                let mut b_vid = [0u8; 4];
                let mut b_did = [0u8; 4];
                let mut b_abar = [0u8; 10];
                let s_bus = u8_to_hex2(ahci_info.pci_bus, &mut b_bus);
                let s_dev = u8_to_hex2(ahci_info.pci_device, &mut b_dev);
                let s_fn = u64_to_str(ahci_info.pci_function as u64, &mut b_fn);
                let s_vid = u16_to_hex4(ahci_info.vendor_id, &mut b_vid);
                let s_did = u16_to_hex4(ahci_info.device_id, &mut b_did);
                let s_abar = u32_to_hex8(ahci_info.abar_phys as u32, &mut b_abar);

                let mut ax = box_x + 180;
                fb.draw_string(ax, ay, "Active PCI ", text_green, None);
                ax += 11 * 8;
                fb.draw_string(ax, ay, s_bus, text_cyan, None);
                ax += 2 * 8;
                fb.draw_string(ax, ay, ":", text_dim, None);
                ax += 8;
                fb.draw_string(ax, ay, s_dev, text_cyan, None);
                ax += 2 * 8;
                fb.draw_string(ax, ay, ".", text_dim, None);
                ax += 8;
                fb.draw_string(ax, ay, s_fn, text_cyan, None);
                ax += s_fn.len() * 8;
                fb.draw_string(ax, ay, " (", text_dim, None);
                ax += 2 * 8;
                fb.draw_string(ax, ay, s_vid, text_yellow, None);
                ax += 4 * 8;
                fb.draw_string(ax, ay, ":", text_dim, None);
                ax += 8;
                fb.draw_string(ax, ay, s_did, text_yellow, None);
                ax += 4 * 8;
                fb.draw_string(ax, ay, ") | ABAR/BAR5: ", text_dim, None);
                ax += 15 * 8;
                fb.draw_string(ax, ay, s_abar, text_white, None);
                ax += 10 * 8;
                fb.draw_string(ax, ay, " (MMIO PWT|PCD Uncached | BusMaster)", text_green, None);
            } else {
                fb.draw_string(box_x + 180, ay, "No AHCI 1.0 controller detected", text_yellow, None);
            }

            let dth_y = ahci_y + 80;
            fb.draw_string(box_x + 20, dth_y, "PORT", text_dim, None);
            fb.draw_string(box_x + 80, dth_y, "MODEL NUMBER", text_dim, None);
            fb.draw_string(box_x + 310, dth_y, "SERIAL NUMBER", text_dim, None);
            fb.draw_string(box_x + 500, dth_y, "FW REV", text_dim, None);
            fb.draw_string(box_x + 590, dth_y, "LBA48 SECTORS / CAPACITY", text_dim, None);
            fb.draw_string(box_x + 860, dth_y, "S.M.A.R.T. STATUS", text_dim, None);
            fb.fill_rect(box_x + 20, dth_y + 18, box_w - 40, 1, border_color);

            let mut dry = dth_y + 26;
            for drv in ahci_info.drives.iter() {
                let mut b_p = [0u8; 20];
                let mut b_sec = [0u8; 20];
                let mut b_mib = [0u8; 20];
                let s_p = u64_to_str(drv.port_number as u64, &mut b_p);
                let s_sec = u64_to_str(drv.total_sectors_lba48, &mut b_sec);
                let s_mib = u64_to_str(drv.capacity_mib, &mut b_mib);

                fb.draw_string(box_x + 20, dry, "P#", text_dim, None);
                fb.draw_string(box_x + 36, dry, s_p, text_cyan, None);
                fb.draw_string(box_x + 80, dry, drv.model.as_str(), text_white, None);
                fb.draw_string(box_x + 310, dry, drv.serial.as_str(), text_yellow, None);
                fb.draw_string(box_x + 500, dry, drv.firmware.as_str(), text_cyan, None);

                let mut cx = box_x + 590;
                fb.draw_string(cx, dry, s_sec, text_white, None);
                cx += s_sec.len() * 8;
                fb.draw_string(cx, dry, " (", text_dim, None);
                cx += 2 * 8;
                fb.draw_string(cx, dry, s_mib, text_green, None);
                cx += s_mib.len() * 8;
                fb.draw_string(cx, dry, " MiB)", text_green, None);

                let smart_str = match (drv.smart_supported, drv.smart_enabled) {
                    (true, true) => "Supported & Enabled (OK)",
                    (true, false) => "Supported (Disabled)",
                    _ => "Not Supported",
                };
                fb.draw_string(box_x + 860, dry, smart_str, text_green, None);
                dry += 24;
            }

            if ahci_info.atapi_ports_count > 0 {
                let mut b_atapi = [0u8; 20];
                let s_atapi = u64_to_str(ahci_info.atapi_ports_count as u64, &mut b_atapi);
                fb.draw_string(box_x + 20, dry + 6, "SATAPI Optical Drives Active: ", text_dim, None);
                fb.draw_string(box_x + 260, dry + 6, s_atapi, text_cyan, None);
                fb.draw_string(box_x + 280, dry + 6, "(UEFI Boot ISO Media | Write-Protected)", text_dim, None);
            }

            // Bottom Card: POSIX Virtual File System (/) & USTAR Initramfs Inspection
            let vfs_y = 370;
            let vfs_h = 372;
            fb.fill_rect(box_x, vfs_y, box_w, vfs_h, card_bg);
            fb.draw_rect_border(box_x, vfs_y, box_w, vfs_h, 2, border_color);

            fb.draw_string(
                box_x + 20,
                vfs_y + 18,
                "[ POSIX VIRTUAL FILE SYSTEM (/) - USTAR INITRAMFS ARCHIVE & FILE CONTENTS ]",
                accent_color,
                None,
            );

            // Left Column: Mounted VFS Directory & File Table
            let vth_y = vfs_y + 48;
            fb.draw_string(box_x + 20, vth_y, "TYPE", text_dim, None);
            fb.draw_string(box_x + 84, vth_y, "SIZE (B)", text_dim, None);
            fb.draw_string(box_x + 172, vth_y, "CANONICAL VFS PATH (MOUNTED AT /)", text_dim, None);
            fb.fill_rect(box_x + 20, vth_y + 18, 520, 1, border_color);

            let mut vry = vth_y + 26;
            for entry in vfs_entries.iter().take(11) {
                let mut b_sz = [0u8; 20];
                let s_sz = u64_to_str(entry.size as u64, &mut b_sz);
                let type_col = if entry.file_type == fs::vfs::FileType::Directory {
                    text_cyan
                } else {
                    text_green
                };
                fb.draw_string(box_x + 20, vry, entry.file_type.as_str(), type_col, None);
                fb.draw_string(box_x + 84, vry, s_sz, text_yellow, None);
                fb.draw_string(box_x + 172, vry, entry.path.as_str(), text_white, None);
                vry += 22;
            }

            // Right Column: Live File Content Read via POSIX `vfs::open` + `vfs::read`
            let preview_x = box_x + 565;
            let preview_w = box_w.saturating_sub(585);
            let preview_y = vth_y;
            let preview_h = vfs_h.saturating_sub(68);
            fb.fill_rect(preview_x, preview_y, preview_w, preview_h, 0x000f172a);
            fb.draw_rect_border(preview_x, preview_y, preview_w, preview_h, 1, border_color);

            fb.draw_string(
                preview_x + 16,
                preview_y + 12,
                "POSIX vfs::read(\"/etc/optios-release\"):",
                text_cyan,
                None,
            );
            fb.fill_rect(preview_x + 16, preview_y + 32, preview_w - 32, 1, border_color);

            let mut l_y = preview_y + 42;
            for line in optios_release_content.lines().take(10) {
                fb.draw_string(preview_x + 16, l_y, line, text_green, None);
                l_y += 22;
            }
        }
    }

    // Bottom Navigation & Status Bar
    let status_y = fb.height.saturating_sub(38);
    fb.fill_rect(0, status_y, fb.width, 38, banner_bg);
    fb.fill_rect(0, status_y, fb.width, 2, border_color);
    fb.draw_string(
        24,
        status_y + 12,
        "OptiOS v0.5.0-alpha  |  Keys: [1/F1] System  [2/F2] PCI  [3/F3] Memory  [4/F4] Storage & VFS  [Tab/Arrows] Cycle",
        text_white,
        Some(banner_bg),
    );

    // Update top-right clock
    render_header_clock(fb, ticks);
}

/// Main entry point invoked by the Limine bootloader in 64-bit Long Mode.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Initialize synchronous serial console (COM1) immediately
    unsafe {
        SERIAL1.init();
    }

    info!("========================================================");
    info!("OptiOS Kernel — Milestone 5: POSIX VFS, Initramfs & SATA AHCI");
    info!("========================================================");

    // 2. Validate bootloader base protocol compliance
    if !is_base_revision_supported() {
        error!("Limine bootloader does not satisfy the required base revision!");
        halt_loop();
    }
    info!("Limine Protocol: Base revision handshake successful.");

    // 3. Register emergency framebuffer if available
    let raw_fb_opt = get_framebuffer();
    if let Some(ref raw_fb) = raw_fb_opt {
        init_global_framebuffer(raw_fb);
    }

    // 4. Initialize kernel GDT and TSS with dedicated Double Fault IST 1 stack
    gdt::init();
    info!("GDT & TSS: Dedicated kernel segments and IST 1 loaded.");

    // 5. Initialize IDT (CPU Exception Gates 0..31 & Hardware IRQ Gates 32..33)
    idt::init();

    // 6. Query CPU identification via CPUID
    let cpu_info = get_cpu_info();
    let cpu_name = cpu_info.as_str();
    info!("CPU (CPUID): {}", cpu_name);

    // 7. Parse and accurately calculate real physical RAM
    let mem_entries_opt = get_memory_map();
    let mem_stats = if let Some(entries) = mem_entries_opt {
        let stats = parse_memory_map(entries);
        info!(
            "Physical Memory: {} MiB installed ({} MiB usable for OS).",
            stats.total_ram_mib(),
            stats.usable_mib()
        );
        info!("Memory Map: {} regions reported by UEFI firmware.", stats.entry_count);
        stats
    } else {
        warn!("Physical memory map not provided by the bootloader!");
        Default::default()
    };

    // 8. Query HHDM virtual offset
    let hhdm_offset = get_hhdm_offset().unwrap_or(0);
    info!("Higher-Half Direct Map (HHDM): 0x{:016x}", hhdm_offset);

    // 9. Initialize Physical Memory Manager with lower 1 MiB protection
    if let Some(entries) = mem_entries_opt {
        pmm::init(entries, hhdm_offset);
    }
    let pmm_test_ok = pmm::test_frame_allocation();
    let pmm_stats = pmm::get_stats();

    // A6 Audit: Demonstrate whether initramfs module physical frames are free or reserved
    #[cfg(feature = "selftest")]
    {
        let initramfs_mod = get_initramfs_module();
        if let Some((mod_path, mod_slice)) = initramfs_mod {
            let mod_virt = mod_slice.as_ptr() as u64;
            let mod_size = mod_slice.len() as u64;
            let mod_phys_start = if mod_virt >= hhdm_offset {
                mod_virt - hhdm_offset
            } else {
                mod_virt
            };
            let mod_phys_end = mod_phys_start + mod_size;

            info!(
                "A6 Audit: Initramfs module '{}' ({} bytes) at phys 0x{:016x}..0x{:016x}",
                mod_path, mod_size, mod_phys_start, mod_phys_end
            );

            // Check containing memory map entry
            if let Some(entries) = mem_entries_opt {
                for entry in entries {
                    let entry_end = entry.base + entry.length;
                    if mod_phys_start >= entry.base && mod_phys_end <= entry_end {
                        let type_str = match entry.entry_type {
                            EntryType::USABLE => "USABLE",
                            EntryType::RESERVED => "RESERVED",
                            EntryType::ACPI_RECLAIMABLE => "ACPI_RECLAIMABLE",
                            EntryType::ACPI_NVS => "ACPI_NVS",
                            EntryType::BAD_MEMORY => "BAD_MEMORY",
                            EntryType::BOOTLOADER_RECLAIMABLE => "BOOTLOADER_RECLAIMABLE",
                            EntryType::EXECUTABLE_AND_MODULES => "EXECUTABLE_AND_MODULES",
                            EntryType::FRAMEBUFFER => "FRAMEBUFFER",
                            _ => "OTHER",
                        };
                        info!(
                            "A6 Audit: Module is within MemMap entry [0x{:016x}..0x{:016x}] type {}",
                            entry.base, entry_end, type_str
                        );
                    }
                }
            }

            // Check each frame in range against PMM bitmap and check bitmap storage overlap
            let start_frame = mod_phys_start / pmm::PAGE_SIZE;
            let end_frame = (mod_phys_end + pmm::PAGE_SIZE - 1) / pmm::PAGE_SIZE;
            let bitmap_phys_start = pmm_stats.bitmap_phys_addr;
            let bitmap_phys_end = bitmap_phys_start + pmm_stats.bitmap_size_bytes as u64;

            let overlaps_bitmap = !(mod_phys_end <= bitmap_phys_start || mod_phys_start >= bitmap_phys_end);
            info!("A6 Audit: Overlaps PMM bitmap storage: {}", overlaps_bitmap);

            for f in start_frame..end_frame {
                let f_addr = f * pmm::PAGE_SIZE;
                let is_res = pmm::is_frame_reserved(f_addr);
                info!(
                    "A6 Audit: Frame 0x{:016x} (index {}) state: {}",
                    f_addr,
                    f,
                    if is_res { "RESERVED" } else { "FREE (ALLOCATABLE)" }
                );
            }
        }
    }

    // 10. Initialize Virtual Memory Manager (VMM) and run self-test
    vmm::init(hhdm_offset);
    let vmm_test_ok = vmm::test_vmm_mapping();
    let cr3_val = vmm::read_cr3();

    // 11. Initialize IrqSafe Kernel Heap Allocator and run dynamic Rust alloc self-test
    heap::init();
    let heap_test_ok = heap::test_heap_allocation();

    // 12. Parse SMBIOS / DMI Firmware tables
    let (smbios_32, smbios_64) = get_smbios_entry_points();
    let smbios_info = smbios::init(smbios_32, smbios_64, hhdm_offset);

    // 13. Enumerate PCI/PCIe hardware across all 256 buses (with Multi-Function & BARs)
    let pci_devices: Vec<PciDevice> = scan_pci_bus();

    // 14. Mount POSIX Virtual File System (/) from USTAR Initramfs and verify files
    let initramfs_mod = get_initramfs_module();
    let _vfs_count = vfs::init(initramfs_mod);
    let vfs_entries = vfs::list_dir("/").unwrap_or_default();
    for entry in vfs_entries.iter() {
        info!(
            "VFS Entry: [{}] {:>6} bytes | mode {:04o} | {}",
            entry.file_type.as_str(),
            entry.size,
            entry.mode,
            entry.path
        );
    }
    let optios_release_content = match vfs::read_to_string("/etc/optios-release") {
        Ok(content) => {
            if let Some(first_line) = content.lines().next() {
                info!("VFS Read Test (/etc/optios-release): {}", first_line);
            }
            content
        }
        Err(e) => {
            warn!("VFS Read Test failed for /etc/optios-release: {}", e);
            String::from("Error reading /etc/optios-release")
        }
    };

    // VFS Boot-Time Functional Selftest (Step 4)
    #[cfg(feature = "selftest")]
    {
        let pass1 = vfs::open("/etc/motd").is_ok();
        info!("VFS Selftest: open(\"/etc/motd\") OK -> {}", if pass1 { "PASS" } else { "FAIL" });

        let pass2 = vfs::open("/etc/../motd") == Err(crate::posix::errno::Errno::EINVAL);
        info!("VFS Selftest: open(\"/etc/../motd\") EINVAL -> {}", if pass2 { "PASS" } else { "FAIL" });

        let pass3 = matches!(vfs::stat("/"), Ok(ref st) if st.path == "/" && st.file_type == vfs::FileType::Directory);
        info!("VFS Selftest: stat(\"/\") OK (root) -> {}", if pass3 { "PASS" } else { "FAIL" });

        let pass4 = vfs::stat("/etc/optios-release").is_ok();
        info!("VFS Selftest: stat(\"/etc/optios-release\") OK -> {}", if pass4 { "PASS" } else { "FAIL" });

        let pass5 = matches!(vfs::stat("/a/../b"), Err(crate::posix::errno::Errno::EINVAL));
        info!("VFS Selftest: stat(\"/a/../b\") EINVAL -> {}", if pass5 { "PASS" } else { "FAIL" });

        let pass6 = matches!(vfs::list_dir("/"), Ok(ref l) if !l.is_empty());
        info!("VFS Selftest: list_dir(\"/\") OK -> {}", if pass6 { "PASS" } else { "FAIL" });

        let pass7 = matches!(vfs::list_dir("/etc"), Ok(ref l) if !l.is_empty());
        info!("VFS Selftest: list_dir(\"/etc\") OK -> {}", if pass7 { "PASS" } else { "FAIL" });

        let pass8 = vfs::open("") == Err(crate::posix::errno::Errno::EINVAL);
        info!("VFS Selftest: open(\"\") EINVAL -> {}", if pass8 { "PASS" } else { "FAIL" });

        let long_path = "/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"; // 257 chars
        let pass9 = vfs::open(long_path) == Err(crate::posix::errno::Errno::ENAMETOOLONG);
        info!("VFS Selftest: open(>255 chars) ENAMETOOLONG -> {}", if pass9 { "PASS" } else { "FAIL" });
    }

    // 15. Initialize Dual 8259 PIC, PIT Timer (100 Hz), PS/2 Keyboard & enable hardware interrupts (STI)
    pic::init();
    pit::init();
    keyboard::init();
    info!("Enabling hardware interrupts (STI) for timer-based AHCI polling.");
    sti();

    // 16. Initialize Read-Only SATA AHCI 1.0 Controller & inspect drives via ATA IDENTIFY (0xEC)
    let ahci_info = ahci::init(&pci_devices);

    // AHCI Selftests (Step B6)
    #[cfg(feature = "selftest")]
    {
        // 1. Timeout verification: wait_until(50 ms, || false)
        // Lower bound is tick-quantised to 10 ms PIT intervals (with +1 tick ceiling);
        // TSC delta is the independent, cycle-accurate measurement datum.
        let tsc_start = time::read_tsc();
        let t_start = time::ticks();
        let timeout_res = time::wait_until(50, || false);
        let t_end = time::ticks();
        let tsc_end = time::read_tsc();
        let elapsed_ticks = t_end.saturating_sub(t_start);
        let elapsed_ms = elapsed_ticks * 10;
        let tsc_delta = tsc_end.saturating_sub(tsc_start);

        let pass_to = timeout_res == Err(optios_ahci_proto::AhciError::Timeout)
            && elapsed_ms >= 50
            && elapsed_ms <= 100;
        info!(
            "AHCI Selftest: wait_until(50ms) timeout check (elapsed {} ms, {} ticks [tick-quantised], TSC delta: {} cycles [independent]) -> {}",
            elapsed_ms,
            elapsed_ticks,
            tsc_delta,
            if pass_to { "PASS" } else { "FAIL" }
        );

        // 2. Port 2 (SATAPI) command abort and recovery check
        if let Some(port2) = ahci::get_port(2) {
            // First attempt: should abort with TaskFileError
            let p2_res1 = unsafe { port2.execute_command(optios_ahci_proto::AllowedCommand::IdentifyDevice) };
            let p2_aborted1 = p2_res1 == Err(optios_ahci_proto::AhciError::TaskFileError);

            // Second attempt after recovery: should also abort with TaskFileError (not Timeout / PortFailed)
            let p2_res2 = unsafe { port2.execute_command(optios_ahci_proto::AllowedCommand::IdentifyDevice) };
            let p2_aborted2 = p2_res2 == Err(optios_ahci_proto::AhciError::TaskFileError);

            // Port 0 should still function and return the exact same disk telemetry
            let p0_ok = if let Some(port0) = ahci::get_port(0) {
                unsafe {
                    match port0.execute_command(optios_ahci_proto::AllowedCommand::IdentifyDevice) {
                        Ok(()) => {
                            let words = port0.read_identify_buffer();
                            let d = optios_ahci_proto::decode_identify_data(&words);
                            d.model == "OptiOS_Test_SSD" && d.serial == "OPTI-2026-9901" && d.total_sectors == 131072
                        }
                        Err(_) => false,
                    }
                }
            } else {
                false
            };

            let pass_satapi = p2_aborted1 && p2_aborted2 && p0_ok;
            info!(
                "AHCI Selftest: Port 2 ATAPI double abort & Port 0 recovery verification -> {}",
                if pass_satapi { "PASS" } else { "FAIL" }
            );
        }
    }

    // 17. Render initial Milestone 5 interactive dashboard on the Framebuffer
    let mut active_tab = DashboardTab::SystemAndSmbios;
    let mut fb_driver_opt = raw_fb_opt.as_ref().map(FramebufferDriver::from_limine);

    if let Some(ref mut fb) = fb_driver_opt {
        info!(
            "Framebuffer active: {}x{} ({} bpp, pitch: {} bytes)",
            fb.width, fb.height, fb.bpp, fb.pitch
        );

        let bg_color = 0x000f172a;
        let banner_bg = 0x001e293b;
        let accent_color = 0x003b82f6;
        let text_white = 0x00f8fafc;

        fb.clear(bg_color);
        fb.fill_rect(0, 0, fb.width, 48, banner_bg);
        fb.fill_rect(0, 47, fb.width, 2, accent_color);
        fb.draw_string(
            24,
            16,
            "OptiOS  |  Live Diagnostic & Recovery Environment",
            text_white,
            Some(banner_bg),
        );

        render_dashboard(
            fb,
            active_tab,
            cpu_name,
            cr3_val,
            &mem_stats,
            &pmm_stats,
            pmm_test_ok,
            vmm_test_ok,
            heap_test_ok,
            &smbios_info,
            &pci_devices,
            mem_entries_opt,
            &ahci_info,
            &vfs_entries,
            &optios_release_content,
            pit::ticks(),
        );

        info!("Milestone 5 interactive dashboard rendered successfully on Framebuffer.");
    } else {
        warn!("No graphical framebuffer available!");
    }

    // 18. Enter the real-time Event Loop
    info!("Entering interactive event loop.");

    let mut last_tenth: u64 = 0;
    let mut logged_timer_milestone = false;

    loop {
        // Sleep until the next hardware interrupt (PIT tick or PS/2 keyboard press)
        hlt();

        let current_ticks = pit::ticks();
        if !logged_timer_milestone && current_ticks >= 10 {
            info!(
                "Timer Verification: PIT IRQ 0 active ({} ticks / {} ms elapsed).",
                current_ticks,
                pit::uptime_ms()
            );
            logged_timer_milestone = true;
        }

        // Process all pending keyboard events from the lock-free ring buffer
        let mut tab_changed = false;
        while let Some(key) = keyboard::poll_key() {
            let new_tab = match key {
                KeyEvent::Char('1') | KeyEvent::F1 => Some(DashboardTab::SystemAndSmbios),
                KeyEvent::Char('2') | KeyEvent::F2 => Some(DashboardTab::PciInventory),
                KeyEvent::Char('3') | KeyEvent::F3 => Some(DashboardTab::MemoryAndHeap),
                KeyEvent::Char('4') | KeyEvent::F4 => Some(DashboardTab::StorageAndVfs),
                KeyEvent::Tab | KeyEvent::ArrowRight => Some(active_tab.next()),
                KeyEvent::ArrowLeft => Some(active_tab.prev()),
                _ => None,
            };

            if let Some(target_tab) = new_tab {
                if target_tab != active_tab {
                    active_tab = target_tab;
                    tab_changed = true;
                    info!(
                        "Keyboard Event: Switched dashboard view to Tab {} [{}].",
                        active_tab as u8,
                        active_tab.title()
                    );
                }
            }
        }

        // Update UI if the active tab changed or 0.1s (10 ticks) elapsed
        let current_tenth = current_ticks / 10;
        if tab_changed {
            if let Some(ref mut fb) = fb_driver_opt {
                render_dashboard(
                    fb,
                    active_tab,
                    cpu_name,
                    cr3_val,
                    &mem_stats,
                    &pmm_stats,
                    pmm_test_ok,
                    vmm_test_ok,
                    heap_test_ok,
                    &smbios_info,
                    &pci_devices,
                    mem_entries_opt,
                    &ahci_info,
                    &vfs_entries,
                    &optios_release_content,
                    current_ticks,
                );
            }
            last_tenth = current_tenth;
        } else if current_tenth != last_tenth {
            if let Some(ref mut fb) = fb_driver_opt {
                render_header_clock(fb, current_ticks);
            }
            last_tenth = current_tenth;
        }
    }
}

/// Kernel panic handler with serial telemetry and on-screen panic dump.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    error!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");
    error!("KERNEL PANIC: Unrecoverable execution fault!");
    if let Some(location) = info.location() {
        error!("Location: {}:{}", location.file(), location.line());
    }
    if let Some(msg) = info.message().as_str() {
        error!("Message: {}", msg);
    }
    error!("!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!!");

    with_framebuffer(|fb| {
        let panic_y = fb.height.saturating_sub(90);
        fb.fill_rect(20, panic_y, fb.width.saturating_sub(40), 70, 0x00dc2626);
        fb.draw_string(
            36,
            panic_y + 16,
            "KERNEL PANIC! Check serial console telemetry (COM1 - 0x3F8) for details.",
            0x00ffffff,
            Some(0x00dc2626),
        );
        if let Some(location) = info.location() {
            let mut buf_line = [0u8; 20];
            let str_line = u64_to_str(location.line() as u64, &mut buf_line);
            fb.draw_string(36, panic_y + 40, "Source: ", 0x00fef08a, Some(0x00dc2626));
            fb.draw_string(36 + 64, panic_y + 40, location.file(), 0x00ffffff, Some(0x00dc2626));
            fb.draw_string(36 + 64 + location.file().len() * 8, panic_y + 40, ":", 0x00ffffff, Some(0x00dc2626));
            fb.draw_string(36 + 64 + location.file().len() * 8 + 8, panic_y + 40, str_line, 0x00ffffff, Some(0x00dc2626));
        }
    });

    halt_loop();
}
