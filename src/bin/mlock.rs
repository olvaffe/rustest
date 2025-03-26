// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use crossterm::event;
use std::{
    env, fmt, fs,
    io::{self, BufRead},
};

const CHUNK_SIZE_MB: usize = 256;

enum MlockHeap {
    Locked,
    Unlocked,
}

struct Mlock {
    locked: Vec<rustest::Mmap>,
    unlocked: Vec<rustest::Mmap>,
}

impl Mlock {
    fn new() -> Mlock {
        Mlock {
            locked: Vec::new(),
            unlocked: Vec::new(),
        }
    }

    fn add(&mut self, heap: MlockHeap) -> Result<(), io::Error> {
        let mut mmap = rustest::Mmap::anonymous(CHUNK_SIZE_MB * 1024 * 1024)?;
        match heap {
            MlockHeap::Locked => {
                mmap.mlock()?;
                self.locked.push(mmap);
            }
            MlockHeap::Unlocked => {
                mmap.fill((self.unlocked.len() + 1) as u8);
                self.unlocked.push(mmap);
            }
        }

        Ok(())
    }

    fn remove(&mut self, heap: MlockHeap) -> bool {
        match heap {
            MlockHeap::Locked => self.locked.pop(),
            MlockHeap::Unlocked => self.unlocked.pop(),
        }
        .is_some()
    }

    fn page_in(&self) {
        for mmap in &self.unlocked {
            let _ = mmap.populate();
        }
    }
}

impl fmt::Display for Mlock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let [locked_mb, unlocked_mb] =
            [self.locked.len(), self.unlocked.len()].map(|len| len * CHUNK_SIZE_MB);
        write!(
            f,
            "locked {:5} MB, unlocked {:5} MB",
            locked_mb, unlocked_mb,
        )
    }
}

struct Proc {
    page_size: usize,

    // pages that are mlock'ed
    mlocked: u64,
    // swap usage
    swap_total: u64,
    swap_free: u64,
    // pages that are anonymous and resident
    anon_pages: u64,

    // accumulated pages swapped in/out to block devices
    pswpin: u64,
    pswpout: u64,

    pswpin_delta: u64,
    pswpout_delta: u64,
}

impl Proc {
    fn collect(prev: Option<Proc>) -> Self {
        let mut proc = Proc {
            page_size: rustest::page_size(),

            mlocked: 0,
            swap_total: 0,
            swap_free: 0,
            anon_pages: 0,

            pswpin: 0,
            pswpout: 0,

            pswpin_delta: 0,
            pswpout_delta: 0,
        };

        let _ = proc.collect_meminfo();
        let _ = proc.collect_vmstat();

        if let Some(prev) = prev {
            proc.pswpin_delta = proc.pswpin - prev.pswpin;
            proc.pswpout_delta = proc.pswpout - prev.pswpout;
        }

        proc
    }

    fn collect_meminfo(&mut self) -> Result<(), io::Error> {
        let fp = fs::File::open("/proc/meminfo")?;
        let reader = io::BufReader::new(fp);

        for line in reader.lines() {
            let line = line?;

            let extract_val = |line: &str| {
                line.split_ascii_whitespace()
                    .nth(1)
                    .and_then(|val| val.parse::<u64>().ok())
                    .unwrap_or_default()
            };

            if line.starts_with("Mlocked:") {
                self.mlocked = extract_val(&line);
            } else if line.starts_with("SwapTotal:") {
                self.swap_total = extract_val(&line);
            } else if line.starts_with("SwapFree:") {
                self.swap_free = extract_val(&line);
            } else if line.starts_with("AnonPages:") {
                self.anon_pages = extract_val(&line);
                break;
            }
        }

        Ok(())
    }
    fn collect_vmstat(&mut self) -> Result<(), io::Error> {
        let fp = fs::File::open("/proc/vmstat")?;
        let reader = io::BufReader::new(fp);

        for line in reader.lines() {
            let line = line?;

            if let Some(val) = line.strip_prefix("pswpin ") {
                self.pswpin = val.parse().unwrap_or_default();
            } else if let Some(val) = line.strip_prefix("pswpout ") {
                self.pswpout = val.parse().unwrap_or_default();
                break;
            }
        }

        Ok(())
    }
}

impl fmt::Display for Proc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let [mlocked, swap_total, swap_free, anon_pages] = [
            self.mlocked,
            self.swap_total,
            self.swap_free,
            self.anon_pages,
        ]
        .map(|kb| kb / 1024);

        let [swap_in, swap_out] = [self.pswpin_delta, self.pswpout_delta]
            .map(|page_count| (page_count as usize) * self.page_size / 1024 / 1024);

        write!(
            f,
            "locked {:5} MB, unlocked {:5} MB, swap {:5} MB, swap i/o +{}/+{} MB",
            mlocked,
            anon_pages.checked_sub(mlocked).unwrap_or(0),
            swap_total - swap_free,
            swap_in,
            swap_out,
        )
    }
}

