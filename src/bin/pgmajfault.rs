// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use std::{env, io};

fn main() -> Result<(), io::Error> {
    let args = env::args().skip(1);

    for arg in args {
        println!("mmapping {}...", &arg);
        let mmap = rustest::Mmap::new(&arg)?;
        println!("paging in {}...", &arg);
        mmap.populate()?;
    }

    Ok(())
}
