// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: Pure FAT32 Filesystem Driver with Long File Name (LFN) Support
// Date: 2026-11-10

//! Pure `#![no_std]` Read-Only FAT32 driver with LFN support.
//! Operates on any storage backend implementing the `BlockDevice` trait.

#![no_std]

extern crate alloc;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use optios_ahci_proto::{AhciError, BlockDevice};

/// Error conditions encountered during FAT32 operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fat32Error {
    InvalidBootSector,
    InvalidCluster,
    IoError,
    NotFound,
    NotAFile,
    NotADirectory,
    CorruptClusterChain,
}

impl From<AhciError> for Fat32Error {
    fn from(_: AhciError) -> Self {
        Fat32Error::IoError
    }
}

/// Parsed BIOS Parameter Block (BPB) for FAT32 volumes.
#[derive(Debug, Clone, Copy)]
pub struct Fat32Bpb {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sectors: u16,
    pub num_fats: u8,
    pub total_sectors: u32,
    pub sectors_per_fat: u32,
    pub root_cluster: u32,
    pub fat_start_sector: u64,
    pub data_start_sector: u64,
}

impl Fat32Bpb {
    /// Parses and validates the FAT32 BPB from the boot sector (Sector 0).
    pub fn parse(sector: &[u8]) -> Result<Self, Fat32Error> {
        if sector.len() < 512 {
            return Err(Fat32Error::InvalidBootSector);
        }

        // Validate 0x55AA boot sector signature
        if sector[510] != 0x55 || sector[511] != 0xAA {
            return Err(Fat32Error::InvalidBootSector);
        }

        let bytes_per_sector = u16::from_le_bytes([sector[11], sector[12]]);
        if bytes_per_sector != 512 {
            return Err(Fat32Error::InvalidBootSector);
        }

        let sectors_per_cluster = sector[13];
        if sectors_per_cluster == 0 || !sectors_per_cluster.is_power_of_two() {
            return Err(Fat32Error::InvalidBootSector);
        }

        let reserved_sectors = u16::from_le_bytes([sector[14], sector[15]]);
        if reserved_sectors == 0 {
            return Err(Fat32Error::InvalidBootSector);
        }

        let num_fats = sector[16];
        if num_fats == 0 {
            return Err(Fat32Error::InvalidBootSector);
        }

        // For FAT32, sectors_per_fat_16 must be 0
        let sectors_per_fat_16 = u16::from_le_bytes([sector[22], sector[23]]);
        let sectors_per_fat_32 = u32::from_le_bytes([sector[36], sector[37], sector[38], sector[39]]);
        let sectors_per_fat = if sectors_per_fat_16 != 0 {
            sectors_per_fat_16 as u32
        } else {
            sectors_per_fat_32
        };

        if sectors_per_fat == 0 {
            return Err(Fat32Error::InvalidBootSector);
        }

        let total_sectors_16 = u16::from_le_bytes([sector[19], sector[20]]);
        let total_sectors_32 = u32::from_le_bytes([sector[32], sector[33], sector[34], sector[35]]);
        let total_sectors = if total_sectors_16 != 0 {
            total_sectors_16 as u32
        } else {
            total_sectors_32
        };

        let root_cluster = u32::from_le_bytes([sector[44], sector[45], sector[46], sector[47]]);
        if root_cluster < 2 {
            return Err(Fat32Error::InvalidBootSector);
        }

        let fat_start_sector = reserved_sectors as u64;
        let data_start_sector = fat_start_sector + (num_fats as u64 * sectors_per_fat as u64);

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sectors,
            num_fats,
            total_sectors,
            sectors_per_fat,
            root_cluster,
            fat_start_sector,
            data_start_sector,
        })
    }
}

/// A directory or file entry in a FAT32 filesystem.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FatDirEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u32,
    pub first_cluster: u32,
}

/// BlockDevice wrapper that offsets all sector accesses to a partition's start LBA.
pub struct PartitionBlockDevice<D: BlockDevice> {
    pub dev: D,
    pub start_lba: u64,
    pub total_sectors: u64,
}

