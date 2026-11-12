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
    /// PR 5173 and siblings: `attackLogic` is pure with per-tile tick fractions (older builds use `attackTilesPerTick`).
    AttackLogicRework5173,
    /// PR 5111: an attack holds the troops actually deducted from its owner.
    AttackTroopsDeducted5111,
    /// PR 4890: `handleDeadDefender` converges over up to 100 passes (older builds do exactly 10).
    AnnexConvergeLoop4890,
    /// PR 5027: annexing needs an enclosed territory; impassable tiles count as map edge.
    AnnexEnclosedOnly5027,
    /// PR 4892: a donate-gold intent without an amount keeps its null gold.
    DonateGoldKeepNull4892,
    /// PR 4953: the Doomsday Clock execution goes inactive once a winner is set.
    DoomsdayIdleOnWinner4953,
    /// PR 4867: a client spawn intent is gated on the phase it was queued in.
    SpawnIntentGate4867,
    /// PR 4921: spawn retries beyond 750 ignore the minimum distance between players.
    SpawnRelaxMinDist4921,
    /// PR 5321: a failed spawn restores the player's previous tiles.
    SpawnRollback5321,
    /// PR 5347: multiplayer spawn phase 200 turns (older builds 300).
    SpawnPhase20s5347,
    /// PR 5200: a train re-resolves a split railroad instead of despawning.
    TrainSplitResolve5200,
    /// PR 5302: trade ship and train spawn rates are throttled by saturation curves.
    TradeTrainSaturation5302,
    /// PR 5223: the win bar is 80 percent in team games too and the win test is cross-multiplied.
    WinThresholdSame5223,
    /// PR 5054: an alliance cancels in-flight MIRVs and warheads too.
    AllianceCancelMirv5054,
    /// PR 5188 and 4927: fixed-point Bezier curve, normalised MIRV flight and staged warheads.
    MirvNormalize5188,
    /// PR 5197 and 5040: cached SAM interception states and dynamic range on upgrade.
    SamTargeting5197,
    /// PR 5332: no teammate aftergame nuking in singleplayer.
    AftergameSingleplayer5332,
    /// PR 5330: smarter betrayal logic for Hard and Impossible nations.
    NationBetray5330,
    /// PR 5294: the juicy attack strategy of nations.
    NationJuicy5294,
    /// PR 5290: nations expanding into terra nullius keep a troop send cap floor.
    NationExpansionCap5290,
    /// PR 5344: crowded-map first structure uses its own one-shot flag.
    NationCrowdedFirst5344,
    /// PR 5223: nation victory denial thresholds as whole percents (one ladder).
    NationVictoryDenialPercent5223,
    /// PR 4758: duo/trio/quad team forming prefers the fullest team.
    TeamAssignDuos4758,
    /// PR 4903: rails cannot start on or cross impassable terrain.
    RailNoImpassable4903,
    /// PR 4975: water nukes fix the minimap ocean bit and magnitudes.
    WaterNukeMinimap4975,
    /// PR 5096: a station set to its own cluster does not leave it first.
    StationClusterSame5096,
    /// PR 5062: overtime lowers the win bar late in a game.
    Overtime5062,
    /// PR 4640: build and upgrade intents carry an amount (stacked purchases).
    BulkPurchase4640,
    /// PR 5030: team modes climb a higher Doomsday Clock ladder.
    DoomsdayTeamLevels5030,
    /// PR 5052: rotted territory becomes fallout wasteland.
    DoomsdayRotWasteland5052,
    /// PR 5907: per-player spawn boost for small trade and rail economies replaces the fleet-size boost.
    SmallEconomyBoost5907,
    /// PR 5805 (package split): water graph edges are renumbered by node pair after the build.
    WaterGraphEdgeSort5805,
    /// PR 5955 and siblings: `PlayerExecution` judges clusters first and annexes severed land.
    SeveredLandAnnex5955,
    /// PR 5919: "stop trading with enemies" (embargo all) skips allies.
    EmbargoAllSkipsAllies5919,
    /// PR 6087 (nation AI split): tribes may pick an allied traitor, alliance renewal does not count the renewed alliance.
    NationAiSplit6087,
    /// PR 6089: an attack checks that it is still active before refunding a completed retreat.
    AttackInactiveBeforeRetreat6089,
    /// PR 6016: a water nuke turns a mini map land tile to water when any of its four tiles is water.
    WaterNukeMiniTile6016,
    /// PR 6204 and siblings: the onboarding tutorial mode (nations and tribes only grow into free land).
    TutorialMode6204,
    /// PR 6242: the tutorial's scripted silo, warship and collapse, smaller atom bombs.
    TutorialSilo6242,
    /// PR 6259 and siblings: friend groups are seated back to back in team assignment.
    TeamFriendGroups6259,
    /// PR 5719: a train reconciles a split of the railroad it is on.
    TrainSplitReconcile5719,
    /// PR 5899: nations play more like winning humans (attack, nuke, structure, MIRV and warship behaviour, bot annexation).
    NationsLikeHumans5899,
    /// v0.36.0: the game-state hash includes a digest of the rail network.
    RailNetworkHash36,
    /// v0.36.0: `GameMap.bfs` is a FIFO breadth-first search (tile order and filter call order change).
    BfsFifoOrder36,
    /// v0.36.0: `PlayerExecution` ranks clusters by 4-way territory area and has a pinch pass.
    PlayerClusterRewrite36,
    /// v0.36.0: a port spawns on the first shore tile of the valid structure tiles (no radius-20 search).
    PortSpawnFirstShore36,
    /// PR 5833: a human may send several emojis per window (nations still one).
    EmojiBatch5833,
    /// PR 4340: impassable terrain (magnitude 31 land cannot be owned, attacked or nuked).
    ImpassableTerrain4340,
    /// PR 4433: warships gain veterancy (more health and shell damage) from kills and captures.
    WarshipVeterancy4433,
    /// PR 4706: MIRV warheads are standard nukes (per-type speeds 15 and 22, real flight, SAM interception).
    MirvWarheadsStandard4706,
    /// PR 4707: launching a MIRV puts the silo on cooldown.
    MirvSiloCooldown4707,
    /// PR 4756: a nuke execution keeps a given launch tile (MIRV warheads start at the separation point).
    NukeKeepSrcTile4756,
    /// PR 4770: SAM targeting fixes (multi-target scoring, final-tick interception) and a SAM missile that moves on init.
    SamTargetingFixes4770,
    /// PR 4429: nations skip ports on lakes below 3000 water tiles (not on Easy).
    NationSmallLakePorts4429,
    /// PR 4654: trade ship motion plans reuse the stepper's cached path (older builds run a one-shot query).
    TradeShipPathReuse4654,
    /// PR 4577: transport ships only target shores on water reachable from their own shoreline.
    ReachableShoreTransport4577,
    /// PR 4647: per-map tribe name themes and custom tribes.
    TribeNameThemes4647,
    /// PR 4458: pirating needs a port in the warship's water body.
    PirateReachablePort4458,
    /// PR 4657: a spawn intent after the spawn phase is ignored for an already spawned player.
    SpawnTeleportGuard4657,
    /// PR 4426: the cluster recalculation offset is seeded from the player id (older builds: the name).
    ClusterOffsetFromId4426,
    /// PR 4428: random spawn ignores the requested tile in `SpawnExecution`.
    RandomSpawnIgnoresTile4428,
    /// PR 4422: `nearby()` skips unowned tiles with fallout.
    NearbySkipsFallout4422,
    /// PR 4406: the longest railroad is 1.4142 times the station range (older: the range itself).
    RailroadMaxSizeDiagonal4406,
    /// PR 4337: railroads longer than `railroadMaxSize` are not built.
    RailroadMaxSizeLimit4337,
    /// PR 4320: `unitsConstructed` does not count buildings under construction twice.
    UnitsConstructedOnce4320,
    /// PR 4297: nation land and boat attacks share one troop calculation.
    AiBoatTroopsUnified4297,
    /// PR 4469: the Doomsday Clock game mode exists (later builds only).
    DoomsdayClock4469,
    /// PR 4893: Doomsday Clock territory rot, decaying troop floor, 7-wave 35 percent ladder and idle-on-winner.
    DoomsdayRotLadder4893,
    /// PR 4479: water components use a wide land marker and the HPA node search walks water tiles.
    WaterComponentMarkers4479,
    /// PR 4495: `neighbors4` and `forEachNeighbor` visit N, S, W, E (older builds W, E, N, S).
    NeighborOrderNswe4495,
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
        class: "C083",
        add: &[
            Change::NationAiSplit6087,
            Change::AttackInactiveBeforeRetreat6089,
            Change::WaterNukeMiniTile6016,
        ],
        remove: &[],
    },
    Edge {
        class: "C084",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C085",
        add: &[Change::TutorialMode6204],
        remove: &[],
    },
    Edge {
        class: "C086",
        add: &[Change::TutorialSilo6242],
        remove: &[],
    },
    Edge {
        class: "C087",
        add: &[Change::TeamFriendGroups6259],
        remove: &[],
    },
    Edge {
        class: "C088",
        add: &[
            Change::RailNetworkHash36,
            Change::BfsFifoOrder36,
            Change::PlayerClusterRewrite36,
            Change::PortSpawnFirstShore36,
        ],
        remove: &[],
    },
    Edge {
        class: "C089",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C090",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C013",
        add: &[],
        remove: &[
            Change::DetMath5191,
            Change::AttackLogicRework5173,
            Change::AttackTroopsDeducted5111,
            Change::AnnexConvergeLoop4890,
            Change::AnnexEnclosedOnly5027,
            Change::DonateGoldKeepNull4892,
            Change::DoomsdayIdleOnWinner4953,
            Change::SpawnIntentGate4867,
            Change::SpawnRelaxMinDist4921,
            Change::SpawnRollback5321,
            Change::SpawnPhase20s5347,
            Change::TrainSplitResolve5200,
            Change::TradeTrainSaturation5302,
            Change::WinThresholdSame5223,
            Change::AllianceCancelMirv5054,
            Change::MirvNormalize5188,
            Change::SamTargeting5197,
            Change::AftergameSingleplayer5332,
            Change::NationBetray5330,
            Change::NationJuicy5294,
            Change::NationExpansionCap5290,
            Change::NationCrowdedFirst5344,
            Change::NationVictoryDenialPercent5223,
            Change::TeamAssignDuos4758,
            Change::RailNoImpassable4903,
            Change::WaterNukeMinimap4975,
        ],
    },
    Edge {
        class: "C014",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C015",
        add: &[],
        remove: &[Change::StationClusterSame5096],
    },
    Edge {
        class: "C016",
        add: &[],
        remove: &[Change::Overtime5062],
    },
    Edge {
        class: "C017",
        add: &[],
        remove: &[
            Change::DoomsdayTeamLevels5030,
            Change::DoomsdayRotWasteland5052,
        ],
    },
    Edge {
        class: "C018",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C019",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C020",
        add: &[],
        remove: &[Change::BulkPurchase4640],
    },
    Edge {
        class: "C021",
        add: &[],
        remove: &[
            Change::DoomsdayRotLadder4893,
            Change::DoomsdayIdleOnWinner4953,
        ],
    },
    Edge {
        class: "C022",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C023",
        add: &[],
        remove: &[
            Change::ImpassableTerrain4340,
            Change::NeighborOrderNswe4495,
            Change::WarshipVeterancy4433,
            Change::MirvWarheadsStandard4706,
            Change::MirvSiloCooldown4707,
            Change::NukeKeepSrcTile4756,
            Change::SamTargetingFixes4770,
            Change::NationSmallLakePorts4429,
            Change::ReachableShoreTransport4577,
            Change::TribeNameThemes4647,
            Change::PirateReachablePort4458,
            Change::WaterComponentMarkers4479,
            Change::TradeShipPathReuse4654,
        ],
    },
    Edge {
        class: "C024",
        add: &[],
        remove: &[Change::SpawnTeleportGuard4657],
    },
    Edge {
        class: "C025",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C026",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C027",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C028",
        add: &[],
        remove: &[Change::DoomsdayClock4469],
    },
    Edge {
        class: "C029",
        add: &[],
        remove: &[
            Change::ClusterOffsetFromId4426,
            Change::RandomSpawnIgnoresTile4428,
            Change::NearbySkipsFallout4422,
        ],
    },
    Edge {
        class: "C030",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C031",
        add: &[],
        remove: &[Change::RailroadMaxSizeDiagonal4406],
    },
    Edge {
        class: "C032",
        add: &[],
        remove: &[Change::RailroadMaxSizeLimit4337],
    },
    Edge {
        class: "C033",
        add: &[],
        remove: &[],
    },
    Edge {
        class: "C034",
        add: &[],
        remove: &[Change::UnitsConstructedOnce4320],
    },
    Edge {
        class: "C035",
        add: &[],
        remove: &[Change::AiBoatTroopsUnified4297],
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
    Change::AttackLogicRework5173,
    Change::AttackTroopsDeducted5111,
    Change::AnnexConvergeLoop4890,
    Change::AnnexEnclosedOnly5027,
    Change::DonateGoldKeepNull4892,
    Change::DoomsdayIdleOnWinner4953,
    Change::SpawnIntentGate4867,
    Change::SpawnRelaxMinDist4921,
    Change::SpawnRollback5321,
    Change::SpawnPhase20s5347,
    Change::TrainSplitResolve5200,
    Change::TradeTrainSaturation5302,
    Change::WinThresholdSame5223,
    Change::AllianceCancelMirv5054,
    Change::MirvNormalize5188,
    Change::SamTargeting5197,
    Change::AftergameSingleplayer5332,
    Change::NationBetray5330,
    Change::NationJuicy5294,
    Change::NationExpansionCap5290,
    Change::NationCrowdedFirst5344,
    Change::NationVictoryDenialPercent5223,
    Change::TeamAssignDuos4758,
    Change::RailNoImpassable4903,
    Change::WaterNukeMinimap4975,
    Change::StationClusterSame5096,
    Change::Overtime5062,
    Change::BulkPurchase4640,
    Change::DoomsdayTeamLevels5030,
    Change::DoomsdayRotWasteland5052,
    Change::ImpassableTerrain4340,
    Change::NeighborOrderNswe4495,
    Change::WarshipVeterancy4433,
    Change::MirvWarheadsStandard4706,
    Change::MirvSiloCooldown4707,
    Change::NukeKeepSrcTile4756,
    Change::SamTargetingFixes4770,
    Change::NationSmallLakePorts4429,
    Change::ReachableShoreTransport4577,
    Change::TribeNameThemes4647,
    Change::PirateReachablePort4458,
    Change::SpawnTeleportGuard4657,
    Change::ClusterOffsetFromId4426,
    Change::RandomSpawnIgnoresTile4428,
    Change::NearbySkipsFallout4422,
    Change::RailroadMaxSizeDiagonal4406,
    Change::RailroadMaxSizeLimit4337,
    Change::UnitsConstructedOnce4320,
    Change::AiBoatTroopsUnified4297,
    Change::DoomsdayClock4469,
    Change::DoomsdayRotLadder4893,
    Change::WaterComponentMarkers4479,
    Change::TradeShipPathReuse4654,
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
        toggle_for_debugging(&mut changes);
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

/// `OFSIM_TOGGLE=Name,Name` flips the named changes of every rule set (bisecting a failing record).
fn toggle_for_debugging(changes: &mut BTreeSet<Change>) {
    let Ok(names) = std::env::var("OFSIM_TOGGLE") else {
        return;
    };
    let all: Vec<Change> = ROOT_CHANGES.to_vec();
    for name in names.split(',') {
        if let Some(&change) = all.iter().find(|c| format!("{c:?}") == name.trim())
            && !changes.remove(&change)
        {
            changes.insert(change);
        }
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
        assert!(!is_ported("C036"));
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

    #[test]
    fn era_f_classes_are_ported_without_detmath() {
        for n in 13..=22 {
            assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
        }
        assert!(!is_ported("C080"));
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        let c013 = rules("8b45be57542f");
        assert!(!c013.has(Change::DetMath5191) && c013.has(Change::Sfc32PseudoRandom4233));
        assert!(!c013.has(Change::AttackLogicRework5173) && !c013.has(Change::SpawnPhase20s5347));
        assert!(c013.has(Change::StationClusterSame5096) && c013.has(Change::Overtime5062));
        assert!(!rules("0cb90ccb7478").has(Change::StationClusterSame5096));
        assert!(!rules("2d5baafdd0cc").has(Change::Overtime5062));
        assert!(!rules("e9e10703e818").has(Change::BulkPurchase4640));
        assert!(rules("20c813f06a40").has(Change::BulkPurchase4640));
    }

    #[test]
    fn v35_classes_are_ported_up_to_c087() {
        for n in 81..=87 {
            assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
        }
        assert!(!is_ported("C088") && !is_ported("C089"));
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        let c081 = rules("dfd420fd7836");
        assert!(c081.has(Change::NationsLikeHumans5899) && c081.has(Change::SeveredLandAnnex5955));
        assert!(!c081.has(Change::NationAiSplit6087));
        let c083 = rules("a2d20f8e5f32");
        assert!(c083.has(Change::NationAiSplit6087) && c083.has(Change::NationsLikeHumans5899));
        assert!(!rules("4e837520e886").has(Change::NationsLikeHumans5899));
        assert!(rules("7ee96860c717").has(Change::TutorialSilo6242));
        assert!(!rules("c144d134d150").has(Change::TutorialSilo6242));
        assert!(rules("80bff198b68f").has(Change::TeamFriendGroups6259));
    }

    #[test]
    fn era_f_tail_classes_remove_their_changes() {
        for n in 23..=35 {
            assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
        }
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        let c022 = rules("7a7ca5be8ff8");
        assert!(
            c022.has(Change::ImpassableTerrain4340) && !c022.has(Change::DoomsdayRotLadder4893)
        );
        let c023 = rules("dd1277e245b5");
        assert!(
            !c023.has(Change::ImpassableTerrain4340) && !c023.has(Change::NeighborOrderNswe4495)
        );
        assert!(
            !c023.has(Change::SamTargetingFixes4770) && !c023.has(Change::WarshipVeterancy4433)
        );
        assert!(c023.has(Change::SpawnTeleportGuard4657));
        assert!(!rules("b716adb7e2f1").has(Change::SpawnTeleportGuard4657));
        assert!(!rules("580460c9692a").has(Change::DoomsdayClock4469));
        assert!(rules("53e1a5b03e35").has(Change::DoomsdayClock4469));
        let c029 = rules("0c4c7d7993c9");
        assert!(
            !c029.has(Change::ClusterOffsetFromId4426) && !c029.has(Change::NearbySkipsFallout4422)
        );
        assert!(rules("d7b781f155dc").has(Change::RailroadMaxSizeLimit4337));
        assert!(!rules("d7b781f155dc").has(Change::RailroadMaxSizeDiagonal4406));
        assert!(!rules("bb2f2f01ee88").has(Change::RailroadMaxSizeLimit4337));
        assert!(rules("661d96ba280f").has(Change::RailroadMaxSizeLimit4337));
        assert!(!rules("305534cc6579").has(Change::UnitsConstructedOnce4320));
        assert!(!rules("5be72db060fc").has(Change::AiBoatTroopsUnified4297));
        assert!(!is_ported("C036"));
    }
}
