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
        if commit.len() >= BASELINE_COMMIT.len() && commit.starts_with(BASELINE_COMMIT) {
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
        assert_eq!(
            Rules::for_commit("abc"),
            Err(RulesError::UnknownBuild("abc".into()))
        );
    }
}
