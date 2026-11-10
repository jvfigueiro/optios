// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Limine Bootloader Protocol Requests & Responses (HHDM, GOP, SMBIOS, Modules)
// Date: 2026-11-09

//! Limine boot protocol requests and interfaces for OptiOS.

use limine::BaseRevision;
use limine::framebuffer::Framebuffer;
use limine::memory_map::Entry;
use limine::request::{
    FramebufferRequest, HhdmRequest, MemoryMapRequest, ModuleRequest, RequestsEndMarker,
    RequestsStartMarker, SmbiosRequest, StackSizeRequest,
};

/// Start marker required by the modern Limine protocol.
#[used]
#[link_section = ".limine_requests_start_marker"]
static START_MARKER: RequestsStartMarker = RequestsStartMarker::new();

/// Base revision tag signaling bootloader protocol compliance.
#[used]
#[link_section = ".limine_requests"]
pub static BASE_REVISION: BaseRevision = BaseRevision::new();

/// Requests a larger 128 KiB stack for the kernel.
#[used]
#[link_section = ".limine_requests"]
pub static STACK_SIZE_REQUEST: StackSizeRequest = StackSizeRequest::new().with_size(128 * 1024);

/// UEFI GOP Framebuffer request.
#[used]
#[link_section = ".limine_requests"]
pub static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

/// Physical memory map request.
#[used]
#[link_section = ".limine_requests"]
pub static MEMORY_MAP_REQUEST: MemoryMapRequest = MemoryMapRequest::new();

/// Higher-Half Direct Map (HHDM) virtual offset request.
#[used]
#[link_section = ".limine_requests"]
pub static HHDM_REQUEST: HhdmRequest = HhdmRequest::new();

/// SMBIOS / DMI firmware table entry point request.
#[used]
#[link_section = ".limine_requests"]
pub static SMBIOS_REQUEST: SmbiosRequest = SmbiosRequest::new();

/// Bootloader modules request (used to load `/boot/initramfs.tar`).
#[used]
#[link_section = ".limine_requests"]
pub static MODULE_REQUEST: ModuleRequest = ModuleRequest::new();

/// End marker required by the modern Limine protocol.
#[used]
#[link_section = ".limine_requests_end_marker"]
static END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

/// Checks whether the bootloader supports the requested base revision.
pub fn is_base_revision_supported() -> bool {
    BASE_REVISION.is_supported()
}

/// Retrieves the primary framebuffer returned by Limine, if available.
pub fn get_framebuffer<'a>() -> Option<Framebuffer<'a>> {
    FRAMEBUFFER_REQUEST
        .get_response()
        .and_then(|resp| resp.framebuffers().next())
}

/// Retrieves physical memory map entries provided by the bootloader.
pub fn get_memory_map<'a>() -> Option<&'a [&'a Entry]> {
    MEMORY_MAP_REQUEST
        .get_response()
        .map(|resp| resp.entries())
}

/// Retrieves the Higher-Half Direct Map virtual offset.
pub fn get_hhdm_offset() -> Option<u64> {
    HHDM_REQUEST.get_response().map(|resp| resp.offset())
}

/// Retrieves the (entry_32, entry_64) SMBIOS entry point addresses from Limine.
pub fn get_smbios_entry_points() -> (Option<u64>, Option<u64>) {
    if let Some(resp) = SMBIOS_REQUEST.get_response() {
        let e32 = resp.entry_32().map(|nz| nz.get() as u64);
        let e64 = resp.entry_64().map(|nz| nz.get() as u64);
        (e32, e64)
    } else {
        (None, None)
    }
}

/// Retrieves the primary initramfs module slice and its boot path, if loaded by Limine.
pub fn get_initramfs_module() -> Option<(&'static str, &'static [u8])> {
    let resp = MODULE_REQUEST.get_response()?;
    let module = resp.modules().first()?;
    let ptr = module.addr() as *const u8;
    let size = module.size() as usize;
    if ptr.is_null() || size == 0 {
        return None;
    }
    let path = module.path().to_str().unwrap_or("/boot/initramfs.tar");
    let slice = unsafe { core::slice::from_raw_parts(ptr, size) };
    Some((path, slice))
}
