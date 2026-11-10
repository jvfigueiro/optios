#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-2.0-or-later
# Copyright (C) 2026 João Figueiró
# Item: Script to Generate a 64 MiB Disk Image with GPT Partition Table & FAT32 Volume
# Date: 2026-11-10

import os
import struct
import sys
import zlib

def format_fat32_partition(image: bytearray, start_lba: int, total_sectors: int):
    sector_size = 512
    p_offset = start_lba * sector_size

    # FAT32 Parameters
    bytes_per_sector = 512
    sectors_per_cluster = 1
    reserved_sectors = 32
    num_fats = 2
    sectors_per_fat = 256
    root_cluster = 2

    # 1. BPB (Sector 0)
    bpb = bytearray(sector_size)
    bpb[0:3] = b"\xEB\x58\x90"
    bpb[3:11] = b"MSWIN4.1"
    struct.pack_into("<H", bpb, 11, bytes_per_sector)
    bpb[13] = sectors_per_cluster
    struct.pack_into("<H", bpb, 14, reserved_sectors)
    bpb[16] = num_fats
    bpb[21] = 0xF8 # Media: Fixed disk
    struct.pack_into("<I", bpb, 32, total_sectors)
    struct.pack_into("<I", bpb, 36, sectors_per_fat)
    struct.pack_into("<I", bpb, 44, root_cluster)
    bpb[66] = 0x29
    struct.pack_into("<I", bpb, 67, 0x12345678)
    bpb[71:82] = b"OPTIO_REC  "
    bpb[82:90] = b"FAT32   "
    bpb[510] = 0x55
    bpb[511] = 0xAA
    image[p_offset : p_offset + sector_size] = bpb

    # 2. FAT1 & FAT2
    fat1_offset = p_offset + (reserved_sectors * sector_size)
    fat2_offset = p_offset + ((reserved_sectors + sectors_per_fat) * sector_size)

    fat_data = bytearray(sectors_per_fat * sector_size)
    # Cluster 0 & 1 reserved
    struct.pack_into("<I", fat_data, 0, 0x0FFFFFF8)
    struct.pack_into("<I", fat_data, 4, 0x0FFFFFFF)
    # Cluster 2 (Root Dir): EOC
    struct.pack_into("<I", fat_data, 8, 0x0FFFFFFF)
    # Cluster 3 (recovery_log.txt): EOC
    struct.pack_into("<I", fat_data, 12, 0x0FFFFFFF)
    # Cluster 4 (README.TXT): EOC
    struct.pack_into("<I", fat_data, 16, 0x0FFFFFFF)

    image[fat1_offset : fat1_offset + len(fat_data)] = fat_data
    image[fat2_offset : fat2_offset + len(fat_data)] = fat_data

    # 3. Data area: Cluster 2 (Root Directory)
    data_start_sector = reserved_sectors + (num_fats * sectors_per_fat) # 32 + 512 = 544
    root_dir_offset = p_offset + (data_start_sector * sector_size)

    # Directory entries
    root_dir = bytearray(sectors_per_cluster * sector_size)

    # Entry 0: Volume label
    root_dir[0:11] = b"OPTIO_REC  "
    root_dir[11] = 0x08

    # Entry 1 & 2 & 3: recovery_log.txt
    # LFN 2 (seq 2 | 0x40): "txt\0"
    e1 = bytearray(32)
    e1[0] = 0x42
    # chars 1..5: 't', 'x', 't', 0x00, 0xFF
    e1[1] = ord('t'); e1[3] = ord('x'); e1[5] = ord('t')
    e1[7] = 0x00; e1[8] = 0x00
    e1[9] = 0xFF; e1[10] = 0xFF
    e1[11] = 0x0F
    for c in range(6):
        e1[14 + c * 2] = 0xFF; e1[15 + c * 2] = 0xFF
    e1[28] = 0xFF; e1[29] = 0xFF; e1[30] = 0xFF; e1[31] = 0xFF

    # LFN 1 (seq 1): "recovery_log."
    e2 = bytearray(32)
    e2[0] = 0x01
    txt1 = "recovery_log.".encode("utf-16le")
    for i in range(13):
        b0 = txt1[i*2]
        b1 = txt1[i*2+1]
        if i < 5:
            e2[1 + i*2] = b0; e2[2 + i*2] = b1
        elif i < 11:
            idx = i - 5
            e2[14 + idx*2] = b0; e2[15 + idx*2] = b1
        else:
            idx = i - 11
            e2[28 + idx*2] = b0; e2[29 + idx*2] = b1
    e2[11] = 0x0F

    recovery_text = (
        "OptiOS Hardware Diagnostic & System Recovery Log\n"
        "Target: /dev/sda1 (AHCI Port 0 Partition 1)\n"
        "Status: Clean Recovery Data Verified\n"
    ).encode("ascii")

    # 8.3 Entry for recovery_log.txt
    e3 = bytearray(32)
    e3[0:11] = b"RECOVE~1TXT"
    e3[11] = 0x20 # Archive
    struct.pack_into("<H", e3, 26, 3) # Cluster 3
    struct.pack_into("<I", e3, 28, len(recovery_text))

    # Entry 4: README.TXT (Cluster 4)
    readme_text = (
        "Welcome to OptiOS FAT32 Recovery Partition!\n"
        "Mountpoint: /mnt/sda1\n"
    ).encode("ascii")
    e4 = bytearray(32)
    e4[0:11] = b"README  TXT"
    e4[11] = 0x20
    struct.pack_into("<H", e4, 26, 4) # Cluster 4
    struct.pack_into("<I", e4, 28, len(readme_text))

    root_dir[32:64] = e1
    root_dir[64:96] = e2
    root_dir[96:128] = e3
    root_dir[128:160] = e4
    image[root_dir_offset : root_dir_offset + len(root_dir)] = root_dir

    # 4. Cluster 3: recovery_log.txt data
    c3_offset = p_offset + ((data_start_sector + (3 - 2) * sectors_per_cluster) * sector_size)
    image[c3_offset : c3_offset + len(recovery_text)] = recovery_text

    # 5. Cluster 4: README.TXT data
    c4_offset = p_offset + ((data_start_sector + (4 - 2) * sectors_per_cluster) * sector_size)
    image[c4_offset : c4_offset + len(readme_text)] = readme_text

