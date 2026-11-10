// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Partition Table Unit Tests
// Date: 2026-11-10

#[cfg(test)]
mod tests {
    use crate::*;
    use alloc::vec;

    #[test]
    fn test_crc32_standard_check_vector() {
        // Standard check vector for IEEE 802.3 CRC-32: "123456789" => 0xCBF43926
        let input = b"123456789";
        let res = crc32(input);
        assert_eq!(res, 0xCBF43926);
    }

    #[test]
    fn test_decode_utf16le_string() {
        // "EFI System" in UTF-16LE followed by zeroes
        let mut buf = [0u8; 72];
        let name = "EFI System";
        for (i, c) in name.chars().enumerate() {
            buf[i * 2] = c as u8;
            buf[i * 2 + 1] = 0;
        }
        let decoded = decode_utf16le_string(&buf);
        assert_eq!(decoded, "EFI System");
    }

    #[test]
    fn test_mbr_parser_protective() {
        let mut sector0 = [0u8; 512];
        sector0[510] = 0x55;
        sector0[511] = 0xAA;

        // Partition 1: Protective MBR (type 0xEE)
        let off = MBR_PARTITION_TABLE_OFFSET;
        sector0[off + 4] = MBR_TYPE_GPT_PROTECTIVE; // 0xEE
        sector0[off + 8] = 0x01; // start_lba = 1
        sector0[off + 12] = 0xFF; // sector_count = 0x0001FFFF
        sector0[off + 13] = 0xFF;
        sector0[off + 14] = 0x01;

        let mbr = parse_mbr(&sector0).expect("Valid protective MBR");
        assert!(mbr.is_protective);
        assert_eq!(mbr.partitions.len(), 1);
        assert_eq!(mbr.partitions[0].part_type, 0xEE);
        assert_eq!(mbr.partitions[0].start_lba, 1);
    }

    #[test]
    fn test_mbr_parser_invalid_signature() {
        let sector0 = [0u8; 512];
        let res = parse_mbr(&sector0);
        assert_eq!(res, Err(PartitionError::InvalidMbrSignature));
    }

    #[test]
    fn test_gpt_parser_valid_and_crc() {
        let mut header = [0u8; 512];
        header[..8].copy_from_slice(GPT_SIGNATURE);
        // Revision 1.0 = 0x00010000
        header[8] = 0x00;
        header[9] = 0x00;
        header[10] = 0x01;
        header[11] = 0x00;
        // Header size = 92
        header[12] = 92;
        header[13] = 0;
        header[14] = 0;
        header[15] = 0;

        // First usable LBA = 34, Last usable LBA = 131038
        header[40] = 34;
        header[48] = (131038 & 0xFF) as u8;
        header[49] = ((131038 >> 8) & 0xFF) as u8;

        // 2 partition entries of 128 bytes each
        header[80] = 2; // num_entries = 2
        header[84] = 128; // entry_size = 128

        let mut entries = vec![0u8; 2 * 128];

        // Entry 0: EFI System Partition
        entries[0..16].copy_from_slice(&GUID_EFI_SYSTEM);
        // Unique GUID
        entries[16] = 0x01;
        // Start LBA = 2048, End LBA = 34815 (32768 sectors = 16 MiB)
        entries[32..40].copy_from_slice(&2048u64.to_le_bytes());
        entries[40..48].copy_from_slice(&34815u64.to_le_bytes());
        // Name "EFI System"
        for (i, c) in "EFI System".chars().enumerate() {
            entries[56 + i * 2] = c as u8;
            entries[56 + i * 2 + 1] = 0;
        }

        // Entry 1: Microsoft Basic Data
        entries[128..144].copy_from_slice(&GUID_MICROSOFT_BASIC_DATA);
        // Start LBA = 34816, End LBA = 129023 (94208 sectors = exactly 46 MiB)
        entries[128 + 32..128 + 40].copy_from_slice(&34816u64.to_le_bytes());
        entries[128 + 40..128 + 48].copy_from_slice(&129023u64.to_le_bytes());
        for (i, c) in "OptiOS Data".chars().enumerate() {
            entries[128 + 56 + i * 2] = c as u8;
            entries[128 + 56 + i * 2 + 1] = 0;
        }

        // Compute array CRC32 and put into header[88..92]
        let array_crc = crc32(&entries);
        header[88..92].copy_from_slice(&array_crc.to_le_bytes());

        // Compute header CRC32 with bytes 16..20 zeroed and put into header[16..20]
        let header_crc = crc32(&header[..92]);
        header[16..20].copy_from_slice(&header_crc.to_le_bytes());

        let table = parse_gpt(&header, &entries).expect("Valid GPT table");
        assert_eq!(table.first_usable_lba, 34);
        assert_eq!(table.partitions.len(), 2);

        assert_eq!(table.partitions[0].name, "EFI System");
        assert_eq!(table.partitions[0].type_name, "EFI System Partition");
        assert_eq!(table.partitions[0].start_lba, 2048);
        assert_eq!(table.partitions[0].end_lba, 34815);
        assert_eq!(table.partitions[0].sector_count, 32768);
        assert_eq!(table.partitions[0].size_mib, 16);

        assert_eq!(table.partitions[1].name, "OptiOS Data");
        assert_eq!(table.partitions[1].type_name, "Microsoft Basic Data");
        assert_eq!(table.partitions[1].start_lba, 34816);
        assert_eq!(table.partitions[1].end_lba, 129023);
        assert_eq!(table.partitions[1].sector_count, 94208);
        assert_eq!(table.partitions[1].size_mib, 46);
    }

    #[test]
    fn test_gpt_parser_crc_mismatch() {
        let mut header = [0u8; 512];
        header[..8].copy_from_slice(GPT_SIGNATURE);
        header[10] = 1; // Revision 1.0
        header[12] = 92; // size
        header[16] = 0xAA; // Corrupt recorded CRC
        header[80] = 1;
        header[84] = 128;
        let entries = vec![0u8; 128];

        let res = parse_gpt(&header, &entries);
        assert!(matches!(res, Err(PartitionError::HeaderCrcMismatch { .. })));
    }
}
