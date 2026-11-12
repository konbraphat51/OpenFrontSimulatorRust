// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim batch`: replays many records in parallel, one JSON line per record, resumable.

use crate::Dump::dump;
use anyhow::{Result, bail};
use ofsim_engine::Runner::Replay::{Verdict, verify};
use ofsim_record::Records::GameRecord;
use rayon::prelude::*;
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Options of one batch run.
pub struct BatchArgs<'a> {
    /// Directory or glob (`dir/*.json.gz`) of records.
    pub records: &'a str,
    /// Map resources directory.
    pub maps: &'a Path,
    /// Upstream clone, read for builds whose map files differ from the baseline.
    pub upstream: &'a Path,
    /// Output root; one subdirectory per record.
    pub out: &'a Path,
    /// Snapshot interval.
    pub every: u32,
    /// Worker threads (0 = all cores).
    pub jobs: usize,
    /// Only verify hashes, write nothing.
    pub verify_only: bool,
    /// Redo records whose `Result.json` exists.
    pub force: bool,
}

/// Tail-anchored `*` and `?` wildcard match.
fn wildcard(pattern: &[u8], text: &[u8]) -> bool {
    match (pattern.first(), text.first()) {
        (None, None) => true,
        (Some(b'*'), _) => {
            wildcard(&pattern[1..], text) || (!text.is_empty() && wildcard(pattern, &text[1..]))
        }
        (Some(b'?'), Some(_)) => wildcard(&pattern[1..], &text[1..]),
        (Some(p), Some(t)) if p == t => wildcard(&pattern[1..], &text[1..]),
        _ => false,
    }
}

/// Record files of a directory or of a `dir/pattern` glob, sorted by name.
pub fn list_records(spec: &str) -> Result<Vec<PathBuf>> {
    let path = Path::new(spec);
    let (dir, pattern) = if path.is_dir() {
        (path.to_path_buf(), "*.json*".to_string())
    } else {
        let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
        match name {
            Some(n) => (dir.unwrap_or(Path::new(".")).to_path_buf(), n),
            None => bail!("bad --records {spec}"),
        }
    };
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().map(|n| n.to_string_lossy().into_owned());
            n.is_some_and(|n| {
                wildcard(pattern.as_bytes(), n.as_bytes())
                    && (n.ends_with(".json.gz") || n.ends_with(".json"))
            })
        })
        .collect();
    files.sort();
    Ok(files)
}

/// Record name without the `.json.gz` suffix.
pub fn record_name(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    name.trim_end_matches(".gz")
        .trim_end_matches(".json")
        .to_string()
}

fn line(name: &str, status: &str, ticks: u32, seconds: f64, extra: &str) -> String {
    let rate = if seconds > 0.0 {
        f64::from(ticks) / seconds
    } else {
        0.0
    };
    format!(
        "{{\"record\":{},\"status\":\"{status}\",\"ticks\":{ticks},\"seconds\":{seconds:.3},\"ticksPerSecond\":{rate:.0}{extra}}}",
        serde_json::to_string(name).unwrap()
    )
}

fn run_one(path: &Path, args: &BatchArgs) -> String {
    let name = record_name(path);
    let out = args.out.join(&name);
    if !args.verify_only && !args.force && out.join("Result.json").exists() {
        return line(&name, "skipped", 0, 0.0, "");
    }
    let started = Instant::now();
    let work = catch_unwind(AssertUnwindSafe(|| -> Result<String> {
        let record = GameRecord::load_decompressed(path)?;
        let maps = ofsim_engine::Builds::Maps::maps_root_for(&record, args.maps, args.upstream)?;
        if args.verify_only {
            let report = verify(path, &maps, None)?;
            let secs = started.elapsed().as_secs_f64();
            let status = match report.verdict {
                Verdict::Ok => "ok",
                Verdict::Mismatch => "desync",
                _ => "error",
            };
            let mut extra = format!(",\"hashesChecked\":{}", report.matched);
            if let Some((tick, _, _)) = report.mismatch {
                extra.push_str(&format!(",\"firstMismatchTick\":{tick}"));
            }
            if let Some((tick, message)) = &report.panic {
                extra.push_str(&format!(
                    ",\"error\":{}",
                    serde_json::to_string(&format!("{message} (tick {tick})"))?
                ));
            }
            return Ok(line(&name, status, report.ticks_run, secs, &extra));
        }
        let result = dump(&record, &maps, &out, args.every)?;
        let secs = started.elapsed().as_secs_f64();
        let mut extra = format!(
            ",\"hashesChecked\":{},\"snapshots\":{}",
            result.hashes_checked,
            result.snapshot_ticks.len()
        );
        if let Some(t) = result.first_mismatch_tick {
            extra.push_str(&format!(",\"firstMismatchTick\":{t}"));
        }
        if let Some(e) = &result.error {
            extra.push_str(&format!(",\"error\":{}", serde_json::to_string(e)?));
        }
        Ok(line(&name, result.status, result.final_tick, secs, &extra))
    }));
    let secs = started.elapsed().as_secs_f64();
    let error_line = |message: String| {
        line(
            &name,
            "error",
            0,
            secs,
            &format!(",\"error\":{}", serde_json::to_string(&message).unwrap()),
        )
    };
    match work {
        Ok(Ok(l)) => l,
        Ok(Err(e)) => error_line(format!("{e:#}")),
        Err(_) => error_line("panic".to_string()),
    }
}

/// Runs the batch; returns the number of records that did not end `ok` or `skipped`.
pub fn batch(args: &BatchArgs) -> Result<usize> {
    let files = list_records(args.records)?;
    if files.is_empty() {
        bail!("no records match {}", args.records);
    }
    if !args.verify_only {
        std::fs::create_dir_all(args.out)?;
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(args.jobs)
        .stack_size(256 << 20)
        .build()?;
    let stdout = std::sync::Mutex::new(std::io::stdout());
    let failures: usize = pool.install(|| {
        files
            .par_iter()
            .map(|path| {
                let text = run_one(path, args);
                let bad = !(text.contains("\"status\":\"ok\"")
                    || text.contains("\"status\":\"skipped\""));
                let mut out = stdout.lock().unwrap();
                let _ = writeln!(out, "{text}");
                let _ = out.flush();
                usize::from(bad)
            })
            .sum()
    });
    Ok(failures)
}
