// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: USTAR In-Memory Archive Parser Library Tests
// Date: 2026-11-09

#[cfg(test)]
mod tests {
    use crate::*;
    use alloc::vec;
    use alloc::vec::Vec;

    fn make_valid_tar(name: &str, content: &[u8]) -> Vec<u8> {
        let mut tar = vec![0u8; 2048]; // header (512) + 1 block data (512) + 2 null blocks (1024) = 2048
        let name_bytes = name.as_bytes();
        tar[..name_bytes.len()].copy_from_slice(name_bytes);
        tar[100..108].copy_from_slice(b"0000644\0");
        tar[108..116].copy_from_slice(b"0001750\0");
        tar[116..124].copy_from_slice(b"0001750\0");
        let size_str = alloc::format!("{:011o}\0", content.len());
        tar[124..136].copy_from_slice(size_str.as_bytes());
        tar[136..148].copy_from_slice(b"00000000000\0");
        tar[156] = b'0';
        tar[257..263].copy_from_slice(b"ustar\0");
        tar[263..265].copy_from_slice(b"00");

        // checksum (148..156)
        for i in 148..156 {
            tar[i] = 0x20;
        }
        let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk_str = alloc::format!("{:06o}\0 ", sum);
        tar[148..156].copy_from_slice(chk_str.as_bytes());

        // data (512..512+content.len())
        tar[512..512 + content.len()].copy_from_slice(content);

        tar
    }

