// SPDX-License-Identifier: AGPL-3.0-only
//! Regression test: every record of the baseline class C000 (v0.34.19 to v0.34.23) replays with every recorded hash matching.
//!
//! Covers the five builds of class C000. Needs the local cache (`Cache/Records/<commit>`, maps from
//! `Cache/Worktrees/<commit>` or `Cache/Maps/<commit>`, else the baseline worktree); skipped when absent.
//! `Golden/VerifyMinimums.json` holds the number of recorded hashes per record. By default each
//! replay stops after `QUICK_TICKS` ticks (debug builds are slow); `OFSIM_VERIFY_FULL=1` replays
//! every record to its end and expects all hashes of `VerifyMinimums.json` to match.

#![allow(non_snake_case)]

use ofsim_engine::Runner::Replay::{Verdict, silence_panics, verify};
use serde_json::Value;
use std::path::{Path, PathBuf};

use ofsim_engine::Builds::Rules::{BASELINE_CLASS_COMMITS, BASELINE_COMMIT};

/// Records per non-baseline build replayed by the default (quick) run.
const QUICK_RECORDS_PER_BUILD: usize = 5;

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

/// Map directory of a build: its own worktree or map snapshot, else the baseline worktree.
fn maps_of(cache: &Path, commit: &str) -> PathBuf {
    [
        cache.join("Worktrees").join(commit).join("resources/maps"),
        cache.join("Maps").join(commit).join("resources/maps"),
        cache
            .join("Worktrees")
            .join(BASELINE_COMMIT)
            .join("resources/maps"),
    ]
    .into_iter()
    .find(|p| p.is_dir())
    .unwrap_or_default()
}

#[test]
fn matched_hashes_do_not_regress() {
    let Some(cache) = find_cache() else {
        eprintln!("skipped: no Cache/Records");
        return;
    };
    let full = std::env::var_os("OFSIM_VERIFY_FULL").is_some();
    silence_panics();
    let minimums: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/Golden/VerifyMinimums.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut failures = Vec::new();
    let mut checked = 0;
    for commit in BASELINE_CLASS_COMMITS {
        let records = cache.join("Records").join(commit);
        let maps = maps_of(&cache, commit);
        if !records.is_dir() || !maps.is_dir() {
            eprintln!("skipped {commit}: no records or maps");
            continue;
        }
        let mut files: Vec<PathBuf> = std::fs::read_dir(&records)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.to_string_lossy().ends_with(".json.gz"))
            .collect();
        files.sort();
        if !full && commit != BASELINE_COMMIT {
            files.truncate(QUICK_RECORDS_PER_BUILD);
        }
        for file in files {
            let name = file
                .file_name()
                .unwrap()
                .to_string_lossy()
                .trim_end_matches(".json.gz")
                .to_string();
            let until = if full { None } else { Some(QUICK_TICKS) };
            let report = verify(&file, &maps, until).unwrap();
            checked += 1;
            let expected = minimums[&name].as_u64().map(|n| n as usize);
            if !matches!(report.verdict, Verdict::Ok | Verdict::Partial) {
                failures.push(format!(
                    "{commit}/{name}: {:?}, mismatch {:?}, panic {:?}",
                    report.verdict, report.mismatch, report.panic
                ));
            } else if report.matched != report.recorded
                || (full && Some(report.matched) != expected)
            {
                failures.push(format!(
                    "{commit}/{name}: matched {} of {} recorded hashes (golden {expected:?})",
                    report.matched, report.recorded
                ));
            }
        }
    }
    eprintln!("checked {checked} records");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
