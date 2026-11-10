// OptiOS
// Copyright (C) 2026 João Figueiró. All rights reserved
// Item: POSIX Standard Error Codes (Errno) & KernelResult Type
// Date: 2026-11-09

//! POSIX standardized error codes (`Errno`) and kernel result alias.
//!
//! Numeric values use traditional Unix/Linux numbering for ease of future libc
//! porting and POSIX toolchain integration. Note that this internal kernel representation
//! does NOT commit OptiOS to the Linux "-errno in RAX" syscall return ABI convention;
//! the exact user-space syscall convention will be decided in the syscall milestone.

use core::fmt;

/// Standardized POSIX error numbers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[allow(dead_code)]
pub enum Errno {
    /// Operation not permitted.
    EPERM = 1,
    /// No such file or directory.
    ENOENT = 2,
    /// Interrupted system call.
    EINTR = 4,
    /// Input/output error.
    EIO = 5,
    /// Bad file descriptor.
    EBADF = 9,
    /// Resource temporarily unavailable (or operation would block).
    EAGAIN = 11,
    /// Out of memory.
    ENOMEM = 12,
    /// Permission denied.
    EACCES = 13,
    /// Bad memory address.
    EFAULT = 14,
    /// Device or resource busy.
    EBUSY = 16,
    /// File or directory already exists.
    EEXIST = 17,
    /// No such device.
    ENODEV = 19,
    /// Not a directory.
    ENOTDIR = 20,
    /// Is a directory.
    EISDIR = 21,
    /// Invalid argument.
    EINVAL = 22,
    /// No space left on device.
    ENOSPC = 28,
    /// Illegal seek.
    ESPIPE = 29,
    /// Read-only file system.
    EROFS = 30,
    /// File name too long.
    ENAMETOOLONG = 36,
    /// Function not implemented.
    ENOSYS = 38,
    /// Value too large for defined data type.
    EOVERFLOW = 75,
}

impl Errno {
    /// POSIX alias: `EWOULDBLOCK` has the exact same numeric value as `EAGAIN`.
    pub const EWOULDBLOCK: Errno = Errno::EAGAIN;

    /// Returns the canonical POSIX symbolic name and description.
    pub const fn description(&self) -> &'static str {
        match self {
            Errno::EPERM => "EPERM (Operation not permitted)",
            Errno::ENOENT => "ENOENT (No such file or directory)",
            Errno::EINTR => "EINTR (Interrupted system call)",
            Errno::EIO => "EIO (Input/output error)",
            Errno::EBADF => "EBADF (Bad file descriptor)",
            Errno::EAGAIN => "EAGAIN / EWOULDBLOCK (Resource temporarily unavailable)",
            Errno::ENOMEM => "ENOMEM (Out of memory)",
            Errno::EACCES => "EACCES (Permission denied)",
            Errno::EFAULT => "EFAULT (Bad address)",
            Errno::EBUSY => "EBUSY (Device or resource busy)",
            Errno::EEXIST => "EEXIST (File exists)",
            Errno::ENODEV => "ENODEV (No such device)",
            Errno::ENOTDIR => "ENOTDIR (Not a directory)",
            Errno::EISDIR => "EISDIR (Is a directory)",
            Errno::EINVAL => "EINVAL (Invalid argument)",
            Errno::ENOSPC => "ENOSPC (No space left on device)",
            Errno::ESPIPE => "ESPIPE (Illegal seek)",
            Errno::EROFS => "EROFS (Read-only file system)",
            Errno::ENAMETOOLONG => "ENAMETOOLONG (File name too long)",
            Errno::ENOSYS => "ENOSYS (Function not implemented)",
            Errno::EOVERFLOW => "EOVERFLOW (Value too large for defined data type)",
        }
    }
}

impl fmt::Display for Errno {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.description())
    }
}

/// Canonical kernel result type returning a value `T` or a POSIX `Errno`.
pub type KernelResult<T> = Result<T, Errno>;

// Compile-time verification that all Errno numeric values are strictly positive and unique.
const _: () = {
    let values = [
        Errno::EPERM as i32,
        Errno::ENOENT as i32,
        Errno::EINTR as i32,
        Errno::EIO as i32,
        Errno::EBADF as i32,
        Errno::EAGAIN as i32,
        Errno::ENOMEM as i32,
        Errno::EACCES as i32,
        Errno::EFAULT as i32,
        Errno::EBUSY as i32,
        Errno::EEXIST as i32,
        Errno::ENODEV as i32,
        Errno::ENOTDIR as i32,
        Errno::EISDIR as i32,
        Errno::EINVAL as i32,
        Errno::ENOSPC as i32,
        Errno::ESPIPE as i32,
        Errno::EROFS as i32,
        Errno::ENAMETOOLONG as i32,
        Errno::ENOSYS as i32,
        Errno::EOVERFLOW as i32,
    ];

    let mut i = 0;
    while i < values.len() {
        assert!(values[i] > 0, "Errno values must be strictly positive");
        let mut j = i + 1;
        while j < values.len() {
            assert!(values[i] != values[j], "Errno values must be strictly unique");
            j += 1;
        }
        i += 1;
    }

    assert!(Errno::EWOULDBLOCK as i32 == Errno::EAGAIN as i32);
};
