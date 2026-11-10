// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: FAT32 Driver Host Unit Tests
// Date: 2026-11-10

extern crate alloc;
use alloc::vec;
use alloc::vec::Vec;

use super::*;
use optios_ahci_proto::{AhciError, BlockDevice};

struct MemBlockDevice {
    data: Vec<u8>,
}

impl MemBlockDevice {
    fn new(total_sectors: u32) -> Self {
        Self {
            data: vec![0u8; total_sectors as usize * 512],
        }
    }
}

impl BlockDevice for MemBlockDevice {
    fn read_sectors(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), AhciError> {
        let start = (lba as usize) * 512;
        let end = start + (count as usize) * 512;
        if end > self.data.len() || buf.len() < (count as usize) * 512 {
            return Err(AhciError::BufferTooSmall);
        }
        buf[..end - start].copy_from_slice(&self.data[start..end]);
        Ok(())
    }

    fn sector_size(&self) -> u32 {
        512
    }

    fn total_sectors(&self) -> u64 {
        (self.data.len() / 512) as u64
    }
}

/// Helper function to create a minimal synthetic FAT32 image.
fn create_test_fat32_image() -> MemBlockDevice {
    let mut dev = MemBlockDevice::new(200);

    // 1. Sector 0: BPB
    let bpb = &mut dev.data[0..512];
    bpb[510] = 0x55;
    bpb[511] = 0xAA;
    bpb[11] = 0x00; // 512 bytes per sector
    bpb[12] = 0x02;
    bpb[13] = 1;    // 1 sector per cluster
    bpb[14] = 32;   // 32 reserved sectors
    bpb[15] = 0;
    bpb[16] = 2;    // 2 FATs
    bpb[32] = 200;  // 200 total sectors
    bpb[36] = 32;   // 32 sectors per FAT
    bpb[44] = 2;    // Root cluster = 2

    // 2. Sector 32: FAT1
    // Clusters 0, 1 reserved
    let fat_offset = 32 * 512;
    // Entry 0 (0x0FFF_FFF8)
    dev.data[fat_offset..fat_offset + 4].copy_from_slice(&0x0FFF_FFF8u32.to_le_bytes());
    // Entry 1 (0x0FFF_FFFF)
    dev.data[fat_offset + 4..fat_offset + 8].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    // Entry 2: Root dir (0x0FFF_FFFF, EOC)
    dev.data[fat_offset + 8..fat_offset + 12].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    // Entry 3: Short name file (0x0FFF_FFFF, EOC)
    dev.data[fat_offset + 12..fat_offset + 16].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());
    // Entry 4: Multi-cluster LFN file cluster 4 -> cluster 5
    dev.data[fat_offset + 16..fat_offset + 20].copy_from_slice(&5u32.to_le_bytes());
    // Entry 5: Cluster 5 -> EOC
    dev.data[fat_offset + 20..fat_offset + 24].copy_from_slice(&0x0FFF_FFFFu32.to_le_bytes());

    // Copy FAT1 to FAT2 (Sector 64)
    let fat2_offset = 64 * 512;
    dev.data.copy_within(fat_offset..fat_offset + 512, fat2_offset);

    // 3. Cluster 2 (Root Directory): data_start_sector = 32 + 2 * 32 = 96
    let root_offset = 96 * 512;

    // Entry 0: Short 8.3 file "README  TXT", first cluster = 3, size = 13
    let e0 = &mut dev.data[root_offset..root_offset + 32];
    e0[0..11].copy_from_slice(b"README  TXT");
    e0[11] = 0x20; // Archive
    e0[26] = 3;    // Cluster low = 3
    e0[28] = 13;   // Size = 13

    // Entry 1: LFN entry for "recovery_log.txt" (16 chars)
    // LFN 2 (chars 14..16): "txt\0"
    let e1 = &mut dev.data[root_offset + 32..root_offset + 64];
    e1[0] = 0x42; // Last LFN, seq 2
    // Chars 1..5: 't', 'x', 't', 0x0000, 0xFFFF
    e1[1] = b't'; e1[3] = b'x'; e1[5] = b't';
    e1[7] = 0x00; e1[8] = 0x00;
    e1[9] = 0xFF; e1[10] = 0xFF;
    e1[11] = 0x0F; // LFN attribute
    // Chars 6..11: 0xFFFF
    for c in 0..6 {
        e1[14 + c * 2] = 0xFF;
        e1[15 + c * 2] = 0xFF;
    }
    // Chars 12..13: 0xFFFF
    e1[28] = 0xFF; e1[29] = 0xFF;
    e1[30] = 0xFF; e1[31] = 0xFF;

    // LFN 1 (chars 1..13): "recovery_log."
    let e2 = &mut dev.data[root_offset + 64..root_offset + 96];
    e2[0] = 0x01; // seq 1
    let text1 = "recovery_log.";
    let utf16: Vec<u16> = text1.encode_utf16().collect();
    for (i, &u) in utf16.iter().enumerate() {
        let bytes = u.to_le_bytes();
        if i < 5 {
            e2[1 + i * 2] = bytes[0];
            e2[2 + i * 2] = bytes[1];
        } else if i < 11 {
            let idx = i - 5;
            e2[14 + idx * 2] = bytes[0];
            e2[15 + idx * 2] = bytes[1];
        } else {
            let idx = i - 11;
            e2[28 + idx * 2] = bytes[0];
            e2[29 + idx * 2] = bytes[1];
        }
    }
    e2[11] = 0x0F; // LFN attribute

    // 8.3 alias for recovery_log.txt: "RECOVE~1TXT", first cluster = 4, size = 600
    let e3 = &mut dev.data[root_offset + 96..root_offset + 128];
    e3[0..11].copy_from_slice(b"RECOVE~1TXT");
    e3[11] = 0x20;
    e3[26] = 4; // Cluster low = 4
    e3[28..32].copy_from_slice(&600u32.to_le_bytes());

    // 4. Cluster 3 (Sector 97): Data for "README.TXT"
    let c3_offset = 97 * 512;
    dev.data[c3_offset..c3_offset + 13].copy_from_slice(b"Hello, FAT32!");

    // 5. Cluster 4 (Sector 98) & Cluster 5 (Sector 99): Data for "recovery_log.txt" (600 bytes)
    let c4_offset = 98 * 512;
    dev.data[c4_offset..c4_offset + 512].fill(b'A');
    let c5_offset = 99 * 512;
    dev.data[c5_offset..c5_offset + 88].fill(b'B');

    dev
}

