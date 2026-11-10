// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Kernel Main Entry Point (_start) & Text Console Boot
// Date: 2026-11-10

//! OptiOS Kernel Main Entry Point (_start) — Milestone 10:
//! Read-Only FAT32 Storage VFS Mounts, Raw Mode Keyboard & Ring 3 Text Inspector.

#![no_std]
#![no_main]

extern crate alloc;

mod arch;
mod boot;
mod drivers;
mod firmware;
mod fs;
pub mod gui;
mod logger;
mod mm;
mod posix;
pub mod shell;
pub mod syscall;
pub mod time;

use core::panic::PanicInfo;
use arch::{halt_loop, hlt, sti};
use arch::x86_64::cpuid::get_cpu_info;
use arch::x86_64::{gdt, idt, pic, pit};
use boot::limine::{
    get_framebuffer, get_hhdm_offset, get_initramfs_module, get_memory_map,
    get_smbios_entry_points, is_base_revision_supported,
};
use drivers::ahci;
use drivers::keyboard;
use drivers::mouse;
use drivers::pci::scan_pci_bus;
use drivers::serial::SERIAL1;
use drivers::video::console;
use drivers::video::framebuffer::{init_global_framebuffer, with_framebuffer};
use firmware::smbios;
use fs::vfs;
use mm::memmap::parse_memory_map;
use mm::{heap, pmm, vmm};

