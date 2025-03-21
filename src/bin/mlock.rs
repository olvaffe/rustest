// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use crossterm::event;
use std::{env, fmt, io};

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
            "locked {} MB, unlocked {} MB, total {} MB",
            locked_mb,
            unlocked_mb,
            locked_mb + unlocked_mb,
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
    let key = match term.poll(-1) {
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
    println!();
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

    let mut term = rustest::Term::new()?;

    loop {
        term.cmd_clear();
        term.cmd_fmt(format_args!("{}", &mlock));
        term.cmd_flush();

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
                term.cmd_fmt(format_args!(" ... paging in ..."));
                term.cmd_flush();
                mlock.page_in();
            }
        }
    }

    term.reset();
    println!();

    Ok(())
}
