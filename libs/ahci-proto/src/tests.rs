// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: AHCI Protocol Unit Tests
// Date: 2026-11-10

#[cfg(test)]
mod tests {
    use crate::*;

    #[test]
    fn test_h2d_register_fis_builder() {
        let mut fis = [0u8; 20];
        let res = build_h2d_register_fis(AllowedCommand::IdentifyDevice, &mut fis);
        assert!(res.is_ok());
        assert_eq!(fis[0], FIS_TYPE_REG_H2D);
        assert_eq!(fis[1], 0x80, "Command bit C must be set");
        assert_eq!(fis[2], 0xEC, "Command byte must be 0xEC (IDENTIFY DEVICE)");
        assert_eq!(fis[7], 0x00, "Device register must be 0");
    }

    #[test]
    fn test_h2d_register_fis_buffer_too_small() {
        let mut fis = [0u8; 19];
        let res = build_h2d_register_fis(AllowedCommand::IdentifyDevice, &mut fis);
        assert_eq!(res, Err(AhciError::BufferTooSmall));
    }

    #[test]
    fn test_build_read_dma_ext_fis() {
        let mut fis = [0u8; 20];
        let lba = 0x1234_5678_9ABC;
        let count = 8;
        let res = build_read_dma_ext_fis(lba, count, &mut fis);
        assert!(res.is_ok());
        assert_eq!(fis[0], FIS_TYPE_REG_H2D);
        assert_eq!(fis[1], 0x80);
        assert_eq!(fis[2], 0x25, "Command must be 0x25 (READ DMA EXT)");
        assert_eq!(fis[4], 0xBC, "LBA Low (bits 7:0)");
        assert_eq!(fis[5], 0x9A, "LBA Mid (bits 15:8)");
        assert_eq!(fis[6], 0x78, "LBA High (bits 23:16)");
        assert_eq!(fis[7], 0x40, "Device register bit 6 (LBA mode)");
        assert_eq!(fis[8], 0x56, "LBA Exp (bits 31:24)");
        assert_eq!(fis[9], 0x34, "LBA Mid Exp (bits 39:32)");
        assert_eq!(fis[10], 0x12, "LBA High Exp (bits 47:40)");
        assert_eq!(fis[12], 8, "Sector count low");
        assert_eq!(fis[13], 0, "Sector count high");
    }

    #[test]
    fn test_build_smart_read_data_fis() {
        let mut fis = [0u8; 20];
        let res = build_smart_read_data_fis(&mut fis);
        assert!(res.is_ok());
        assert_eq!(fis[0], FIS_TYPE_REG_H2D);
        assert_eq!(fis[1], 0x80);
        assert_eq!(fis[2], 0xB0, "Command 0xB0 (SMART)");
        assert_eq!(fis[3], 0xD0, "Feature 0xD0 (SMART READ DATA)");
        assert_eq!(fis[5], 0x4F, "LBA Mid constant 0x4F");
        assert_eq!(fis[6], 0xC2, "LBA High constant 0xC2");
        assert_eq!(fis[12], 0x01, "Sector count 1");
    }

    #[test]
    fn test_command_header_builder_readonly() {
        let mut hdr = [0u32; 4];
        let ctba = 0x1_2345_6000u64;
        let res = build_command_header(5, 1, ctba, &mut hdr);
        assert!(res.is_ok());

        assert_eq!(hdr[0], (1 << 16) | 5);
        assert_eq!(hdr[0] & (1 << 6), 0, "Write bit W must be 0 (Read-Only)");
        assert_eq!(hdr[1], 0, "PRDBC must be 0");
        assert_eq!(hdr[2], 0x2345_6000, "Lower 32 bits of CTBA");
        assert_eq!(hdr[3], 0x1, "Upper 32 bits of CTBA");
    }

    #[test]
    fn test_find_free_slot_ncs32_edge_cases() {
        assert_eq!(find_free_slot(0x0000_0000, 32), Some(0));
        assert_eq!(find_free_slot(0x0000_0001, 32), Some(1));
        assert_eq!(find_free_slot(0x7FFF_FFFF, 32), Some(31));
        assert_eq!(find_free_slot(0xFFFF_FFFF, 32), None);
    }

    #[test]
    fn test_find_free_slot_ncs1_edge_cases() {
        assert_eq!(find_free_slot(0x0000_0000, 1), Some(0));
        assert_eq!(find_free_slot(0x0000_0001, 1), None);
        assert_eq!(find_free_slot(0xFFFF_FFFE, 1), Some(0));
        assert_eq!(find_free_slot(0xFFFF_FFFF, 1), None);
    }

