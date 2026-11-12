// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim verify`: replays a record and compares the engine hash with the recorded one.

use crate::JsNumberFormat::js_number_to_string;
use crate::Replaying::{Stop, replay};
use anyhow::Result;
use std::path::Path;

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
