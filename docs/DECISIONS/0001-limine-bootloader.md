# 1. Adoption of Limine as Initial Bootloader

* **Date:** 2026-11-09
* **Status:** Accepted
* **Context:** OptiOS initial bootstrap and x86-64 UEFI boot

## Context

OptiOS is an operating system designed initially for computer diagnostics and recovery booted via UEFI removable USB media.
Developing a custom UEFI bootloader from scratch or maintaining legacy multiboot stages would divert critical development resources during early kernel bootstrap.

## Decision

Adopt **Limine** as the initial bootloader for OptiOS on x86-64 UEFI platforms.

Technical rationale:
1. **Native UEFI & Long Mode Support:** Delivers control directly in 64-bit Long Mode with paging enabled and higher-half virtual address mapping.
2. **Open, Simple Protocol:** Clean C structs and tag requests with explicit versioning and responses for HHDM, Framebuffer GOP, and Memory Map.
3. **Robustness:** Native support for static ELF64 binaries without custom packaging wrappers.
4. **Independent Rust no_std Binding:** Can be consumed directly in `no_std` environments without runtime overhead.

## Consequences

- The kernel must define request structures in a dedicated ELF section (`.limine_requests`).
- The linker script maps the kernel in the virtual higher half (`0xffffffff80000000`).
- Boot protocol logic is isolated in `src/boot/`, decoupling the kernel core from future bootloader migrations.
