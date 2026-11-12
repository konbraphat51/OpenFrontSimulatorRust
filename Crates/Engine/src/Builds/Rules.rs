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
    /// PR 5823: an attack with nothing to conquer retreats in `init` instead of lingering.
    AttackRetreatNoTargets5823,
    /// PR 5587: trade ship (230 -> 330) and train (500 -> 560) saturation midpoints.
    SaturationMidpoints5587,
    /// PR 5545: `PlayerExecution` skips a hole cluster when picking the largest border cluster.
    InternalBorderAnnexFix5545,
    /// PR 5525: nukes credit no kill for units of friendly (ally, teammate) owners.
    NukeFriendlyKillNoCredit5525,
    /// PR 5462: train saturation midpoint 300 -> 500.
    TrainSaturation500Pr5462,
    /// PR 5436: team count of Duos/Trios/Quads is at least 2.
    TeamCountMinTwo5436,
    /// PR 5425: player-vs-player attack speed divisor 7.77 -> 8.55.
    AttackSpeedDivisor5425,
    /// PR 5564: Channel Islands gets its custom tribes and the `western` tribe theme.
    ChannelIslandsTribes5564,
    /// PR 5547 and sibling new-map PRs: tribe themes of Canary Islands, New Zealand, Pulicat Lake.
    NewMapThemes5547,
    /// PR 5907: per-player spawn boost for small trade and rail economies replaces the fleet-size boost.
    SmallEconomyBoost5907,
    /// PR 5805 (package split): water graph edges are renumbered by node pair after the build.
    WaterGraphEdgeSort5805,
    /// PR 5955 and siblings: `PlayerExecution` judges clusters first and annexes severed land.
    SeveredLandAnnex5955,
    /// PR 5919: "stop trading with enemies" (embargo all) skips allies.
    EmbargoAllSkipsAllies5919,
    /// PR 5719: a train reconciles a split of the railroad it is on.
    TrainSplitReconcile5719,
    /// PR 5899: nations play more like winning humans (attack, nuke, structure, MIRV and warship behaviour, bot annexation).
    NationsLikeHumans5899,
    /// PR 5833: a human may send several emojis per window (nations still one).
    EmojiBatch5833,
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
const EDGES: &[Edge] = &[
    Edge {
        class: "C000",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C001",
        add: &[Change::AttackRetreatNoTargets5823],
        remove: &[],
    },
    Edge {
        class: "C002",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C003",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C004",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C005",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C006",
        add: &[],
        remove: &[Change::ChannelIslandsTribes5564],
    },
    Edge {
        class: "C007",
        add: &[],
        remove: &[
            Change::InternalBorderAnnexFix5545,
            Change::SaturationMidpoints5587,
        ],
    },
    Edge {
        class: "C008",
        add: &[],
        remove: &[Change::NukeFriendlyKillNoCredit5525],
    },
    Edge {
        class: "C009",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C010",
        add: &[],
        remove: &[Change::TrainSaturation500Pr5462],
    },
    Edge {
        class: "C011",
        add: &[],
        remove: &[Change::TeamCountMinTwo5436, Change::AttackSpeedDivisor5425],
    },
    Edge {
        class: "C012",
        add: &[Change::NewMapThemes5547],
        remove: &[],
    },
    Edge {
        class: "C081",
        add: &[
            Change::SmallEconomyBoost5907,
            Change::EmojiBatch5833,
            Change::NationsLikeHumans5899,
            Change::WaterGraphEdgeSort5805,
            Change::TrainSplitReconcile5719,
            Change::SeveredLandAnnex5955,
            Change::EmbargoAllSkipsAllies5919,
        ],
        remove: &[],
    },
    Edge {
        class: "C082",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C090",
        add: &[],
        remove: &[],
    },
];

/// Changes of the baseline class C000.
const ROOT_CHANGES: &[Change] = &[
    Change::DetMath5191,
    Change::Sfc32PseudoRandom4233,
    Change::SaturationMidpoints5587,
    Change::InternalBorderAnnexFix5545,
    Change::NukeFriendlyKillNoCredit5525,
    Change::TrainSaturation500Pr5462,
    Change::TeamCountMinTwo5436,
    Change::AttackSpeedDivisor5425,
    Change::ChannelIslandsTribes5564,
];

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

/// Builds of class C000 (v0.34.19 to v0.34.23): identical simulation code, so the baseline rules apply.
pub const BASELINE_CLASS_COMMITS: [&str; 5] = [
    "a188868bc16c",
    "feba4a51c334",
    BASELINE_COMMIT,
    "d7075bface21",
    "71154c52c2a2",
];

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
        for commit in BASELINE_CLASS_COMMITS {
            assert_eq!(Rules::for_commit(commit).unwrap().class_id(), "C000");
        }
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

    #[test]
    fn era_d_classes_are_ported_with_the_expected_changes() {
        for n in 0..=12 {
            assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
        }
        assert!(!is_ported("C013"));
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        assert!(rules("3f5633b92c95").has(Change::AttackRetreatNoTargets5823));
        assert!(!rules("824d1412be58").has(Change::AttackRetreatNoTargets5823));
        let c006 = rules("7c27263390d8");
        assert!(!c006.has(Change::ChannelIslandsTribes5564));
        assert!(c006.has(Change::InternalBorderAnnexFix5545));
        let c007 = rules("82e5fce9502b");
        assert!(!c007.has(Change::InternalBorderAnnexFix5545));
        assert!(!c007.has(Change::SaturationMidpoints5587));
        assert!(c007.has(Change::TrainSaturation500Pr5462));
        assert!(!rules("ece79bfd3348").has(Change::NukeFriendlyKillNoCredit5525));
        assert!(!rules("222e4078982e").has(Change::TrainSaturation500Pr5462));
        let c011 = rules("1e973bb534b8");
        assert!(
            !c011.has(Change::TeamCountMinTwo5436) && !c011.has(Change::AttackSpeedDivisor5425)
        );
        let c012 = rules("e910d727d2dd");
        assert!(c012.has(Change::NewMapThemes5547) && c012.has(Change::ChannelIslandsTribes5564));
        assert!(!rules("4e837520e886").has(Change::NewMapThemes5547));
    }
}
