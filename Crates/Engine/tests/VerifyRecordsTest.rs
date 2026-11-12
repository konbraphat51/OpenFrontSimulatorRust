// SPDX-License-Identifier: AGPL-3.0-only
//! Regression test: every baseline record keeps matching at least the recorded number of hashes,
//! and no recorded hash that the engine computes ever differs.
//!
//! Needs the local cache (`Cache/Records/<commit>`, baseline worktree maps); skipped when absent.
//! When an execution lands and more hashes match, raise the numbers in `Golden/VerifyMinimums.json`.

#![allow(non_snake_case)]

use ofsim_engine::Runner::Replay::{Verdict, silence_panics, verify};
use serde_json::Value;
use std::path::{Path, PathBuf};

const BASELINE: &str = "4e837520e886";

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
        let report = verify(&file, &maps, None).unwrap();
        let min = minimums[&name].as_u64().unwrap_or(0) as usize;
        if report.verdict == Verdict::Mismatch {
            failures.push(format!("{name}: hash mismatch {:?}", report.mismatch));
        }
        if report.matched < min {
            failures.push(format!(
                "{name}: matched {} hashes, expected at least {min} ({:?})",
                report.matched, report.panic
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
