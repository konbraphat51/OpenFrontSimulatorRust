// SPDX-License-Identifier: AGPL-3.0-only
//! Named upstream changes a build contains, and the commit to rule-set table.

use std::collections::BTreeSet;
use thiserror::Error;

/// One upstream behaviour change, named after the upstream PR; more are added per build.
///
/// Engine code branches on [`Rules::has`] at the exact place upstream changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Change {
    /// PR 5191: deterministic `DetMath` replaces V8's `Math.exp/log/pow/atan2`.
    DetMath5191,
    /// PR 4233: `PseudoRandom` is sfc32 seeded by splitmix32 (older builds use another generator).
    Sfc32PseudoRandom4233,
}

/// Raised for a commit this engine has no rule set for.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RulesError {
    /// The commit is not in the build table.
    #[error("unknown build commit {0}")]
    UnknownBuild(String),
}

/// The set of upstream changes a build contains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    changes: BTreeSet<Change>,
}

/// Commit of the baseline build v0.34.22.
pub const BASELINE_COMMIT: &str = "4e837520e886";

/// Builds of class C000 (v0.34.19 to v0.34.23): identical simulation code, so the baseline rules apply.
pub const BASELINE_CLASS_COMMITS: [&str; 5] = [
    "a188868bc16c",
    "feba4a51c334",
    BASELINE_COMMIT,
    "d7075bface21",
    "71154c52c2a2",
];

impl Rules {
    /// Rules of the baseline build (every known change applied).
    pub fn baseline() -> Self {
        Self {
            changes: [Change::DetMath5191, Change::Sfc32PseudoRandom4233]
                .into_iter()
                .collect(),
        }
    }

    /// Rules for a record's `gitCommit` (full hash or any prefix of at least 12 characters).
    pub fn for_commit(commit: &str) -> Result<Self, RulesError> {
        let is_baseline_class = BASELINE_CLASS_COMMITS
            .iter()
            .any(|known| commit.len() >= known.len() && commit.starts_with(known));
        if is_baseline_class {
            return Ok(Self::baseline());
        }
        Err(RulesError::UnknownBuild(commit.to_string()))
    }

    /// True when the build contains `change`.
    pub fn has(&self, change: Change) -> bool {
        self.changes.contains(&change)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_and_unknown() {
        let r = Rules::for_commit("4e837520e886").unwrap();
        assert!(r.has(Change::DetMath5191) && r.has(Change::Sfc32PseudoRandom4233));
        assert!(Rules::for_commit("4e837520e886deadbeef").is_ok());
        for commit in BASELINE_CLASS_COMMITS {
            assert_eq!(Rules::for_commit(commit).unwrap(), Rules::baseline());
        }
        assert_eq!(
            Rules::for_commit("abc"),
            Err(RulesError::UnknownBuild("abc".into()))
        );
    }
}
