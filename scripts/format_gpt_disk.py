#!/usr/bin/env python3
# OptiOS
# Copyright (C) 2026 João Figueiró. All rights reserved
# Item: Script to Generate a 64 MiB Disk Image with a Valid GPT Partition Table
# Date: 2026-11-10

import os
import struct
import sys
import zlib

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
    p1_end_lba = 2048 + (16 * 1024 * 1024 // sector_size) - 1 # 34815
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
    print(f"==> Formatted {size_mib} MiB GPT disk image at {image_path} with 2 partitions.")

if __name__ == "__main__":
    path = sys.argv[1] if len(sys.argv) > 1 else "build/test_disk.img"
    create_gpt_disk(path)
