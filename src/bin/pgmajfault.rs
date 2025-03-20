// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use std::{
    cmp, env, ffi, fs,
    io::{self, Seek},
    mem,
    os::fd::{AsFd, AsRawFd},
    ptr, slice,
};

struct Mmap {
    addr: *mut ffi::c_void,
    len: usize,
}

impl Mmap {
    fn new(path: &str) -> Result<Self, io::Error> {
        let mut fp = fs::File::open(path)?;
        let len = fp.seek(io::SeekFrom::End(0))? as usize;
        let fd = fp.as_fd();

        // SAFETY: all args are valid
        let addr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                fd.as_raw_fd(),
                0,
            )
        };
        if addr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(Mmap { addr, len })
    }

    fn as_bytes(&self) -> &[u8] {
        // SAFETY: we control self
        unsafe { slice::from_raw_parts(self.addr as _, self.len) }
    }

    fn populate(&self) -> Result<(), io::Error> {
        const BUF_SIZE: usize = 4096;
        let mut buf = Box::new(mem::MaybeUninit::<[u8; BUF_SIZE]>::uninit());
        let src = self.as_bytes();

        let mut offset = 0;
        while offset < self.len {
            let copy = cmp::min(self.len - offset, BUF_SIZE);

            let buf_ptr = buf.as_mut_ptr();
            // SAFETY: I guess so?
            let mut buf_arr = unsafe { *buf_ptr };
            // SAFETY: all args are valid
            let _ = unsafe {
                libc::memcpy(buf_arr.as_mut_ptr() as _, src[offset..].as_ptr() as _, copy)
            };

            offset += copy;
        }

        Ok(())
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        // SAFETY: all args are valid
        let _ = unsafe { libc::munmap(self.addr, self.len) };
    }
}

fn main() -> Result<(), io::Error> {
    let args = env::args().skip(1);

    for arg in args {
        println!("mmapping {}...", &arg);
        let mmap = Mmap::new(&arg)?;
        println!("paging in {}...", &arg);
        mmap.populate()?;
    }

    Ok(())
}