#[test]
fn test_bpb_parsing_valid() {
    let dev = create_test_fat32_image();
    let bpb = Fat32Bpb::parse(&dev.data[0..512]).expect("BPB should parse cleanly");
    assert_eq!(bpb.bytes_per_sector, 512);
    assert_eq!(bpb.sectors_per_cluster, 1);
    assert_eq!(bpb.reserved_sectors, 32);
    assert_eq!(bpb.num_fats, 2);
    assert_eq!(bpb.sectors_per_fat, 32);
    assert_eq!(bpb.root_cluster, 2);
    assert_eq!(bpb.fat_start_sector, 32);
    assert_eq!(bpb.data_start_sector, 96);
}

#[test]
fn test_bpb_parsing_invalid_signature() {
    let mut sector = [0u8; 512];
    sector[510] = 0x00;
    sector[511] = 0x00;
    assert_eq!(Fat32Bpb::parse(&sector).err(), Some(Fat32Error::InvalidBootSector));
}

#[test]
fn test_list_root_directory() {
    let dev = create_test_fat32_image();
    let mut fs = Fat32FileSystem::new(dev).expect("FileSystem should mount");
    let entries = fs.list_dir("/").expect("list_dir root should succeed");

    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "README.TXT");
    assert!(!entries[0].is_dir);
    assert_eq!(entries[0].size, 13);

    assert_eq!(entries[1].name, "recovery_log.txt");
    assert!(!entries[1].is_dir);
    assert_eq!(entries[1].size, 600);
}

#[test]
fn test_read_short_name_file() {
    let dev = create_test_fat32_image();
    let mut fs = Fat32FileSystem::new(dev).expect("FileSystem should mount");

    let mut buf = [0u8; 32];
    let n = fs.read_file("/README.TXT", 0, &mut buf).expect("read_file should succeed");
    assert_eq!(n, 13);
    assert_eq!(&buf[..13], b"Hello, FAT32!");
}

#[test]
fn test_read_lfn_multi_cluster_file() {
    let dev = create_test_fat32_image();
    let mut fs = Fat32FileSystem::new(dev).expect("FileSystem should mount");

    let mut buf = vec![0u8; 700];
    let n = fs.read_file("/recovery_log.txt", 0, &mut buf).expect("read_file should succeed");
    assert_eq!(n, 600);
    // First 512 bytes are 'A'
    assert!(buf[..512].iter().all(|&b| b == b'A'));
    // Remaining 88 bytes are 'B'
    assert!(buf[512..600].iter().all(|&b| b == b'B'));
}

#[test]
fn test_stat_file_and_not_found() {
    let dev = create_test_fat32_image();
    let mut fs = Fat32FileSystem::new(dev).expect("FileSystem should mount");

    let st = fs.stat("/recovery_log.txt").expect("stat should find file");
    assert_eq!(st.size, 600);
    assert_eq!(st.first_cluster, 4);

    let err = fs.stat("/nonexistent.file").err();
    assert_eq!(err, Some(Fat32Error::NotFound));
}
