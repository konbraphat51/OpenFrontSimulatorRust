// SPDX-License-Identifier: AGPL-3.0-only
//! Replay of a record through the engine: the loop shared by `ofsim verify`, `ofsim trace` and the
//! regression tests. Panics (including `not ported: ...`) are caught per record and reported with
//! the tick they happened on.

use super::GameRunner::{GameRunner, create_game_runner};
use crate::Builds::Rules::Rules;
use crate::Compat::JsNumber::js_number_to_string;
use anyhow::{Context, Result};
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

/// Verdict of a verification run.
#[derive(Debug, PartialEq, Eq)]
pub enum Verdict {
    /// Every recorded hash matched through the last turn.
    Ok,
    /// A recorded hash differed.
    Mismatch,
    /// A not-ported execution was hit before the end.
    NotPorted,
    /// Some other panic stopped the replay.
    Panic,
    /// Stopped early by `--until` with every hash so far matching.
    Partial,
}

/// Outcome of verifying one record.
#[derive(Debug)]
pub struct Report {
    /// The verdict.
    pub verdict: Verdict,
    /// Hashes compared and equal.
    pub matched: usize,
    /// Hashes the record holds in total.
    pub recorded: usize,
    /// Turns in the record.
    pub turns: usize,
    /// Ticks that ran.
    pub ticks_run: u32,
    /// First mismatch (tick, expected, actual).
    pub mismatch: Option<(u32, f64, f64)>,
    /// Stop tick and message of a panic.
    pub panic: Option<(u32, String)>,
}

impl Report {
    /// One line of `key=value` pairs.
    pub fn line(&self, name: &str) -> String {
        let verdict = match self.verdict {
            Verdict::Ok => "OK",
            Verdict::Mismatch => "MISMATCH",
            Verdict::NotPorted => "NOT_PORTED",
            Verdict::Panic => "PANIC",
            Verdict::Partial => "PARTIAL",
        };
        let mut s = format!(
            "{name} {verdict} matched={}/{} turns={} ticks_run={}",
            self.matched, self.recorded, self.turns, self.ticks_run
        );
        if let Some((tick, expected, actual)) = self.mismatch {
            s.push_str(&format!(
                " first_mismatch_tick={tick} expected={} actual={}",
                js_number_to_string(expected),
                js_number_to_string(actual)
            ));
        }
        if let Some((tick, message)) = &self.panic {
            s.push_str(&format!(" stop_tick={tick} message=\"{message}\""));
        }
        s
    }
}

/// Verifies one record.
pub fn verify(record: &Path, maps: &Path, until: Option<u32>) -> Result<Report> {
    let mut matched = 0usize;
    let mut mismatch: Option<(u32, f64, f64)> = None;
    let mut recorded = 0usize;
    let outcome = replay(record, maps, until, &mut |_runner, info| {
        if let Some(expected) = info.turn.hash {
            match info.engine_hash {
                Some(actual) if actual == expected => matched += 1,
                Some(actual) => {
                    mismatch = Some((info.tick, expected, actual));
                    return false;
                }
                None => {
                    // The record holds a hash on a tick the engine does not hash.
                    mismatch = Some((info.tick, expected, f64::NAN));
                    return false;
                }
            }
        }
        true
    })?;
    let record_data = ofsim_record::Records::GameRecord::load_decompressed(record)?;
    for t in &record_data.turns {
        if t.hash.is_some() && until.is_none_or(|u| t.turn_number <= u) {
            recorded += 1;
        }
    }
    let (verdict, panic) = match (&outcome.stop, &mismatch) {
        (_, Some(_)) => (Verdict::Mismatch, None),
        (Stop::Panic { tick, message }, _) => {
            let v = if message.starts_with("not ported:") {
                Verdict::NotPorted
            } else {
                Verdict::Panic
            };
            (v, Some((*tick, message.clone())))
        }
        (Stop::UntilReached, _) => (Verdict::Partial, None),
        _ => (Verdict::Ok, None),
    };
    Ok(Report {
        verdict,
        matched,
        recorded,
        turns: outcome.total_turns,
        ticks_run: outcome.ticks_run,
        mismatch,
        panic,
    })
}
