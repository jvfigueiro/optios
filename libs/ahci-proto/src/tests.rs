// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: AHCI Protocol Unit Tests
// Date: 2026-11-09

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
    fn test_command_header_builder_readonly() {
        let mut hdr = [0u32; 4];
        let ctba = 0x1_2345_6000u64;
        let res = build_command_header(5, 1, ctba, &mut hdr);
        assert!(res.is_ok());

        // dw0: (prdtl << 16) | (w << 6) | cfl
        // prdtl=1 => 1 << 16 = 0x10000; w is strictly 0; cfl=5 => 5. Total = 0x10005
        assert_eq!(hdr[0], (1 << 16) | 5);
        assert_eq!(hdr[0] & (1 << 6), 0, "Write bit W must be 0 (Read-Only)");
        assert_eq!(hdr[1], 0, "PRDBC must be 0");
        assert_eq!(hdr[2], 0x2345_6000, "Lower 32 bits of CTBA");
        assert_eq!(hdr[3], 0x1, "Upper 32 bits of CTBA");
    }

    #[test]
    fn test_find_free_slot_ncs32_edge_cases() {
        // NCS = 32: all 32 slots available when occupied = 0
        assert_eq!(find_free_slot(0x0000_0000, 32), Some(0));

        // Slot 0 busy, slot 1 free
        assert_eq!(find_free_slot(0x0000_0001, 32), Some(1));

        // Slots 0..30 busy, slot 31 free
        assert_eq!(find_free_slot(0x7FFF_FFFF, 32), Some(31));

        // All 32 slots occupied -> None
        assert_eq!(find_free_slot(0xFFFF_FFFF, 32), None);
    }

    #[test]
    fn test_find_free_slot_ncs1_edge_cases() {
        // NCS = 1: only slot 0 exists
        assert_eq!(find_free_slot(0x0000_0000, 1), Some(0));

        // Slot 0 occupied -> None (even if higher bits are 0)
        assert_eq!(find_free_slot(0x0000_0001, 1), None);

        // Higher bits set but slot 0 free: should return Some(0)
        assert_eq!(find_free_slot(0xFFFF_FFFE, 1), Some(0));

        // All-ones -> None
        assert_eq!(find_free_slot(0xFFFF_FFFF, 1), None);
    }

    #[test]
    fn test_find_free_slot_partial_ncs() {
        // NCS = 4: slots 0..3 valid
        assert_eq!(find_free_slot(0b0000, 4), Some(0));
        assert_eq!(find_free_slot(0b0001, 4), Some(1));
        assert_eq!(find_free_slot(0b0011, 4), Some(2));
        assert_eq!(find_free_slot(0b0111, 4), Some(3));
        assert_eq!(find_free_slot(0b1111, 4), None);

        // Higher bits set (e.g. bit 5 is 0) but within NCS=4 all are 1 -> None
        assert_eq!(find_free_slot(0xFFFF_FFF0 | 0b1111, 4), None);

        // NCS = 0 -> None
        assert_eq!(find_free_slot(0, 0), None);
    }

    #[test]
    fn test_prdt_builder_dbc_len_minus_one() {
        let mut prdt = [0u32; 4];
        let buf_phys = 0x0000_0000_1000_2000u64;
        // 512 bytes data buffer
        let res = build_prdt_entry(buf_phys, 512, false, &mut prdt);
        assert!(res.is_ok());
        assert_eq!(prdt[0], 0x1000_2000);
        assert_eq!(prdt[1], 0);
        assert_eq!(prdt[2], 0);
        // dw3: DBC = len - 1 = 512 - 1 = 511
        assert_eq!(prdt[3] & 0x3F_FFFF, 511, "PRDT DBC must be byte_count - 1");
        assert_eq!(prdt[3] & (1 << 31), 0, "IOC bit 31 must be 0");
    }

    #[test]
    fn test_decode_identify_data_telemetry() {
        let mut words = [0u16; 256];

        // Model: "OptiOS_Test_SSD" (words 27..47)
        // In ATA IDENTIFY, pairs of ASCII characters are stored big-endian in 16-bit words:
        // character 0 is in (w >> 8) (hi), character 1 is in (w & 0xFF) (lo).
        let model_str = b"OptiOS_Test_SSD     ";
        for i in 0..10 {
            let hi = model_str[i * 2] as u16;
            let lo = model_str[i * 2 + 1] as u16;
            words[27 + i] = (hi << 8) | lo;
        }

        // Serial: "OPTI-2026-9901      " (words 10..20)
        let serial_str = b"OPTI-2026-9901      ";
        for i in 0..10 {
            let hi = serial_str[i * 2] as u16;
            let lo = serial_str[i * 2 + 1] as u16;
            words[10 + i] = (hi << 8) | lo;
        }

        // Firmware: "2.5+    " (words 23..27)
        let fw_str = b"2.5+    ";
        for i in 0..4 {
            let hi = fw_str[i * 2] as u16;
            let lo = fw_str[i * 2 + 1] as u16;
            words[23 + i] = (hi << 8) | lo;
        }

        // Sector count: 131072 sectors = 64 MiB (LBA48 in words 100..103)
        // 131072 = 0x0002_0000 -> word 100 = 0x0000, word 101 = 0x0002
        words[100] = 0x0000;
        words[101] = 0x0002;
        words[102] = 0x0000;
        words[103] = 0x0000;

        // SMART supported: word 82 bit 0 = 1
        words[82] = 0x0001;
        // SMART enabled: word 85 bit 0 = 1
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
}
