// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Pure ELF64 Parser Unit Tests
// Date: 2026-11-11

#[cfg(test)]
mod tests {
    use crate::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn build_minimal_elf64(vaddr: u64, memsz: u64, filesz: u64, entry: u64) -> Vec<u8> {
        let mut data = vec![0u8; 128];
        // e_ident
        data[0..4].copy_from_slice(&ELF_MAGIC);
        data[4] = ELF_CLASS_64;
        data[5] = ELF_DATA_2LSB;
        data[6] = ELF_VERSION_CURRENT;
        // e_type = ET_EXEC
        data[16..18].copy_from_slice(&ET_EXEC.to_le_bytes());
        // e_machine = EM_X86_64
        data[18..20].copy_from_slice(&EM_X86_64.to_le_bytes());
        // e_version = 1
        data[20..24].copy_from_slice(&1u32.to_le_bytes());
        // e_entry
        data[24..32].copy_from_slice(&entry.to_le_bytes());
        // e_phoff = 64
        data[32..40].copy_from_slice(&64u64.to_le_bytes());
        // e_ehsize = 64
        data[52..54].copy_from_slice(&64u16.to_le_bytes());
        // e_phentsize = 56
        data[54..56].copy_from_slice(&56u16.to_le_bytes());
        // e_phnum = 1
        data[56..58].copy_from_slice(&1u16.to_le_bytes());

        // Program Header at 64
        let ph_offset = 64;
        // p_type = PT_LOAD
        data[ph_offset..ph_offset + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        // p_flags = PF_R | PF_X
        data[ph_offset + 4..ph_offset + 8].copy_from_slice(&(PF_R | PF_X).to_le_bytes());
        // p_offset = 0
        data[ph_offset + 8..ph_offset + 16].copy_from_slice(&0u64.to_le_bytes());
        // p_vaddr
        data[ph_offset + 16..ph_offset + 24].copy_from_slice(&vaddr.to_le_bytes());
        // p_paddr
        data[ph_offset + 24..ph_offset + 32].copy_from_slice(&vaddr.to_le_bytes());
        // p_filesz
        data[ph_offset + 32..ph_offset + 40].copy_from_slice(&filesz.to_le_bytes());
        // p_memsz
        data[ph_offset + 40..ph_offset + 48].copy_from_slice(&memsz.to_le_bytes());
        // p_align = 4096
        data[ph_offset + 48..ph_offset + 56].copy_from_slice(&4096u64.to_le_bytes());

        data
    }

    #[test]
    fn test_valid_minimal_elf64() {
        let vaddr = 0x0000_0000_0040_0000;
        let elf_bytes = build_minimal_elf64(vaddr, 4096, 128, vaddr);
        let res = parse_elf(&elf_bytes);
        assert!(res.is_ok(), "Expected valid ELF parse, got: {:?}", res.err());
        let bin = res.unwrap();
        assert_eq!(bin.entry_point, vaddr);
        assert_eq!(bin.segments.len(), 1);
        assert_eq!(bin.segments[0].vaddr, vaddr);
        assert_eq!(bin.segments[0].mem_size, 4096);
    }

    #[test]
    fn test_invalid_magic() {
        let mut elf_bytes = build_minimal_elf64(0x400000, 4096, 128, 0x400000);
        elf_bytes[0] = 0x00;
        assert_eq!(parse_elf(&elf_bytes).err(), Some(ElfError::InvalidMagic));
    }

    #[test]
    fn test_wrong_architecture() {
        let mut elf_bytes = build_minimal_elf64(0x400000, 4096, 128, 0x400000);
        // Change EM_X86_64 (62) to ARM (40)
        elf_bytes[18..20].copy_from_slice(&40u16.to_le_bytes());
        assert_eq!(parse_elf(&elf_bytes).err(), Some(ElfError::InvalidArchitecture));
    }

    #[test]
    fn test_kernel_address_overlap_rejected() {
        // Attempt to map segment into kernel higher half
        let kernel_vaddr = 0xFFFF_8000_0000_0000;
        let elf_bytes = build_minimal_elf64(kernel_vaddr, 4096, 128, kernel_vaddr);
        assert_eq!(
            parse_elf(&elf_bytes).err(),
            Some(ElfError::KernelAddressOverlap),
            "Should reject user segments mapped into kernel address space"
        );
    }

    #[test]
    fn test_filesz_exceeds_memsz() {
        let elf_bytes = build_minimal_elf64(0x400000, 64, 128, 0x400000);
        assert_eq!(
            parse_elf(&elf_bytes).err(),
            Some(ElfError::FileSizeExceedsMemSize)
        );
    }

    #[test]
    fn test_truncated_buffer() {
        let data = [0x7Fu8, b'E', b'L', b'F', 2, 1, 1];
        assert_eq!(parse_elf(&data).err(), Some(ElfError::BufferTooSmall));
    }

    #[test]
    fn test_elf_wx_segment_violation_rejected() {
        let mut elf_bytes = build_minimal_elf64(0x400000, 4096, 128, 0x400000);
        let ph_offset = 64;
        // Mark segment as simultaneously writable and executable (PF_W | PF_X)
        elf_bytes[ph_offset + 4..ph_offset + 8].copy_from_slice(&(PF_W | PF_X).to_le_bytes());
        assert_eq!(
            parse_elf(&elf_bytes).err(),
            Some(ElfError::WxViolation),
            "Should reject ELF segment that is simultaneously writable and executable (W^X violation)"
        );
    }
}