struct ProcSelf {
    // pages that are mlock'ed
    vm_lck: u64,
    // pages that are anonymous and resident
    rss_anon: u64,
    // pages that are anonymous and swapped out
    vm_swap: u64,
}

impl ProcSelf {
    fn collect() -> Self {
        let mut pid = ProcSelf {
            vm_lck: 0,
            rss_anon: 0,
            vm_swap: 0,
        };

        let _ = pid.collect_status();

        pid
    }

    fn collect_status(&mut self) -> Result<(), io::Error> {
        let fp = fs::File::open("/proc/self/status")?;
        let reader = io::BufReader::new(fp);

        for line in reader.lines() {
            let line = line?;

            let extract_val = |line: &str| {
                line.split_ascii_whitespace()
                    .nth(1)
                    .and_then(|val| val.parse::<u64>().ok())
                    .unwrap_or_default()
            };

            if line.starts_with("VmLck:") {
                self.vm_lck = extract_val(&line);
            } else if line.starts_with("RssAnon:") {
                self.rss_anon = extract_val(&line);
            } else if line.starts_with("VmSwap:") {
                self.vm_swap = extract_val(&line);
                break;
            }
        }

        Ok(())
    }
}

impl fmt::Display for ProcSelf {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> Result<(), fmt::Error> {
        let [vm_lck, rss_anon, vm_swap] =
            [self.vm_lck, self.rss_anon, self.vm_swap].map(|kb| kb / 1024);
        write!(
            f,
            "locked {:5} MB, unlocked {:5} MB, swap {:5} MB",
            vm_lck,
            rss_anon.checked_sub(vm_lck).unwrap_or(0),
            vm_swap,
        )
    }
}

enum Action {
    Redraw,
    Quit,
    Add(MlockHeap),
    Remove(MlockHeap),
    PageIn,
}

fn term_wait_action(term: &mut rustest::Term) -> Action {
    let key = match term.poll(1000) {
        Ok(Some(key)) => key,
        Ok(None) => return Action::Redraw,
        Err(_) => return Action::Quit,
    };

    match key.modifiers {
        event::KeyModifiers::CONTROL => match key.code {
            event::KeyCode::Char('c') | event::KeyCode::Char('d') => Action::Quit,
            _ => Action::Redraw,
        },
        event::KeyModifiers::SHIFT | event::KeyModifiers::NONE => match key.code {
            event::KeyCode::Char('+') | event::KeyCode::Char('=') => Action::Add(MlockHeap::Locked),
            event::KeyCode::Char('-') | event::KeyCode::Char('_') => {
                Action::Remove(MlockHeap::Locked)
            }
            event::KeyCode::Char(']') | event::KeyCode::Char('}') => {
                Action::Add(MlockHeap::Unlocked)
            }
            event::KeyCode::Char('[') | event::KeyCode::Char('{') => {
                Action::Remove(MlockHeap::Unlocked)
            }
            event::KeyCode::Char('p') | event::KeyCode::Char('P') => Action::PageIn,
            event::KeyCode::Char('q') | event::KeyCode::Esc => Action::Quit,
            _ => Action::Redraw,
        },
        _ => Action::Redraw,
    }
}

fn print_help() {
    println!("usage:");
    println!("  +/-: add/remove locked mappings");
    println!("  ]/[: add/remove unlocked mappings");
    println!("  p: page in unlocked mappings");
    println!("  q: quit");
}

fn main() -> Result<(), io::Error> {
    let init_mb: usize = env::args()
        .nth(1)
        .map(|s| s.parse().unwrap_or_default())
        .unwrap_or_default();
    let init_count = init_mb / CHUNK_SIZE_MB;

    let mut mlock = Mlock::new();
    for _ in 0..init_count {
        let _ = mlock.add(MlockHeap::Locked);
    }

    print_help();
    println!();

    let mut term = rustest::Term::new()?;

    let mut sys_prev = None;
    loop {
        let sys = Proc::collect(sys_prev);
        let pid = ProcSelf::collect();

        term.cmd_fmt(format_args!("mlock:     {}\r\n", &mlock));
        term.cmd_fmt(format_args!("proc self: {}\r\n", &pid));
        term.cmd_fmt(format_args!("proc sys:  {}\r\n", &sys));
        term.cmd_flush();

        sys_prev = Some(sys);

        match term_wait_action(&mut term) {
            Action::Redraw => (),
            Action::Quit => break,
            Action::Add(heap) => {
                let _ = mlock.add(heap);
            }
            Action::Remove(heap) => {
                mlock.remove(heap);
            }
            Action::PageIn => {
                term.cmd_str(" ... paging in ...");
                term.cmd_flush();
                mlock.page_in();
            }
        }

        term.cmd_clear(3);
    }

    term.reset();
    println!();

    Ok(())
}
