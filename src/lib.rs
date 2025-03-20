// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use std::{
    ffi, fs,
    io::{self, Seek},
    os::fd::{AsFd, AsRawFd, RawFd},
    ptr, slice,
};

pub struct Mmap {
    addr: *mut ffi::c_void,
    len: usize,
}

impl Mmap {
    pub fn new(path: &str) -> Result<Self, io::Error> {
        let mut fp = fs::File::open(path)?;
        let len = fp.seek(io::SeekFrom::End(0))? as usize;
        let fd = fp.as_fd();

        Self::mmap_raw(len, libc::PROT_READ, libc::MAP_SHARED, fd.as_raw_fd())
    }

    pub fn anonymous(len: usize) -> Result<Self, io::Error> {
        Self::mmap_raw(
            len,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANONYMOUS,
            -1,
        )
    }

    fn mmap_raw(len: usize, prot: i32, flags: i32, fd: RawFd) -> Result<Self, io::Error> {
        let addr = ptr::null_mut();
        let offset = 0;

        // SAFETY: all args are valid
        let addr = unsafe { libc::mmap(addr, len, prot, flags, fd, offset) };
        if addr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        Ok(Mmap { addr, len })
    }

    pub fn mlock(&self) -> Result<(), io::Error> {
        // SAFETY: we control self
        let ret = unsafe { libc::mlock(self.addr, self.len) };
        if ret != 0 {
            return Err(io::Error::last_os_error());
        }

        Ok(())
    }

    pub fn munlock(&self) {
        // SAFETY: we control self
        unsafe { libc::munlock(self.addr, self.len) };
    }

    pub fn populate(&self) -> Result<(), io::Error> {
        self.mlock()?;
        self.munlock();

        Ok(())
    }

    pub fn fill(&mut self, val: u8) {
        let mut page_size = unsafe { libc::sysconf(libc::_SC_PAGE_SIZE) } as usize;
        if page_size <= 0 {
            page_size = 4096;
        }

        // SAFETY: we control self
        let bytes = unsafe { slice::from_raw_parts_mut(self.addr as _, self.len) };
        let page_count = (bytes.len() + page_size - 1) / page_size;
        for page in 0..page_count {
            bytes[page * page_size] = val;
        }
    }
}

impl Drop for Mmap {
    fn drop(&mut self) {
        // SAFETY: all args are valid
        let _ = unsafe { libc::munmap(self.addr, self.len) };
    }
}