impl<D: BlockDevice> PartitionBlockDevice<D> {
    pub fn new(dev: D, start_lba: u64, total_sectors: u64) -> Self {
        Self {
            dev,
            start_lba,
            total_sectors,
        }
    }
}

impl<D: BlockDevice> BlockDevice for PartitionBlockDevice<D> {
    fn read_sectors(&mut self, lba: u64, count: u32, buf: &mut [u8]) -> Result<(), AhciError> {
        if lba + count as u64 > self.total_sectors {
            return Err(AhciError::BufferTooSmall);
        }
        self.dev.read_sectors(self.start_lba + lba, count, buf)
    }

    fn sector_size(&self) -> u32 {
        self.dev.sector_size()
    }

    fn total_sectors(&self) -> u64 {
        self.total_sectors
    }
}

/// Read-Only FAT32 filesystem driver.
pub struct Fat32FileSystem<D: BlockDevice> {
    dev: D,
    bpb: Fat32Bpb,
}

impl<D: BlockDevice> Fat32FileSystem<D> {
    /// Initializes and mounts a FAT32 filesystem from the given block device.
    pub fn new(mut dev: D) -> Result<Self, Fat32Error> {
        let mut boot_sector = [0u8; 512];
        dev.read_sectors(0, 1, &mut boot_sector)?;
        let bpb = Fat32Bpb::parse(&boot_sector)?;

        Ok(Self { dev, bpb })
    }

    /// Returns the parsed BPB parameters.
    pub fn bpb(&self) -> &Fat32Bpb {
        &self.bpb
    }

    /// Reads the next cluster in a cluster chain from the active File Allocation Table (FAT1).
    pub fn next_cluster(&mut self, cluster: u32) -> Result<Option<u32>, Fat32Error> {
        if cluster < 2 {
            return Err(Fat32Error::InvalidCluster);
        }

        let fat_offset = (cluster as u64) * 4;
        let fat_sector = self.bpb.fat_start_sector + (fat_offset / self.bpb.bytes_per_sector as u64);
        let offset_in_sector = (fat_offset % self.bpb.bytes_per_sector as u64) as usize;

        let mut sector_buf = [0u8; 512];
        self.dev.read_sectors(fat_sector, 1, &mut sector_buf)?;

        let next_val = u32::from_le_bytes([
            sector_buf[offset_in_sector],
            sector_buf[offset_in_sector + 1],
            sector_buf[offset_in_sector + 2],
            sector_buf[offset_in_sector + 3],
        ]) & 0x0FFF_FFFF;

        if next_val >= 0x0FFF_FFF8 {
            Ok(None) // End of Cluster Chain (EOC)
        } else if next_val == 0x0FFF_FFF7 {
            Err(Fat32Error::CorruptClusterChain) // Bad cluster
        } else if next_val < 2 {
            Err(Fat32Error::CorruptClusterChain)
        } else {
            Ok(Some(next_val))
        }
    }

    /// Traverses and collects all clusters in a chain starting at `start_cluster`.
    pub fn read_cluster_chain(&mut self, start_cluster: u32) -> Result<Vec<u32>, Fat32Error> {
        let mut chain = Vec::new();
        let mut cur = start_cluster;
        chain.push(cur);

        for _ in 0..65536 {
            match self.next_cluster(cur)? {
                Some(next) => {
                    chain.push(next);
                    cur = next;
                }
                None => break,
            }
        }

        Ok(chain)
    }

    /// Calculates the starting physical LBA sector for a given data cluster number (>= 2).
    pub fn cluster_lba(&self, cluster: u32) -> u64 {
        self.bpb.data_start_sector + ((cluster - 2) as u64 * self.bpb.sectors_per_cluster as u64)
    }