    #[test]
    fn test_find_free_slot_partial_ncs() {
        assert_eq!(find_free_slot(0b0000, 4), Some(0));
        assert_eq!(find_free_slot(0b0001, 4), Some(1));
        assert_eq!(find_free_slot(0b0011, 4), Some(2));
        assert_eq!(find_free_slot(0b0111, 4), Some(3));
        assert_eq!(find_free_slot(0b1111, 4), None);
        assert_eq!(find_free_slot(0xFFFF_FFF0 | 0b1111, 4), None);
        assert_eq!(find_free_slot(0, 0), None);
    }

    #[test]
    fn test_prdt_builder_dbc_len_minus_one() {
        let mut prdt = [0u32; 4];
        let buf_phys = 0x0000_0000_1000_2000u64;
        let res = build_prdt_entry(buf_phys, 512, false, &mut prdt);
        assert!(res.is_ok());
        assert_eq!(prdt[0], 0x1000_2000);
        assert_eq!(prdt[1], 0);
        assert_eq!(prdt[2], 0);
        assert_eq!(prdt[3] & 0x3F_FFFF, 511, "PRDT DBC must be byte_count - 1");
        assert_eq!(prdt[3] & (1 << 31), 0, "IOC bit 31 must be 0");
    }

    #[test]
    fn test_decode_identify_data_telemetry() {
        let mut words = [0u16; 256];

        let model_str = b"OptiOS_Test_SSD     ";
        for i in 0..10 {
            let hi = model_str[i * 2] as u16;
            let lo = model_str[i * 2 + 1] as u16;
            words[27 + i] = (hi << 8) | lo;
        }

        let serial_str = b"OPTI-2026-9901      ";
        for i in 0..10 {
            let hi = serial_str[i * 2] as u16;
            let lo = serial_str[i * 2 + 1] as u16;
            words[10 + i] = (hi << 8) | lo;
        }

        let fw_str = b"2.5+    ";
        for i in 0..4 {
            let hi = fw_str[i * 2] as u16;
            let lo = fw_str[i * 2 + 1] as u16;
            words[23 + i] = (hi << 8) | lo;
        }

        words[100] = 0x0000;
        words[101] = 0x0002;
        words[102] = 0x0000;
        words[103] = 0x0000;

        words[82] = 0x0001;
        words[85] = 0x0001;

        let disk = decode_identify_data(&words);
        assert_eq!(disk.model, "OptiOS_Test_SSD");
        assert_eq!(disk.serial, "OPTI-2026-9901");
        assert_eq!(disk.firmware, "2.5+");
        assert_eq!(disk.total_sectors, 131072);
        assert_eq!(disk.capacity_mib, 64);
        assert!(disk.smart_supported);
        assert!(disk.smart_enabled);
    }

    #[test]
    fn test_parse_smart_data_telemetry() {
        let mut data = [0u8; 512];
        data[0] = 0x10; // Revision 16
        data[1] = 0x00;

        // Attribute 1: ID 5 (Reallocated Sectors), raw = 0
        let off1 = 2;
        data[off1] = 5;
        data[off1 + 3] = 100; // current
        data[off1 + 4] = 100; // worst
        data[off1 + 5] = 0;   // raw = 0

        // Attribute 2: ID 9 (Power-On Hours), raw = 1234
        let off2 = 2 + 12;
        data[off2] = 9;
        data[off2 + 3] = 98;
        data[off2 + 4] = 98;
        data[off2 + 5] = (1234 & 0xFF) as u8;
        data[off2 + 6] = ((1234 >> 8) & 0xFF) as u8;

        // Attribute 3: ID 12 (Power Cycle Count), raw = 42
        let off3 = 2 + 24;
        data[off3] = 12;
        data[off3 + 3] = 99;
        data[off3 + 4] = 99;
        data[off3 + 5] = 42;

        // Attribute 4: ID 194 (Temperature Celsius), raw = 37 C
        let off4 = 2 + 36;
        data[off4] = 194;
        data[off4 + 3] = 63;
        data[off4 + 4] = 50;
        data[off4 + 5] = 37;

        let telemetry = parse_smart_data(&data);
        assert_eq!(telemetry.revision, 16);
        assert_eq!(telemetry.reallocated_sectors, Some(0));
        assert_eq!(telemetry.power_on_hours, Some(1234));
        assert_eq!(telemetry.power_cycles, Some(42));
        assert_eq!(telemetry.temperature_c, Some(37));
        assert_eq!(telemetry.attributes.len(), 4);
    }
}
