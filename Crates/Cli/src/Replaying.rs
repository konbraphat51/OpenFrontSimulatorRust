// SPDX-License-Identifier: AGPL-3.0-only
//! Shared replay loop of `verify` and `trace`: runs a record tick by tick and catches panics.

use anyhow::{Context, Result};
use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Runner::GameRunner::{GameRunner, create_game_runner};
use ofsim_record::Records::{GameRecord, Turn};
use ofsim_record::StartInfo::GameStartInfo;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::Mutex;

static LAST_PANIC: Mutex<Option<String>> = Mutex::new(None);

/// Installs a panic hook that keeps the message and location instead of printing them.
pub fn silence_panics() {
    std::panic::set_hook(Box::new(|info| {
        let message = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "panic".to_string()
        };
        let location = info
            .location()
            .map(|l| format!(" ({}:{})", l.file(), l.line()))
            .unwrap_or_default();
        *LAST_PANIC.lock().unwrap() = Some(format!("{message}{location}"));
    }));
}

/// Why a replay stopped.
#[derive(Debug)]
pub enum Stop {
    /// Every turn ran.
    Completed,
    /// `--until` was reached.
    UntilReached,
    /// The callback asked to stop.
    Callback,
    /// A panic at `tick` (including `not ported: ...`).
    Panic {
        /// Tick being executed.
        tick: u32,
        /// Panic message and location.
        message: String,
    },
}

/// Result of a replay.
#[derive(Debug)]
pub struct Outcome {
    /// Number of turns in the record.
    pub total_turns: usize,
    /// Number of ticks that ran to completion.
    pub ticks_run: u32,
    /// Why the replay stopped.
    pub stop: Stop,
}

/// What the callback receives after each completed tick.
pub struct TickInfo<'a> {
    /// The tick that just ran (`ticks()` before it ran).
    pub tick: u32,
    /// The record turn of that tick.
    pub turn: &'a Turn,
    /// The hash the engine computed inside the tick (every 10th tick).
    pub engine_hash: Option<f64>,
}

/// Loads `record_path` and runs it through the engine, calling `on_tick` after every tick; the
/// callback returns false to stop.
pub fn replay(
    record_path: &Path,
    maps: &Path,
    until: Option<u32>,
    on_tick: &mut dyn FnMut(&GameRunner, &TickInfo) -> bool,
) -> Result<Outcome> {
    let record = GameRecord::load_decompressed(record_path)?;
    let rules = Rules::for_commit(&record.git_commit)
        .with_context(|| format!("record commit {}", record.git_commit))?;
    let start = GameStartInfo::from_record(&record);
    let total_turns = record.turns.len();

    let created = catch_unwind(AssertUnwindSafe(|| create_game_runner(&start, maps, rules)));
    let mut runner = match created {
        Ok(r) => r?,
        Err(_) => {
            let message = LAST_PANIC.lock().unwrap().take().unwrap_or_default();
            return Ok(Outcome {
                total_turns,
                ticks_run: 0,
                stop: Stop::Panic { tick: 0, message },
            });
        }
    };

    let mut ticks_run = 0u32;
    for turn in &record.turns {
        let tick = runner.game.ticks();
        if until.is_some_and(|u| tick > u) {
            return Ok(Outcome {
                total_turns,
                ticks_run,
                stop: Stop::UntilReached,
            });
        }
        runner.add_turn(turn.clone());
        let step = catch_unwind(AssertUnwindSafe(|| runner.execute_next_tick()));
        let engine_hash = match step {
            Ok(Some(h)) => h,
            Ok(None) => unreachable!("a turn was just queued"),
            Err(_) => {
                let message = LAST_PANIC.lock().unwrap().take().unwrap_or_default();
                return Ok(Outcome {
                    total_turns,
                    ticks_run,
                    stop: Stop::Panic { tick, message },
                });
            }
        };
        ticks_run += 1;
        let info = TickInfo {
            tick,
            turn,
            engine_hash,
        };
        if !on_tick(&runner, &info) {
            return Ok(Outcome {
                total_turns,
                ticks_run,
                stop: Stop::Callback,
            });
        }
    }
    Ok(Outcome {
        total_turns,
        ticks_run,
        stop: Stop::Completed,
    })
}
