// SPDX-License-Identifier: AGPL-3.0-only
//! Per-build map resources: reads the map files of a record's commit from an upstream clone when
//! they differ from the baseline build's.

use super::Rules::BASELINE_COMMIT;
use anyhow::{Context, Result, bail};
use ofsim_record::Records::GameRecord;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Runs `git -C <repo> <args>` and returns stdout, or None when git fails.
fn git_output(repo: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .ok()?;
    out.status.success().then_some(out.stdout)
}

/// Git object id of `resources/maps/<key>` in `commit`, or None when the directory is absent.
fn map_tree_id(upstream: &Path, commit: &str, key: &str) -> Option<String> {
    let spec = format!("{commit}:resources/maps/{key}");
    let out = git_output(upstream, &["rev-parse", &spec])?;
    Some(String::from_utf8_lossy(&out).trim().to_string())
}

/// Maps root to use for `record`: `baseline_maps` when the record's map is identical in the
/// record's commit and the baseline, otherwise a directory extracted (once) from `upstream` into
/// `<upstream parent>/Maps/<commit12>/`.
pub fn maps_root_for(
    record: &GameRecord,
    baseline_maps: &Path,
    upstream: &Path,
) -> Result<PathBuf> {
    maps_root_for_commit(
        &record.git_commit,
        &record.info.config.game_map,
        baseline_maps,
        upstream,
    )
}

/// [`maps_root_for`] for a commit and a map wire name (`GameMapType` value).
pub fn maps_root_for_commit(
    commit: &str,
    wire: &str,
    baseline_maps: &Path,
    upstream: &Path,
) -> Result<PathBuf> {
    let commit12 = commit.get(..12).unwrap_or(commit);
    if commit12 == BASELINE_COMMIT {
        return Ok(baseline_maps.to_path_buf());
    }
    let key = ofsim_map::directory_key_of(wire)?;
    if !upstream.join(".git").exists() {
        bail!(
            "build {commit12} needs the upstream clone to resolve maps; {} is not a git checkout",
            upstream.display()
        );
    }
    let theirs = map_tree_id(upstream, commit, &key);
    let baseline = map_tree_id(upstream, BASELINE_COMMIT, &key);
    if theirs.is_none() || theirs == baseline {
        return Ok(baseline_maps.to_path_buf());
    }
    let base = upstream
        .parent()
        .unwrap_or(Path::new("."))
        .join("Maps")
        .join(commit12);
    let root = base.join("resources").join("maps");
    if root.join(&key).join("manifest.json").exists() {
        return Ok(root);
    }
    std::fs::create_dir_all(&base)?;
    let archive = git_output(
        upstream,
        &["archive", commit, &format!("resources/maps/{key}")],
    )
    .with_context(|| format!("git archive of maps/{key} at {commit12}"))?;
    let mut tar = Command::new("tar")
        .arg("-x")
        .arg("-C")
        .arg(&base)
        .stdin(std::process::Stdio::piped())
        .spawn()
        .context("spawning tar")?;
    {
        use std::io::Write;
        tar.stdin.take().context("tar stdin")?.write_all(&archive)?;
    }
    if !tar.wait()?.success() {
        bail!("tar failed extracting maps/{key} of {commit12}");
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Skipped when the local upstream clone is absent.
    #[test]
    fn changed_map_is_extracted_and_unchanged_map_uses_baseline() {
        let upstream = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../OpenFrontIO");
        if !upstream.join(".git").exists() {
            return;
        }
        let baseline = Path::new("/nonexistent/baseline");
        // v0.34.12 predates the Channel Islands rework of the baseline.
        let changed = maps_root_for_commit("7c27263390d8", "Channel Islands", baseline, &upstream);
        let changed = changed.unwrap();
        assert!(changed.join("channelislands/map.bin").exists());
        let same = maps_root_for_commit("7c27263390d8", "World", baseline, &upstream).unwrap();
        assert_eq!(same, baseline);
    }
}
