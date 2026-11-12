// SPDX-License-Identifier: AGPL-3.0-only
//! Named upstream changes a build contains, and the commit to rule-set mapping.
//!
//! `BuildClasses.toml` (generated, checked in) gives commit -> class and class -> parent; the
//! `EDGES` table below says which `Change`s each ported class adds to or removes from its parent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use thiserror::Error;

/// One upstream behaviour change, named after the upstream PR; more are added per build.
///
/// Engine code branches on [`Rules::has`] at the exact place upstream changed; `has` is true
/// when the build contains the changed (newer) behaviour.
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
    #[error("unsupported build {commit}; nearest known build {nearest} (class {class})")]
    UnknownBuild {
        /// The commit asked for.
        commit: String,
        /// The known build sharing the longest commit prefix.
        nearest: String,
        /// Class of that build.
        class: String,
    },
    /// The build is known but its class has no rule edges yet.
    #[error("build {commit} (class {class}) not ported yet")]
    NotPorted {
        /// The commit asked for.
        commit: String,
        /// Class of the build.
        class: String,
    },
}

/// Changes a class adds to / removes from its parent class.
struct Edge {
    class: &'static str,
    add: &'static [Change],
    remove: &'static [Change],
}

/// Ported classes, parents first. `C000` is the root and holds `ROOT_CHANGES`.
const EDGES: &[Edge] = &[Edge {
    class: "C000",
    add: &[],
    remove: &[],
}];

/// Changes of the baseline class C000.
const ROOT_CHANGES: &[Change] = &[Change::DetMath5191, Change::Sfc32PseudoRandom4233];

/// Commit of the baseline build v0.34.22.
pub const BASELINE_COMMIT: &str = "4e837520e886";

/// Shortest accepted commit prefix.
const COMMIT_PREFIX_LEN: usize = 12;

/// Parsed `BuildClasses.toml`.
struct ClassTable {
    /// commit12 -> class id.
    builds: BTreeMap<String, String>,
    /// class id -> parent class id (empty for the root).
    parents: BTreeMap<String, String>,
}

fn class_table() -> &'static ClassTable {
    static TABLE: OnceLock<ClassTable> = OnceLock::new();
    TABLE.get_or_init(|| {
        let text = include_str!("BuildClasses.toml");
        let value: toml::Value = text.parse().expect("BuildClasses.toml parses");
        let builds = value["builds"]
            .as_table()
            .expect("[builds] table")
            .iter()
            .map(|(k, v)| (k.clone(), v.as_str().expect("class id").to_string()))
            .collect();
        let parents = value["class"]
            .as_table()
            .expect("[class] table")
            .iter()
            .map(|(k, v)| {
                let parent = v["parent"].as_str().unwrap_or("").to_string();
                (k.clone(), parent)
            })
            .collect();
        ClassTable { builds, parents }
    })
}

/// The set of upstream changes a build contains.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rules {
    class: String,
    changes: BTreeSet<Change>,
}

impl Rules {
    /// Rules of the baseline build (class C000).
    pub fn baseline() -> Self {
        Self {
            class: "C000".to_string(),
            changes: ROOT_CHANGES.iter().copied().collect(),
        }
    }

    /// Rules of a class, or None when the class or one of its ancestors is not ported.
    pub fn for_class(class: &str) -> Option<Self> {
        let table = class_table();
        let mut chain = vec![class];
        while let Some(parent) = table.parents.get(*chain.last()?) {
            if parent.is_empty() {
                break;
            }
            chain.push(parent.as_str());
        }
        let mut changes: BTreeSet<Change> = ROOT_CHANGES.iter().copied().collect();
        if *chain.last()? != "C000" {
            return None;
        }
        for id in chain.iter().rev() {
            let edge = EDGES.iter().find(|e| e.class == *id)?;
            changes.extend(edge.add.iter().copied());
            for removed in edge.remove {
                changes.remove(removed);
            }
        }
        Some(Self {
            class: class.to_string(),
            changes,
        })
    }

