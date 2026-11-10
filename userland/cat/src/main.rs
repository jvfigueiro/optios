// SPDX-License-Identifier: GPL-2.0-or-later
// Copyright (C) 2026 João Figueiró
// Item: Standalone Userland POSIX Cat Executable (Ring 3)
// Date: 2026-11-10

//! Standalone Ring 3 userland POSIX `cat` executable for OptiOS.
//! Receives arguments via System V AMD64 ABI (argc/argv), opens files using `sys_open`,
//! reads file or pseudo-filesystem contents via `sys_read`, and outputs to STDOUT via `sys_write`.

#![no_std]
#![no_main]

use core::panic::PanicInfo;
use optios_libc::{close, exit, extract_args, open, print, println, read, write};

#[no_mangle]
pub extern "C" fn _start(argc: u64, argv: *const *const u8) -> ! {
    let args = unsafe { extract_args(argc, argv) };

    if args.len() <= 1 {
        // Read from STDIN (fd 0)
        let mut buf = [0u8; 512];
        loop {
            match read(0, &mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let _ = write(1, &buf[..n]);
                }
                Err(_) => break,
            }
        }
        exit(0);
    }

    let mut exit_code = 0;
    for file_path in &args[1..] {
        match open(file_path, 0, 0) {
            Ok(fd) => {
                let mut buf = [0u8; 1024];
                loop {
                    match read(fd, &mut buf) {
                        Ok(0) => break,
                        Ok(n) => {
                            let _ = write(1, &buf[..n]);
                        }
                        Err(_) => {
                            println!("cat: {}: read error", file_path);
                            exit_code = 1;
                            break;
                        }
                    }
                }
                let _ = close(fd);
            }
            Err(_) => {
                println!("cat: {}: No such file or directory", file_path);
                exit_code = 1;
            }
        }
    }

    exit(exit_code);
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    print!("cat: fatal panic encountered!\n");
    exit(127);
}