/// Kernel entry point invoked by the Limine bootloader in 64-bit Long Mode.
#[no_mangle]
pub extern "C" fn _start() -> ! {
    // 1. Initialize COM1 UART serial port (0x3F8) for early hardware logging
    unsafe {
        SERIAL1.init();
    }

    info!("========================================================");
    info!("OptiOS Kernel — Milestone 10: FAT32 Storage VFS Mounts & Ring 3 Text Inspector");
    info!("========================================================");

    // 2. Validate Limine Base Revision Protocol Handshake
    if !is_base_revision_supported() {
        error!("FATAL: Limine bootloader base revision is not supported.");
        halt_loop();
    }
    info!("Limine Protocol: Base revision handshake successful.");

    // 3. Initialize dedicated Kernel GDT & TSS (IST 1 loaded)
    gdt::init();
    info!("GDT & TSS: Dedicated kernel segments and IST 1 loaded.");

    // 4. Initialize Interrupt Descriptor Table (IDT) with exception handlers
    idt::init();

    // 5. Query and display CPU model via CPUID
    let cpu_name = get_cpu_info();
    info!("CPU (CPUID): {}", cpu_name.as_str());

    // 6. Query and parse UEFI memory map passed by Limine
    let memmap_entries = match get_memory_map() {
        Some(entries) => entries,
        None => {
            error!("FATAL: UEFI memory map not provided by Limine.");
            halt_loop();
        }
    };
    let mem_stats = parse_memory_map(memmap_entries);
    info!(
        "Physical Memory: {} MiB installed ({} MiB usable for OS).",
        mem_stats.total_ram_bytes / (1024 * 1024),
        mem_stats.usable_bytes / (1024 * 1024)
    );
    info!(
        "Memory Map: {} regions reported by UEFI firmware.",
        memmap_entries.len()
    );

    // 7. Initialize Higher-Half Direct Map (HHDM) virtual offset
    let hhdm_offset = match get_hhdm_offset() {
        Some(offset) => offset,
        None => {
            error!("FATAL: HHDM virtual offset not provided by Limine.");
            halt_loop();
        }
    };
    info!("Higher-Half Direct Map (HHDM): 0x{:016x}", hhdm_offset);

    // 8. Initialize Physical Memory Manager (PMM)
    pmm::init(memmap_entries, hhdm_offset);
    let pmm_stats = pmm::get_stats();
    info!(
        "PMM initialized: {} total frames ({} free frames, {} MiB usable).",
        pmm_stats.total_frames,
        pmm_stats.free_frames,
        (pmm_stats.free_frames * 4096) / (1024 * 1024)
    );
    info!(
        "PMM Bitmap: Located at physical 0x{:016x} (size: {} KiB).",
        pmm_stats.bitmap_phys_addr,
        pmm_stats.bitmap_size_bytes / 1024
    );
    info!("PMM Security: Lower 1 MiB (first 256 frames) permanently reserved.");

    // 9. Initialize Virtual Memory Manager (VMM)
    vmm::init(hhdm_offset);

    // 10. Initialize Kernel Heap Allocator (2048 KiB)
    heap::init();
    let (_heap_used, heap_total) = heap::get_stats();
    info!(
        "Kernel Heap initialized (IrqSafe): {} KiB mapped.",
        heap_total / 1024
    );

    // 11. Initialize Framebuffer & Fullscreen Text Console with RAM Double Buffering
    let raw_fb_opt = get_framebuffer();
    if let Some(ref raw_fb) = raw_fb_opt {
        init_global_framebuffer(raw_fb);
        let fb_w = raw_fb.width() as usize;
        let fb_h = raw_fb.height() as usize;
        with_framebuffer(|fb| {
            if let Err(e) = fb.enable_double_buffering() {
                warn!("Framebuffer: Could not enable RAM double buffering: {}", e);
            }
        });
        console::init(fb_w, fb_h);
        info!(
            "Framebuffer active: {}x{} (32 bpp, text console 160x50, RAM Double Buffering enabled).",
            raw_fb.width(),
            raw_fb.height()
        );
    }

    // 12. Parse SMBIOS Firmware Tables
    let (entry_32, entry_64) = get_smbios_entry_points();
    let smbios_info = smbios::init(entry_32, entry_64, hhdm_offset);
    info!(
        "SMBIOS {}.{} parsed ({} structures): BIOS={} ({}, {}), System={} ({})",
        smbios_info.major_version,
        smbios_info.minor_version,
        smbios_info.structure_count,
        smbios_info.bios_vendor,
        smbios_info.bios_version,
        smbios_info.bios_release_date,
        smbios_info.sys_manufacturer,
        smbios_info.sys_product_name
    );

    // 13. Enumerate PCI Bus
    let pci_devices = scan_pci_bus();
    info!(
        "PCI Scanner: Enumerated {} devices across buses 0..=255.",
        pci_devices.len()
    );
    for dev in &pci_devices {
        info!(
            "PCI [{:02x}:{:02x}.{}] ID: {:04x}:{:04x} (ProgIF {:02x}, BAR0 {:08x}) | {} ({})",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            dev.prog_if,
            dev.bars[0],
            dev.class_name(),
            dev.vendor_name()
        );
    }

    // 14. Mount USTAR Initramfs
    let initramfs_mod = get_initramfs_module();
    let _count = vfs::init(initramfs_mod);

    // Print VFS inventory
    if let Ok(entries) = vfs::list_dir("/") {
        for entry in &entries {
            info!(
                "VFS Entry: [{:4}] {:>8} bytes | mode {:04o} | {}",
                entry.file_type.as_str(),
                entry.size,
                entry.mode,
                entry.path
            );
        }
    }

    // Read test /etc/optios-release
    if let Ok(fd) = vfs::open("/etc/optios-release") {
        let mut buf = [0u8; 512];
        if let Ok(n) = vfs::read(fd, 0, &mut buf) {
            if let Ok(content) = core::str::from_utf8(&buf[..n]) {
                if let Some(first_line) = content.lines().next() {
                    info!("VFS Read Test (/etc/optios-release): {}", first_line);
                }
            }
        }
    }

    // 15. Initialize Dual 8259 PIC, PIT Timer (100 Hz), PS/2 Keyboard & enable hardware interrupts (STI)
    pic::init();
    pit::init();
    keyboard::init();
    mouse::init();
    info!("Enabling hardware interrupts (STI) for timer-based AHCI polling & PS/2 mouse.");
    sti();

    // 16. Initialize Read-Only SATA AHCI 1.0 Controller & inspect drives
    let ahci_info = ahci::init(&pci_devices);

    // 16.1 Mount FAT32 Partition 1 of AHCI Port 0 at /mnt/sda1
    if let Some(port0) = ahci::get_port(0) {
        if let Some(p1) = ahci_info
            .drives
            .iter()
            .find(|d| d.port_number == 0)
            .and_then(|d| d.partitions.first())
        {
            let _ = vfs::mount_fat32_sda1(port0, p1.start_lba, p1.sector_count);
        } else {
            let _ = vfs::mount_fat32_sda1(port0, 2048, 32768);
        }
    }

    // 17. Initialize Syscall Subsystem (AMD64 MSRs, TSS kernel stack, dispatcher)
    syscall::init();

    // Boot Selftests
    #[cfg(feature = "selftest")]
    {
        // AHCI Selftests
        if let Some(port0) = ahci::get_port(0) {
            let mut sec0 = [0u8; 512];
            let read0_ok = unsafe { port0.read_sectors(0, 1, &mut sec0).is_ok() };
            let mbr_ok = read0_ok && sec0[510] == 0x55 && sec0[511] == 0xAA;

            let mut sec1 = [0u8; 512];
            let read1_ok = unsafe { port0.read_sectors(1, 1, &mut sec1).is_ok() };
            let gpt_ok = read1_ok && &sec1[..8] == b"EFI PART";

            let smart_res = unsafe { port0.read_smart_data() };
            let smart_ok = smart_res.is_ok();

            info!(
                "AHCI Selftest: Port 0 READ DMA EXT (LBA 0 MBR sig 0x55AA) -> {}",
                if mbr_ok { "PASS" } else { "FAIL" }
            );
            info!(
                "AHCI Selftest: Port 0 READ DMA EXT (LBA 1 GPT sig 'EFI PART') -> {}",
                if gpt_ok { "PASS" } else { "FAIL" }
            );
            info!(
                "AHCI Selftest: Port 0 SMART READ DATA (0xB0 / 0xD0) -> {}",
                if smart_ok { "PASS" } else { "FAIL" }
            );
        }

        // Syscall / Userland Selftest with Zero-Leak Verification
        let free_before = pmm::get_stats().free_frames;
        info!("Syscall Selftest: Spawning Ring 3 userland binary '/bin/sysinfo' (initial free frames: {})...", free_before);
        match syscall::loader::spawn_user_process("/bin/sysinfo") {
            Ok(code) => {
                info!("Syscall Selftest: '/bin/sysinfo' executed in Ring 3, exit status {} -> PASS", code);
            }
            Err(e) => {
                error!("Syscall Selftest: '/bin/sysinfo' failed: {} -> FAIL", e);
            }
        }
        let free_after = pmm::get_stats().free_frames;
        info!(
            "Memory Leak Selftest: PMM free frames: before={}, after={} (delta={}) -> {}",
            free_before,
            free_after,
            free_after as i64 - free_before as i64,
            if free_before == free_after { "PASS (Zero Leaks)" } else { "FAIL (Leak detected)" }
        );
    }

    info!("Entering interactive console event loop.");
    crate::logger::set_console_logging(false);

    // 18. Initialize Shell Session
    let mut shell = shell::Shell::new();
    shell.init_session();

    // 19. Fullscreen Text Console Interactive Event Loop
    loop {
        while let Some(key) = keyboard::poll_key() {
            shell.handle_key(key);
        }
        hlt();
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

    console::print_str("\n\n\x1b[31mKERNEL PANIC: Unrecoverable execution fault!\n");
    if let Some(location) = info.location() {
        let mut buf_line = [0u8; 20];
        let mut val = location.line() as u64;
        let mut i = 20;
        if val == 0 {
            buf_line[0] = b'0';
            i = 19;
        } else {
            while val > 0 && i > 0 {
                i -= 1;
                buf_line[i] = b'0' + (val % 10) as u8;
                val /= 10;
            }
        }
        let line_str = core::str::from_utf8(&buf_line[i..]).unwrap_or("0");
        console::print_str("Location: ");
        console::print_str(location.file());
        console::print_str(":");
        console::print_str(line_str);
        console::print_str("\n");
    }
    if let Some(msg) = info.message().as_str() {
        console::print_str("Message: ");
        console::print_str(msg);
        console::print_str("\n");
    }

    halt_loop();
}