    /// Reads directory entries from all clusters in a directory's cluster chain.
    pub fn read_dir_chain(&mut self, start_cluster: u32) -> Result<Vec<FatDirEntry>, Fat32Error> {
        let chain = self.read_cluster_chain(start_cluster)?;
        let mut entries = Vec::new();
        let cluster_bytes = (self.bpb.sectors_per_cluster as usize) * (self.bpb.bytes_per_sector as usize);
        let mut cluster_buf = vec![0u8; cluster_bytes];

        // LFN accumulation buffer (stores pairs of (index, text))
        let mut lfn_parts: Vec<(u8, String)> = Vec::new();

        for cluster in chain {
            let lba = self.cluster_lba(cluster);
            self.dev.read_sectors(lba, self.bpb.sectors_per_cluster as u32, &mut cluster_buf)?;

            let num_entries = cluster_bytes / 32;
            for i in 0..num_entries {
                let entry = &cluster_buf[i * 32..(i + 1) * 32];
                let first_byte = entry[0];

                if first_byte == 0x00 {
                    // No more entries in directory
                    return Ok(entries);
                }
                if first_byte == 0xE5 {
                    // Deleted entry: clear LFN accumulator
                    lfn_parts.clear();
                    continue;
                }

                let attr = entry[11];
                if attr == 0x0F {
                    // Long File Name (LFN) Entry
                    let seq = entry[0] & 0x1F;
                    let mut chars = Vec::new();

                    // Chars 1..5 (offsets 1..11)
                    for c_idx in 0..5 {
                        let off = 1 + c_idx * 2;
                        chars.push(u16::from_le_bytes([entry[off], entry[off + 1]]));
                    }
                    // Chars 6..11 (offsets 14..26)
                    for c_idx in 0..6 {
                        let off = 14 + c_idx * 2;
                        chars.push(u16::from_le_bytes([entry[off], entry[off + 1]]));
                    }
                    // Chars 12..13 (offsets 28..32)
                    for c_idx in 0..2 {
                        let off = 28 + c_idx * 2;
                        chars.push(u16::from_le_bytes([entry[off], entry[off + 1]]));
                    }

                    let mut s = String::new();
                    for &c in &chars {
                        if c == 0x0000 || c == 0xFFFF {
                            break;
                        }
                        if let Some(ch) = core::char::from_u32(c as u32) {
                            s.push(ch);
                        }
                    }

                    lfn_parts.push((seq, s));
                    continue;
                }

                // Volume label entry: skip
                if (attr & 0x08) != 0 {
                    lfn_parts.clear();
                    continue;
                }

                // Standard 8.3 Directory Entry
                let name = if !lfn_parts.is_empty() {
                    // Reassemble LFN in ascending sequence order
                    lfn_parts.sort_by_key(|&(seq, _)| seq);
                    let mut full_name = String::new();
                    for (_, part) in &lfn_parts {
                        full_name.push_str(part);
                    }
                    lfn_parts.clear();
                    full_name
                } else {
                    format_short_name(&entry[0..11])
                };

                let is_dir = (attr & 0x10) != 0;
                let size = u32::from_le_bytes([entry[28], entry[29], entry[30], entry[31]]);
                let cluster_hi = u16::from_le_bytes([entry[20], entry[21]]) as u32;
                let cluster_lo = u16::from_le_bytes([entry[26], entry[27]]) as u32;
                let first_cluster = (cluster_hi << 16) | cluster_lo;

                // Skip '.' and '..'
                if name != "." && name != ".." && !name.is_empty() {
                    entries.push(FatDirEntry {
                        name,
                        is_dir,
                        size,
                        first_cluster,
                    });
                }
            }
        }

        Ok(entries)
    }

