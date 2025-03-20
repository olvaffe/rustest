// Copyright 2025 Google LLC
// SPDX-License-Identifier: MIT

use crossterm::{cursor, event, execute, style, terminal};
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
        let mmap = rustest::Mmap::anonymous(CHUNK_SIZE_MB * 1024 * 1024)?;
        if let MlockHeap::Locked = heap {
            mmap.mlock()?;
        }

        match heap {
            MlockHeap::Locked => self.locked.push(mmap),
            MlockHeap::Unlocked => self.unlocked.push(mmap),
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
    Nop,
    Quit,
    Add(MlockHeap),
    Remove(MlockHeap),
    PageIn,
}

fn term_init() -> Result<(), io::Error> {
    execute!(io::stdout(), cursor::Hide)?;
    terminal::enable_raw_mode().inspect_err(|_| {
        let _ = execute!(io::stdout(), cursor::Show);
    })
}

fn term_restore() {
    let _ = terminal::disable_raw_mode();
    let _ = execute!(io::stdout(), cursor::Show);
}

fn term_wait_action() -> Action {
    let ev = match event::read() {
        Ok(event::Event::Key(ev)) => ev,
        Ok(_) => return Action::Nop,
        Err(_) => return Action::Quit,
    };

    match ev.modifiers {
        event::KeyModifiers::CONTROL => match ev.code {
            event::KeyCode::Char('c') | event::KeyCode::Char('d') => Action::Quit,
            _ => Action::Nop,
        },
        event::KeyModifiers::SHIFT | event::KeyModifiers::NONE => match ev.code {
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
            _ => Action::Nop,
        },
        _ => Action::Nop,
    }
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

    term_init()?;

    loop {
        let _ = execute!(
            io::stdout(),
            terminal::Clear(terminal::ClearType::CurrentLine),
            style::Print(format!("\r{}", &mlock))
        );

        match term_wait_action() {
            Action::Nop => (),
            Action::Quit => break,
            Action::Add(heap) => {
                let _ = mlock.add(heap);
            }
            Action::Remove(heap) => {
                mlock.remove(heap);
            }
            Action::PageIn => {
                let _ = execute!(io::stdout(), style::Print(" ... paging in ..."));
                mlock.page_in();
            }
        }
    }

    term_restore();

    println!();

    Ok(())
}