    #[test]
    fn test_valid_archive() {
        let tar = make_valid_tar("etc/test.txt", b"hello world");
        let res = parse_ustar_archive(&tar);
        assert!(res.is_ok(), "Expected Ok, got {:?}", res);
        let entries = res.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, "/etc/test.txt");
        assert_eq!(entries[0].data, b"hello world");
    }

    #[test]
    fn test_bad_checksum() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar[148..156].copy_from_slice(b"001234\0 ");
        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::BadChecksum), "Expected BadChecksum, got {:?}", res);
    }

    #[test]
    fn test_bad_version() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar[263..265].copy_from_slice(b"99");
        for i in 148..156 { tar[i] = 0x20; }
        let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk_str = alloc::format!("{:06o}\0 ", sum);
        tar[148..156].copy_from_slice(chk_str.as_bytes());

        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::BadVersion), "Expected BadVersion, got {:?}", res);
    }

    #[test]
    fn test_bad_magic() {
        // Case 1: GNU-style "ustar  \0" (with space at index 262 instead of NUL)
        let mut tar1 = make_valid_tar("etc/test.txt", b"hello world");
        tar1[257..263].copy_from_slice(b"ustar ");
        for i in 148..156 { tar1[i] = 0x20; }
        let sum1: usize = tar1[..512].iter().map(|&b| b as usize).sum();
        let chk_str1 = alloc::format!("{:06o}\0 ", sum1);
        tar1[148..156].copy_from_slice(chk_str1.as_bytes());

        let res1 = parse_ustar_archive(&tar1);
        assert_eq!(res1, Err(TarError::BadMagic), "Expected BadMagic for 'ustar ', got {:?}", res1);

        // Case 2: All-zero magic
        let mut tar2 = make_valid_tar("etc/test.txt", b"hello world");
        tar2[257..263].copy_from_slice(&[0u8; 6]);
        for i in 148..156 { tar2[i] = 0x20; }
        let sum2: usize = tar2[..512].iter().map(|&b| b as usize).sum();
        let chk_str2 = alloc::format!("{:06o}\0 ", sum2);
        tar2[148..156].copy_from_slice(chk_str2.as_bytes());

        let res2 = parse_ustar_archive(&tar2);
        assert_eq!(res2, Err(TarError::BadMagic), "Expected BadMagic for all-zero magic, got {:?}", res2);
    }

    #[test]
    fn test_non_octal_size() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar[124..136].copy_from_slice(b"00000000088\0");
        for i in 148..156 { tar[i] = 0x20; }
        let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk_str = alloc::format!("{:06o}\0 ", sum);
        tar[148..156].copy_from_slice(chk_str.as_bytes());

        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::NonOctalField), "Expected NonOctalField, got {:?}", res);
    }

    #[test]
    fn test_size_larger_than_module() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar[124..136].copy_from_slice(b"00000303240\0");
        for i in 148..156 { tar[i] = 0x20; }
        let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk_str = alloc::format!("{:06o}\0 ", sum);
        tar[148..156].copy_from_slice(chk_str.as_bytes());

        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::SizeOutOfBounds), "Expected SizeOutOfBounds, got {:?}", res);
    }

    #[test]
    fn test_truncated_data() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar.truncate(600);
        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::TruncatedArchive), "Expected TruncatedArchive, got {:?}", res);
    }

    #[test]
    fn test_missing_end_blocks() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello world");
        tar.truncate(1024);
        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::UnexpectedEof), "Expected UnexpectedEof, got {:?}", res);
    }

    #[test]
    fn test_path_dot_dot_prefix() {
        let res = normalize_path("", "../x");
        assert_eq!(res, Err(TarError::InvalidPath), "Expected InvalidPath for prefix '..', got {:?}", res);
    }

    #[test]
    fn test_path_dot_dot_internal() {
        let res = normalize_path("", "/a/../b");
        assert_eq!(res, Err(TarError::InvalidPath), "Expected InvalidPath for internal '..', got {:?}", res);
    }

    #[test]
    fn test_path_dot_dot_multi() {
        let res = normalize_path("", "a/../../b");
        assert_eq!(res, Err(TarError::InvalidPath), "Expected InvalidPath for multiple '..', got {:?}", res);
    }

    #[test]
    fn test_path_normalise_ok() {
        let res = normalize_path("", "./a//b");
        assert_eq!(res, Ok(String::from("/a/b")), "Expected /a/b, got {:?}", res);
    }

    #[test]
    fn test_octal_checked_mul_overflow() {
        // 23 octal digits: cannot fit in 64-bit usize (max 22 digits)
        let overflow_bytes = b"77777777777777777777777\0";
        let res = parse_octal(overflow_bytes);
        assert_eq!(res, Err(TarError::NumericOverflow), "Expected NumericOverflow, got {:?}", res);
    }

    #[test]
    fn test_base256_binary_size_field() {
        // GNU tar base-256 encoding sets high bit 0x80 on the first byte
        let base256_bytes = b"\x80\x00\x00\x00\x00\x00\x00\x00\x00\x00\x01\x00";
        let res = parse_octal(base256_bytes);
        assert_eq!(res, Err(TarError::HighBitSet), "Expected HighBitSet for base-256, got {:?}", res);
    }

    #[test]
    fn test_junk_after_octal_terminator() {
        // Terminator NUL followed by non-zero/non-space garbage
        let junk_bytes = b"0000644\0JUNK";
        let res = parse_octal(junk_bytes);
        assert_eq!(res, Err(TarError::JunkAfterTerminator), "Expected JunkAfterTerminator, got {:?}", res);
    }

    #[test]
    fn test_path_length_exceeds_255() {
        let long_name = "a".repeat(256);
        let res = normalize_path("", &long_name);
        assert_eq!(res, Err(TarError::PathTooLong), "Expected PathTooLong, got {:?}", res);
    }

    #[test]
    fn test_composed_prefix_and_name_exceeds_255() {
        let prefix = "p".repeat(150);
        let name = "n".repeat(120);
        let res = normalize_path(&prefix, &name);
        assert_eq!(res, Err(TarError::PathTooLong), "Expected PathTooLong for composed prefix+name > 255, got {:?}", res);
    }

    #[test]
    fn test_normalize_rejects_dot_dot() {
        // Exercises path normalization rejecting any `..` traversal component
        assert_eq!(normalize_path("", "etc/../secret"), Err(TarError::InvalidPath), "path with .. must be rejected");
        assert_eq!(normalize_path("", "../etc/passwd"), Err(TarError::InvalidPath), "path with leading .. must be rejected");
        assert_eq!(normalize_path("", "/var/log/../log"), Err(TarError::InvalidPath), "path with internal .. must be rejected");
    }

    #[test]
    fn test_embedded_nul_in_path() {
        let res = normalize_path("", "etc/optios\0secret");
        assert_eq!(res, Err(TarError::InvalidPath), "Expected InvalidPath for embedded NUL, got {:?}", res);
    }

    #[test]
    fn test_invalid_utf8_in_header() {
        let mut tar = make_valid_tar("etc/test.txt", b"hello");
        // Inject invalid UTF-8 byte into name
        tar[0..4].copy_from_slice(&[0xFF, 0xFE, 0xFD, 0xFC]);
        for i in 148..156 { tar[i] = 0x20; }
        let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk_str = alloc::format!("{:06o}\0 ", sum);
        tar[148..156].copy_from_slice(chk_str.as_bytes());

        let res = parse_ustar_archive(&tar);
        assert_eq!(res, Err(TarError::InvalidUtf8), "Expected InvalidUtf8, got {:?}", res);
    }

    #[test]
    fn test_symlink_and_hardlink_ignored_not_followed() {
        for &typeflag in &[b'1', b'2'] {
            let mut tar = make_valid_tar("etc/link_target", b"data");
            // Set typeflag: '1' = hard link, '2' = symbolic link
            tar[156] = typeflag;
            tar[157..169].copy_from_slice(b"/etc/passwd\0");
            for i in 148..156 { tar[i] = 0x20; }
            let sum: usize = tar[..512].iter().map(|&b| b as usize).sum();
            let chk_str = alloc::format!("{:06o}\0 ", sum);
            tar[148..156].copy_from_slice(chk_str.as_bytes());

            let res = parse_ustar_archive(&tar);
            assert!(res.is_ok(), "Expected Ok for typeflag {}, got {:?}", typeflag as char, res);
            let entries = res.unwrap();
            assert_eq!(entries.len(), 0, "Typeflag {} must be ignored and not present in nodes", typeflag as char);
        }
    }

    #[test]
    fn test_pax_and_gnu_extensions_skipped_correctly() {
        // Policy: OptiOS ignores PAX ('x', 'g') and GNU ('L', 'K') metadata headers without error.
        // Their data payload blocks MUST be skipped based on the size field so that the subsequent
        // file header is parsed at the correct block-aligned offset without misalignment.
        for &typeflag in &[b'x', b'g', b'L', b'K'] {
            let mut tar = vec![0u8; 3072]; // 512 hdr1 + 512 data1 + 512 hdr2 + 512 data2 + 1024 zero = 3072

            // 1. Extension Header (offset 0..512)
            tar[..10].copy_from_slice(b"pax_hdr\0\0\0");
            tar[100..108].copy_from_slice(b"0000644\0");
            tar[108..116].copy_from_slice(b"0001750\0");
            tar[116..124].copy_from_slice(b"0001750\0");
            tar[124..136].copy_from_slice(b"00000000024\0"); // 20 bytes payload (1 block)
            tar[136..148].copy_from_slice(b"00000000000\0");
            tar[156] = typeflag;
            tar[257..263].copy_from_slice(b"ustar\0");
            tar[263..265].copy_from_slice(b"00");
            for i in 148..156 { tar[i] = 0x20; }
            let sum1: usize = tar[..512].iter().map(|&b| b as usize).sum();
            let chk_str1 = alloc::format!("{:06o}\0 ", sum1);
            tar[148..156].copy_from_slice(chk_str1.as_bytes());

            // 1. Data payload (offset 512..1024)
            tar[512..532].copy_from_slice(b"20 comment=optios\n\0\0");

            // 2. Regular File Header (offset 1024..1536)
            let hdr2 = 1024;
            tar[hdr2..hdr2 + 8].copy_from_slice(b"etc/motd");
            tar[hdr2 + 100..hdr2 + 108].copy_from_slice(b"0000644\0");
            tar[hdr2 + 108..hdr2 + 116].copy_from_slice(b"0001750\0");
            tar[hdr2 + 116..hdr2 + 124].copy_from_slice(b"0001750\0");
            tar[hdr2 + 124..hdr2 + 136].copy_from_slice(b"00000000013\0"); // 11 bytes
            tar[hdr2 + 136..hdr2 + 148].copy_from_slice(b"00000000000\0");
            tar[hdr2 + 156] = b'0';
            tar[hdr2 + 257..hdr2 + 263].copy_from_slice(b"ustar\0");
            tar[hdr2 + 263..hdr2 + 265].copy_from_slice(b"00");
            for i in hdr2 + 148..hdr2 + 156 { tar[i] = 0x20; }
            let sum2: usize = tar[hdr2..hdr2 + 512].iter().map(|&b| b as usize).sum();
            let chk_str2 = alloc::format!("{:06o}\0 ", sum2);
            tar[hdr2 + 148..hdr2 + 156].copy_from_slice(chk_str2.as_bytes());

            // 2. Data payload (offset 1536..2048)
            tar[1536..1547].copy_from_slice(b"hello world");

            // 3. End of archive: offsets 2048..3072 are all null bytes

            let res = parse_ustar_archive(&tar);
            assert!(res.is_ok(), "Expected Ok for extension typeflag {}, got {:?}", typeflag as char, res);
            let entries = res.unwrap();
            assert_eq!(entries.len(), 1, "Only regular file should be present");
            assert_eq!(entries[0].path, "/etc/motd");
            assert_eq!(entries[0].data, b"hello world");
        }
    }

    #[test]
    fn test_normalize_path_root_and_empty() {
        let res_root = normalize_path("", "/");
        let res_dot = normalize_path("", ".");
        let res_dot_slash = normalize_path("", "./");
        let res_empty = normalize_path("", "");
        assert_eq!(res_root, Ok(String::from("/")), "Expected Ok('/') for root '/'");
        assert_eq!(res_dot, Ok(String::from("/")), "Expected Ok('/') for '.'");
        assert_eq!(res_dot_slash, Ok(String::from("/")), "Expected Ok('/') for './'");
        assert_eq!(res_empty, Err(TarError::InvalidPath), "Expected InvalidPath for empty path ''");
    }

    #[test]
    fn test_root_entries_skipped() {
        // Archives generated by `tar -C rootfs .` typically start with entries like "./", ".", or "/"
        // These root directory entries must be cleanly skipped by parse_ustar_archive without error.
        let mut tar = vec![0u8; 3584]; // 3 headers (1536) + 2 blocks data (1024) + 2 null blocks (1024) = 3584

        // 1. Entry 1: "./" directory entry (offset 0..512)
        tar[..2].copy_from_slice(b"./");
        tar[100..108].copy_from_slice(b"0000755\0");
        tar[108..116].copy_from_slice(b"0001750\0");
        tar[116..124].copy_from_slice(b"0001750\0");
        tar[124..136].copy_from_slice(b"00000000000\0");
        tar[136..148].copy_from_slice(b"00000000000\0");
        tar[156] = b'5'; // Directory
        tar[257..263].copy_from_slice(b"ustar\0");
        tar[263..265].copy_from_slice(b"00");
        for i in 148..156 { tar[i] = 0x20; }
        let sum1: usize = tar[..512].iter().map(|&b| b as usize).sum();
        let chk1 = alloc::format!("{:06o}\0 ", sum1);
        tar[148..156].copy_from_slice(chk1.as_bytes());

        // 2. Entry 2: "." directory entry (offset 512..1024)
        let hdr2 = 512;
        tar[hdr2..hdr2 + 1].copy_from_slice(b".");
        tar[hdr2 + 100..hdr2 + 108].copy_from_slice(b"0000755\0");
        tar[hdr2 + 108..hdr2 + 116].copy_from_slice(b"0001750\0");
        tar[hdr2 + 116..hdr2 + 124].copy_from_slice(b"0001750\0");
        tar[hdr2 + 124..hdr2 + 136].copy_from_slice(b"00000000000\0");
        tar[hdr2 + 136..hdr2 + 148].copy_from_slice(b"00000000000\0");
        tar[hdr2 + 156] = b'5';
        tar[hdr2 + 257..hdr2 + 263].copy_from_slice(b"ustar\0");
        tar[hdr2 + 263..hdr2 + 265].copy_from_slice(b"00");
        for i in hdr2 + 148..hdr2 + 156 { tar[i] = 0x20; }
        let sum2: usize = tar[hdr2..hdr2 + 512].iter().map(|&b| b as usize).sum();
        let chk2 = alloc::format!("{:06o}\0 ", sum2);
        tar[hdr2 + 148..hdr2 + 156].copy_from_slice(chk2.as_bytes());

        // 3. Entry 3: Regular file "etc/motd" (offset 1024..1536)
        let hdr3 = 1024;
        tar[hdr3..hdr3 + 8].copy_from_slice(b"etc/motd");
        tar[hdr3 + 100..hdr3 + 108].copy_from_slice(b"0000644\0");
        tar[hdr3 + 108..hdr3 + 116].copy_from_slice(b"0001750\0");
        tar[hdr3 + 116..hdr3 + 124].copy_from_slice(b"0001750\0");
        tar[hdr3 + 124..hdr3 + 136].copy_from_slice(b"00000000013\0"); // 11 bytes
        tar[hdr3 + 136..hdr3 + 148].copy_from_slice(b"00000000000\0");
        tar[hdr3 + 156] = b'0';
        tar[hdr3 + 257..hdr3 + 263].copy_from_slice(b"ustar\0");
        tar[hdr3 + 263..hdr3 + 265].copy_from_slice(b"00");
        for i in hdr3 + 148..hdr3 + 156 { tar[i] = 0x20; }
        let sum3: usize = tar[hdr3..hdr3 + 512].iter().map(|&b| b as usize).sum();
        let chk3 = alloc::format!("{:06o}\0 ", sum3);
        tar[hdr3 + 148..hdr3 + 156].copy_from_slice(chk3.as_bytes());

        // 3. Data payload (offset 1536..2048)
        tar[1536..1547].copy_from_slice(b"hello world");

        // 4. Null blocks at 2048..3072

        let res = parse_ustar_archive(&tar);
        assert!(res.is_ok(), "Expected Ok when parsing archive with root entries, got {:?}", res);
        let entries = res.unwrap();
        assert_eq!(entries.len(), 1, "Root entries './' and '.' must be skipped, leaving only regular file");
        assert_eq!(entries[0].path, "/etc/motd");
        assert_eq!(entries[0].data, b"hello world");
    }
}
