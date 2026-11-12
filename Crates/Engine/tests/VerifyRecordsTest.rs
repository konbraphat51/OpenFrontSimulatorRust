// SPDX-License-Identifier: AGPL-3.0-only
//! Regression test: every record of the baseline class replays with every recorded hash matching.
//!
//! Needs the local cache (`Cache/Records/<commit>`, baseline worktree maps); skipped when absent.
//! `Golden/VerifyMinimums.json` holds the number of recorded hashes per record. By default each
//! replay stops after `QUICK_TICKS` ticks (debug builds are slow); `OFSIM_VERIFY_FULL=1` replays
//! every record to its end and expects all hashes of `VerifyMinimums.json` to match.

#![allow(non_snake_case)]

use ofsim_engine::Runner::Replay::{Verdict, silence_panics, verify};
use serde_json::Value;
use std::path::{Path, PathBuf};

const BASELINE: &str = "4e837520e886";

/// Tick limit of the default (quick) run.
const QUICK_TICKS: u32 = 400;

fn find_cache() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OFSIM_CACHE") {
        return Some(PathBuf::from(path));
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|ancestor| {
            let nested = ancestor.join("Cache");
            if nested.join("Records").is_dir() {
                Some(nested)
            } else if ancestor.join("Records").is_dir() && ancestor.join("Worktrees").is_dir() {
                Some(ancestor.to_path_buf())
            } else {
                None
            }
        })
}

#[test]
fn matched_hashes_do_not_regress() {
    let Some(cache) = find_cache() else {
        eprintln!("skipped: no Cache/Records");
        return;
    };
    let records = cache.join("Records").join(BASELINE);
    let maps = cache
        .join("Worktrees")
        .join(BASELINE)
        .join("resources/maps");
    if !records.is_dir() || !maps.is_dir() {
        eprintln!("skipped: no baseline records or maps");
        return;
    }
    let full = std::env::var_os("OFSIM_VERIFY_FULL").is_some();
    silence_panics();
    let minimums: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/Golden/VerifyMinimums.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&records)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
        .collect();
    files.sort();
    let mut failures = Vec::new();
    for file in files {
        let name = file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .trim_end_matches(".json.gz")
            .to_string();
        let until = if full { None } else { Some(QUICK_TICKS) };
        let report = verify(&file, &maps, until).unwrap();
        let expected = minimums[&name].as_u64().map(|n| n as usize);
        if !matches!(report.verdict, Verdict::Ok | Verdict::Partial) {
            failures.push(format!(
                "{name}: {:?}, mismatch {:?}, panic {:?}",
                report.verdict, report.mismatch, report.panic
            ));
        } else if report.matched != report.recorded || (full && Some(report.matched) != expected) {
            failures.push(format!(
                "{name}: matched {} of {} recorded hashes (golden {expected:?})",
                report.matched, report.recorded
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