    /// Rules for a record's `gitCommit` (full hash or any prefix of at least 12 characters).
    pub fn for_commit(commit: &str) -> Result<Self, RulesError> {
        let table = class_table();
        let key = commit.get(..COMMIT_PREFIX_LEN).unwrap_or("");
        let class = commit.len() >= COMMIT_PREFIX_LEN
            && !key.is_empty()
            && commit.len() <= 40
            && table.builds.contains_key(key);
        if !class {
            let nearest = table
                .builds
                .keys()
                .max_by_key(|b| common_prefix(b, commit))
                .cloned()
                .unwrap_or_default();
            let class = table.builds.get(&nearest).cloned().unwrap_or_default();
            return Err(RulesError::UnknownBuild {
                commit: commit.to_string(),
                nearest,
                class,
            });
        }
        let class = table.builds[key].clone();
        Self::for_class(&class).ok_or(RulesError::NotPorted {
            commit: commit.to_string(),
            class,
        })
    }

    /// Class id of the build (for example `C000`).
    pub fn class_id(&self) -> &str {
        &self.class
    }

    /// True when the build contains `change`.
    pub fn has(&self, change: Change) -> bool {
        self.changes.contains(&change)
    }
}

/// Number of leading characters two strings share.
fn common_prefix(a: &str, b: &str) -> usize {
    a.chars().zip(b.chars()).take_while(|(x, y)| x == y).count()
}

/// Every build commit (12 characters) of the table with its class, for tools and tests.
pub fn known_builds() -> impl Iterator<Item = (&'static str, &'static str)> {
    class_table()
        .builds
        .iter()
        .map(|(b, c)| (b.as_str(), c.as_str()))
}

/// True when the class has a rule set (it and all its ancestors are ported).
pub fn is_ported(class: &str) -> bool {
    Rules::for_class(class).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_and_unknown() {
        let r = Rules::for_commit("4e837520e886").unwrap();
        assert!(r.has(Change::DetMath5191) && r.has(Change::Sfc32PseudoRandom4233));
        assert_eq!(r.class_id(), "C000");
        assert!(Rules::for_commit("4e837520e886deadbeef").is_ok());
        assert!(matches!(
            Rules::for_commit("abc"),
            Err(RulesError::UnknownBuild { .. })
        ));
        assert!(matches!(
            Rules::for_commit("4e837520e885"),
            Err(RulesError::UnknownBuild { .. })
        ));
    }

    #[test]
    fn every_build_resolves_to_its_class() {
        let table = class_table();
        assert_eq!(table.builds.len(), 159);
        for (build, class) in known_builds() {
            assert!(table.parents.contains_key(class), "{build} -> {class}");
            match Rules::for_commit(build) {
                Ok(r) => assert_eq!(r.class_id(), class),
                Err(RulesError::NotPorted { class: c, .. }) => assert_eq!(c, class),
                Err(e) => panic!("{build}: {e}"),
            }
        }
        assert!(Rules::for_commit("a26585a47b17").is_err());
    }

    #[test]
    fn unported_class_errors_with_class() {
        let e = Rules::for_commit("a26585a47b17").unwrap_err();
        assert_eq!(
            e.to_string(),
            "build a26585a47b17 (class C079) not ported yet"
        );
    }

    #[test]
    fn builds_doc_lists_resolve() {
        // Every 12-hex commit mentioned in Docs/Builds.md is a known build.
        let docs = include_str!("../../../../Docs/Builds.md");
        let table = class_table();
        let mut seen = 0;
        for word in docs.split(|c: char| !c.is_ascii_hexdigit()) {
            if word.len() == 12 && table.builds.contains_key(word) {
                seen += 1;
            }
        }
        assert!(seen >= 100, "only {seen} builds found in Builds.md");
    }
}