def create_gpt_disk(image_path: str, size_mib: int = 64):
    sector_size = 512
    total_sectors = (size_mib * 1024 * 1024) // sector_size
    image = bytearray(total_sectors * sector_size)

    # 1. Protective MBR (LBA 0)
    # Offset 446: Partition Entry 1
    # Type 0xEE (GPT Protective), Start LBA 1, Sector Count = total_sectors - 1
    struct.pack_into(
        "<B3sB3sII",
        image,
        446,
        0x00,           # Boot indicator
        b"\x00\x02\x00",# Starting CHS
        0xEE,           # OS Type: GPT Protective
        b"\xFF\xFF\xFF",# Ending CHS
        1,              # Starting LBA
        min(0xFFFFFFFF, total_sectors - 1) # Sector count
    )
    # MBR Signature 0x55AA
    image[510] = 0x55
    image[511] = 0xAA

    # 2. Partition Entries Array (LBAs 2..=33 = 128 entries * 128 bytes = 16384 bytes)
    entries = bytearray(128 * 128)

    # Partition 1: EFI System Partition (16 MiB)
    # GUID C12A7328-F81F-11D2-BA4B-00A0C93EC93B
    p1_type_guid = bytes([
        0x28, 0x73, 0x2A, 0xC1, 0x1F, 0xF8, 0xD2, 0x11,
        0xBA, 0x4B, 0x00, 0xA0, 0xC9, 0x3E, 0xC9, 0x3B
    ])
    p1_unique_guid = b"\x11" * 16
    p1_start_lba = 2048
    p1_sectors = 16 * 1024 * 1024 // sector_size # 32768 sectors
    p1_end_lba = p1_start_lba + p1_sectors - 1   # 34815
    p1_attrs = 0
    p1_name = "EFI System Partition".encode("utf-16le")

    struct.pack_into(
        "<16s16sQQQ",
        entries,
        0,
        p1_type_guid,
        p1_unique_guid,
        p1_start_lba,
        p1_end_lba,
        p1_attrs
    )
    entries[56:56 + len(p1_name)] = p1_name

    # Format Partition 1 as FAT32 filesystem containing recovery_log.txt
    format_fat32_partition(image, p1_start_lba, p1_sectors)

    # Partition 2: OptiOS Customer Data (46 MiB)
    # GUID EBD0A0A2-B9E5-4433-87C0-68B6B72699C7
    p2_type_guid = bytes([
        0xA2, 0xA0, 0xD0, 0xEB, 0xE5, 0xB9, 0x33, 0x44,
        0x87, 0xC0, 0x68, 0xB6, 0xB7, 0x26, 0x99, 0xC7
    ])
    p2_unique_guid = b"\x22" * 16
    p2_start_lba = 34816
    p2_end_lba = 34816 + (46 * 1024 * 1024 // sector_size) - 1 # 129023
    p2_attrs = 0
    p2_name = "OptiOS Customer Data".encode("utf-16le")

    struct.pack_into(
        "<16s16sQQQ",
        entries,
        128,
        p2_type_guid,
        p2_unique_guid,
        p2_start_lba,
        p2_end_lba,
        p2_attrs
    )
    entries[128 + 56:128 + 56 + len(p2_name)] = p2_name

    # Copy partition array to LBA 2 (bytes 1024..17408)
    image[2 * sector_size : 2 * sector_size + len(entries)] = entries

    # 3. GPT Header (LBA 1, bytes 512..1024)
    array_crc = zlib.crc32(entries) & 0xFFFFFFFF
    header = bytearray(sector_size)

    sig = b"EFI PART"
    rev = 0x00010000
    hdr_size = 92
    current_lba = 1
    backup_lba = total_sectors - 1
    first_usable = 34
    last_usable = total_sectors - 34
    disk_guid = b"\x42" * 16
    part_entries_lba = 2
    num_parts = 128
    part_entry_size = 128

    struct.pack_into(
        "<8sIIIIQQQQ16sQIII",
        header,
        0,
        sig,
        rev,
        hdr_size,
        0,              # Header CRC32 placeholder (offset 16)
        0,              # Reserved
        current_lba,
        backup_lba,
        first_usable,
        last_usable,
        disk_guid,
        part_entries_lba,
        num_parts,
        part_entry_size,
        array_crc
    )

    hdr_crc = zlib.crc32(header[:hdr_size]) & 0xFFFFFFFF
    struct.pack_into("<I", header, 16, hdr_crc)

    image[sector_size : 2 * sector_size] = header

    # 4. Backup GPT: Entries at (total_sectors - 33) and Backup Header at (total_sectors - 1)
    backup_entries_lba = total_sectors - 33
    image[backup_entries_lba * sector_size : (backup_entries_lba + 32) * sector_size] = entries

    backup_header = bytearray(header)
    struct.pack_into("<QQ", backup_header, 24, backup_lba, current_lba) # swap current/backup LBA
    struct.pack_into("<Q", backup_header, 72, backup_entries_lba)
    struct.pack_into("<I", backup_header, 16, 0)
    backup_hdr_crc = zlib.crc32(backup_header[:hdr_size]) & 0xFFFFFFFF
    struct.pack_into("<I", backup_header, 16, backup_hdr_crc)

    image[backup_lba * sector_size : (backup_lba + 1) * sector_size] = backup_header

    # Write out to disk
    os.makedirs(os.path.dirname(os.path.abspath(image_path)), exist_ok=True)
    with open(image_path, "wb") as f:
        f.write(image)
    print(f"==> Formatted {size_mib} MiB GPT disk image at {image_path} with FAT32 on Partition 1.")

if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else "build/test_disk.img"
    create_gpt_disk(path)