    /// Lists directory entries for a given path (e.g. `/` or `/subdir`).
    pub fn list_dir(&mut self, path: &str) -> Result<Vec<FatDirEntry>, Fat32Error> {
        let clean = path.trim_matches('/');
        if clean.is_empty() {
            return self.read_dir_chain(self.bpb.root_cluster);
        }

        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        let mut cur_cluster = self.bpb.root_cluster;

        for (idx, part) in parts.iter().enumerate() {
            let entries = self.read_dir_chain(cur_cluster)?;
            let found = entries
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(part))
                .ok_or(Fat32Error::NotFound)?;

            if idx == parts.len() - 1 {
                if found.is_dir {
                    return self.read_dir_chain(found.first_cluster);
                } else {
                    return Err(Fat32Error::NotADirectory);
                }
            } else {
                if !found.is_dir {
                    return Err(Fat32Error::NotADirectory);
                }
                cur_cluster = found.first_cluster;
            }
        }

        Err(Fat32Error::NotFound)
    }

    /// Queries file or directory metadata for a given path.
    pub fn stat(&mut self, path: &str) -> Result<FatDirEntry, Fat32Error> {
        let clean = path.trim_matches('/');
        if clean.is_empty() {
            return Ok(FatDirEntry {
                name: String::from("/"),
                is_dir: true,
                size: 0,
                first_cluster: self.bpb.root_cluster,
            });
        }

        let parts: Vec<&str> = clean.split('/').filter(|s| !s.is_empty()).collect();
        let mut cur_cluster = self.bpb.root_cluster;

        for (idx, part) in parts.iter().enumerate() {
            let entries = self.read_dir_chain(cur_cluster)?;
            let found = entries
                .into_iter()
                .find(|e| e.name.eq_ignore_ascii_case(part))
                .ok_or(Fat32Error::NotFound)?;

            if idx == parts.len() - 1 {
                return Ok(found);
            } else {
                if !found.is_dir {
                    return Err(Fat32Error::NotADirectory);
                }
                cur_cluster = found.first_cluster;
            }
        }

        Err(Fat32Error::NotFound)
    }

    /// Reads up to `out.len()` bytes from the file at `path` starting at byte `offset`.
    pub fn read_file(&mut self, path: &str, offset: usize, out: &mut [u8]) -> Result<usize, Fat32Error> {
        let entry = self.stat(path)?;
        if entry.is_dir {
            return Err(Fat32Error::NotAFile);
        }

        let file_size = entry.size as usize;
        if offset >= file_size {
            return Ok(0);
        }

        let to_read = (file_size - offset).min(out.len());
        let cluster_bytes = (self.bpb.sectors_per_cluster as usize) * (self.bpb.bytes_per_sector as usize);
        let chain = self.read_cluster_chain(entry.first_cluster)?;

        let mut cluster_buf = vec![0u8; cluster_bytes];
        let mut current_offset = 0;
        let mut bytes_written = 0;

        for cluster in chain {
            let cluster_end = current_offset + cluster_bytes;
            if offset + to_read <= current_offset {
                break;
            }

            if offset < cluster_end && (offset + to_read) > current_offset {
                let lba = self.cluster_lba(cluster);
                self.dev.read_sectors(lba, self.bpb.sectors_per_cluster as u32, &mut cluster_buf)?;

                let start_in_cluster = if offset > current_offset {
                    offset - current_offset
                } else {
                    0
                };
                let end_in_cluster = ((offset + to_read) - current_offset).min(cluster_bytes);
                let slice = &cluster_buf[start_in_cluster..end_in_cluster];

                out[bytes_written..bytes_written + slice.len()].copy_from_slice(slice);
                bytes_written += slice.len();
            }

            current_offset += cluster_bytes;
        }

        Ok(bytes_written)
    }
}

/// Converts an 11-byte short 8.3 FAT filename into a standard string representation.
fn format_short_name(raw: &[u8]) -> String {
    let name_part = core::str::from_utf8(&raw[0..8]).unwrap_or("").trim_end();
    let ext_part = core::str::from_utf8(&raw[8..11]).unwrap_or("").trim_end();

    if ext_part.is_empty() {
        String::from(name_part)
    } else {
        format!("{}.{}", name_part, ext_part)
    }
}

#[cfg(test)]
mod tests;
