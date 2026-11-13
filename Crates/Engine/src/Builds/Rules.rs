// SPDX-License-Identifier: AGPL-3.0-only
//! Named upstream changes a build contains, and the commit to rule-set mapping.
//!
//! `BuildClasses.toml` (generated, checked in) gives commit -> class and class -> parent; the
//! `Edges/<class>.toml` (one file per ported class, embedded by `build.rs`) says which `Change`s the
//! class adds to or removes from its parent.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;
use thiserror::Error;

/// Declares `Change` and `Change::ALL` from one variant list, so flag names in `Edges/*.toml` resolve.
macro_rules! change_enum {
    ($(#[$emeta:meta])* pub enum Change { $($(#[$meta:meta])* $name:ident,)* }) => {
        $(#[$emeta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub enum Change {
            $($(#[$meta])* $name,)*
        }

        impl Change {
            /// Every change, in declaration order.
            pub const ALL: &'static [Change] = &[$(Change::$name,)*];
        }
    };
}

change_enum! {
/// One upstream behaviour change, named after the upstream PR; more are added per build.
///
/// Engine code branches on [`Rules::has`] at the exact place upstream changed; `has` is true
/// when the build contains the changed (newer) behaviour.
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
    /// PR 4979: one HPA finder swaps its graph on rebuild (older builds and v0.35.0 on create a new finder).
    WaterHpaSharedFinder4979,
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
    /// v0.36.0: nations book troops they send this tick so later decisions see what is left at home.
    NationHomeTroops36,
    /// v0.36.0: a nation without a land front attacks the nearest bots across the water.
    NationOverseasBots36,
    /// v0.36.0: the water graph edge search is bounded by the squared cluster size.
    AbstractGraphWideSearch36,
    /// v0.36.0: a train spawned off the rail starts one tile behind its first rail tile.
    TrainSpawnOffRail36,
    /// v0.36.0: MIRV warhead targets come from a clipped box with a spatial overlap grid and a saturation limit.
    MirvTargetGrid36,
    /// v0.36.0: tribe themes and custom tribes of the maps added or edited since the baseline.
    TribeMapData36,
    /// v0.36.1: tribe themes of four more maps (Ottoman Empire, Turkiye, World Inverted, Zealandia).
    TribeMapData361,
    /// v0.36.0: nation warships keep nuke gold, invasions may dip into it, and the enemy fleet must outnumber ours.
    NationWarshipSpareGold36,
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
    /// PR 4239 and 4278: Hard and Impossible nations cap the troops they send and skip attacks under 20 percent of the target's troops.
    AiTroopCap4239,
    /// PR 4282: nations are not blocked by PVP immunity (older: they respect it like humans).
    NationsIgnoreImmunity4282,
    /// Meta update: atom and hydrogen bombs fly at speed 10 (older: 8).
    NukeSpeed10Meta,
    /// Meta update: the trade ship spawn sigmoid midpoint is 400 (older: 200).
    TradeShipSigmoid400Meta,
    /// PR 3199: the spawn phase ends through `SpawnTimerExecution` / `endSpawnPhase` (older: while `ticks <= numSpawnPhaseTurns`).
    SpawnPhaseTimer3199,
    /// PR 4117: nations track their queued spawn and are created before the tribes (older: they retry every attack tick, after the tribes).
    NationSpawnRetry4117,
    /// PR 1563 and 4016: a defense post on captured tiles is destroyed (older: its level drops and it changes hands).
    DefensePostDestroyed1563,
    /// PR 4014: self attacks, self donations and `isFriendly(self)` are guarded.
    SelfActionGuards4014,
    /// PR 3997: a missile silo execution ends once its silo is gone.
    SiloInactiveGuard3997,
    /// PR 4151: a move-warship intent skips warships outside the new patrol tile's water body.
    WarshipPatrolComponent4151,
    /// PR 3820: attacks walk neighbours with `forEachNeighbor` (older: `neighbors()` in N, S, W, E order).
    AttackForEachNeighbor3820,
    /// PR 4079: train stations reach 110 tiles and railroads follow the station range (older: 100 and 120).
    TrainRange110Pr4079,
    /// PR 4149: the abstract graph A* uses a min-heap (older: a bucket queue that clamps priorities).
    AbstractAStarMinHeap4149,
    /// PR 4495: `neighbors4` and `forEachNeighbor` visit N, S, W, E (older builds W, E, N, S).
    NeighborOrderNswe4495,
    /// v0.31: PseudoRandom.nextInt floors its bounds (older: it does not).
    NextIntFloorsArgs31,
    /// PR 2046: `shuffleArray` stops at index 1 (older builds also draw once for index 0).
    ShuffleArrayStopsAtOne2046,
    /// v0.31: SAM cooldown 90 ticks (older 120, before v0.30.0-beta5 75).
    SamCooldown90V31,
    /// v0.30.0-beta5: SAM cooldown 120 ticks (older 75).
    SamCooldown120V30,
    /// v0.31: missile silo cooldown 90 ticks (older 75).
    SiloCooldown90V31,
    /// v0.31: port construction takes 50 ticks (older 20).
    PortBuild50V31,
    /// v0.31: attacks on bots lose 30 percent magnitude (older 20 percent).
    BotAttackMalus07V31,
    /// v0.31: attacker troop loss blends 0.6 current and 0.4 alternative (older 0.7 and 0.3).
    AttackerLoss6040V31,
    /// v0.30.0-beta5: attacker troop loss blends 0.7 and 0.3 (older 0.5 and 0.5).
    AttackerLoss7030V30,
    /// v0.31: bots grow troops at half rate (older 0.6).
    BotTroopGrowth05V31,
    /// v0.31: atom and hydrogen bombs fly at speed 8 (older 6).
    NukeSpeed8V31,
    /// v0.31: conquering a human takes half its gold (older: all of it).
    ConquerGoldHalfHuman31,
    /// v0.31: cities under construction add no troop cap.
    CityTroopsFinishedOnly31,
    /// v0.31: an attack skips conquered tiles that are not land.
    AttackSkipsWater31,
    /// v0.31: a troop donation with nothing to give ends at init.
    DonateTroopZeroInactive31,
    /// v0.31: nation attack choice reports whether it attacked, tries the next option, scans lakes and nearby players (older: first match ends the turn, ocean shores only).
    AiAttackRework31,
    /// v0.31: an alliance cancels nukes whose blast would hit the new ally (older: only nukes aimed at its tiles).
    AllianceNukeCancelBlast31,
    /// v0.31: trade routes, captured trade ships, warship and port spawns respect water components and lakes (older: any port, ocean shores).
    TradeWaterComponents31,
    /// v0.31: a SAM that reloads may fire in the same tick (older: the reload ends its tick).
    SamReloadThenFire31,
    /// v0.31: Easy nations react slower (65 to 100), team games embargo every outsider on Hard, ship tracking needs a port (older: none of these).
    NationExecutionTweaks31,
    /// v0.31: incomingAttacks() leaves out attacks of dead attackers.
    IncomingAttacksAliveOnly31,
    /// v0.31: nation structures (paced placement, defense posts on attack fronts, shared water ports, save-up changes) replace the plain build order with defense posts.
    NationStructureRework31,
    /// v0.31: nations check silos before choosing a nuke target, hunt dense targets, upgrade helpful silos and save for a MIRV differently in team games.
    NationNukeRework31,
    /// v0.31: nation warships respond to incoming transports, move patrols and spawn on any water (older: ocean only, no patrol moves).
    NationWarshipRework31,
    /// v0.31: MIRV steamroll thresholds (gap 2, 1.5, 1.25; Easy needs 20 cities) and easy nations never betray humans (older: 1.5, 1.3, 1.2; 15 cities; they may).
    NationBehaviorTweaks31,
    /// v0.31: the generated nation names draw from the larger name tables and special plurals (older: 114 templates and 66 nouns).
    NationNamesExpanded31,
    /// v0.31: warships heal in ports, retreat to dock, fight enemy warships and keep their patrol after a lost path (older: the plain v0.30 warship).
    WarshipDocking31,
    /// v0.31: the clan tag comes from the clanTag field of a player (older: parsed from the [TAG] in the username).
    ClanTagField31,
    /// v0.30.10 (PR 3455): a surrounded cluster is judged against the box of the enemy tiles around it (older: the enemy border box).
    ClusterLocalEnemyBox3455,
    /// v0.30.10 (PR 3448): players under 100 tiles recompute clusters every tick and a change on the last calc tick counts.
    ClusterCalcSmallPlayers3448,
    /// v0.30.10 (PR 3437): nations do not donate troops in public games.
    NoPublicDonation3437,
    /// v0.30.10 (PR 3433): a random-spawn game has a 150 turn spawn phase (older: 300).
    RandomSpawnShortPhase3433,
    /// v0.30.0-beta5 (PR 3410): after a win teammates may nuke each other and SAMs shoot teammate nukes (older: never).
    AftergameTeammateNukes3410,
    /// v0.30.2: train spawn rate 15 and 9 free cities, trade ship sigmoid scale 75000 (older: 18, 5 cities, 50000).
    TrainTradeRetune30,
    /// v0.30.0-beta5: train gold falls with the cities visited and trains spawn at 18 (older: flat gold, rate 25).
    TrainDistanceGold30Beta,
    /// v0.31: nations stop alliance behaviour when alliances are disabled (older: they only fail to send requests).
    NationDisabledAlliances31,
    /// v0.30.0-beta5: the disableAlliances game option exists (older: alliances cannot be disabled).
    DisableAlliancesOption30,
    /// v0.30.22 only: one nuke in six bounces past its target and detonates half the distance beyond.
    BouncingNukes30,
    /// v0.30.0-beta2 only: trains spawn at (factories + 10) * 25 (beta1 and beta5: 18).
    TrainSpawnRate25Beta2,
    /// v0.30.0-beta2: a game ends at 170 minutes (beta1: no hard limit).
    HardTimeLimit170Min30,
    /// v0.30.0-beta2: nations reject alliance requests made during the spawn phase (beta1: they judge them).
    AllianceSpawnReject30,
    /// v0.30.0-beta2: bots are spawned from their own seed (simpleHash + 2) (beta1: the game runner seed).
    BotSpawnerOwnSeed30,
    /// v0.30.0-beta2: nations follow the nations setting (count, default, disabled) (beta1: disableNations flag, compact maps everywhere).
    NationCreationConfigurable30,
    /// v0.31: a transport ship order rejects the target's pending alliance request (older: only land attacks do).
    TransportRejectsAlliances31,
    /// PR 3227: attacker troop loss blends the current loss with an alternative (older: the plain current loss).
    AttackerLossBlend3227,
    /// PR 3184: the Medium/Hard nation start troops, troop cap and growth tables moved one tier down.
    NationDifficultyRebalance3184,
    /// PR 3222: nations respect spawn immunity and the immunity test is strict (older: only humans, inclusive).
    NationSpawnImmunity3222,
    /// PR 3232: trade ship gold has no port bonus and spawning uses a rejection pity timer (older: port count bonus and per-player rates).
    TradeShipMeta3232,
    /// PR 3020: conquering a human that never attacked yields no gold.
    ConquerGoldSkipInactive3020,
    /// PR 3206: a MIRV also lowers the launcher's relation to its target.
    MirvRelationBoth3206,
    /// PR 2954: an attack does not hand a captured tile to a neighbour allied with the target.
    AttackFriendlyNeighbor2954,
    /// PR 3176: nuke explosions deal troop losses per player in bulk, unit and attack troops clamp at zero.
    NukeExplosionBulk3176,
    /// PR 2895: bots delete their structures, nations hit structure-owning bots first.
    BotStructureFarming2895,
    /// PR 3168: a dead player silently drops all alliances.
    AllianceRemovedOnDeath3168,
    /// PR 3155: accepting an alliance cancels nukes between the allies; the reply intent is reject only.
    AllianceRadialAccept3155,
    /// PR 3092: a player with an incoming request may always answer it with a request of its own.
    AllianceIncomingBypass3092,
    /// PR 3105: team games forbid nukes that would hit a teammate's structures.
    NukeTeammateStructureGuard3105,
    /// PR 2912: a nuke angers every owner of a structure in range, and incoming requests of angered players are rejected.
    NukeBreakAllyList2912,
    /// PR 3003: railroads split where a new structure overlaps them, clusters are recomputed lazily.
    RailSnapOverlap3003,
    /// PR 3130: a station picks its train destination by reservoir sampling over cached trade stations.
    TrainDestinationReservoir3130,
    /// PR 3223: SAM targeting picks one best target and flags nukes already targeted.
    SamTargetingFix3223,
    /// PR 3009 and 3278: random spawn needs a fully valid spawn area and a respawn frees old tiles first.
    SpawnValidation3009,
    /// PR 3122: usernames arrive censored from the server (older: the game runner censors them).
    ServerUsernameCensoring3122,
    /// v0.30.0-beta1: tribe names come from the revised `TribeNames` pools (older: the `BotNames.ts` lists, 180 prefixes and 71 suffixes).
    TribeNamesRevised30,
    /// PR 3124: border tile updates walk `forEachNeighbor` (older: `neighbors()` in N, S, W, E order).
    BorderUpdateForEach3124,
    /// PR 3286: a retreating transport ship returns to the closest owned shore (older: to its launch tile).
    TransportRetreatNearest3286,
    /// PR 3167: a nuke blast does not destroy SAM missiles.
    SamMissileImmunity3167,
    /// PR 3072: Easy nations start with 12,500 troops, cap at half and grow at 0.9 (older: 18,750, three quarters and 0.95).
    EasyNationsEasier3072,
    /// PR 2984: nations donate troops in team games, check the boat limit and tile share for winners (older: none of these).
    NationAiRework2984,
    /// PR 2971: nations reject team-game alliances 25, 50, 75 and 100 percent of the time (older: 0, 20, 40, 60).
    NationTeamRejectHarder2971,
    /// PR 2959: the boat intent carries no target or source tile, smoothing keeps two magnitude passes, the short path search covers 260 tiles.
    PathfindingRefinements2959,
    /// PR 2937: shore endpoints coerce to the best connected water neighbour (older: the first one).
    ShoreCoerceBestNeighbor2937,
    /// PR 2958: with random spawn a player cannot re-roll its spawn tile.
    RandomSpawnLock2958,
    /// PR 2950: station ids start at 1 and the station graph allows 32 neighbours (older: 0 and 8).
    RailPathfindingFix2950,
    /// PR 3134: MIRV warheads separate even when the launcher died meanwhile.
    MirvWarheadsWhenDead3134,
    /// PR 2932: naval invasions search with a short bounded path, MinHeap queues, a final smoothing pass and a refined launch tile (older: none of these).
    NavalInvasionPathing2932,
    /// PR 2915: lobbies can set the starting gold and a gold multiplier.
    LobbyGoldOptions2915,
    /// PR 2896: a bot expands into free land only when a land tile next to its border is unowned (older: any border tile next to terra nullius, water included).
    BotTerraNulliusLand2896,
    /// PR 2878: a nuke also angers allies owning a structure in its blast (older: only the weighted tile count).
    NukeBreakAlliedStructures2878,
    /// PR 3263: team games have spawn areas; disabling transport ships also zeroes the boat limit.
    TeamSpawnAreas3263,
    /// PR 3152: nations use the structure behaviour class (older: an inline build order).
    NationStructureBehavior3152,
    /// PR 3246 and 3206: later nation AI behaviours (SAM overwhelm, home defence, greetings, MIRV cooldown, alliance limits).
    NationBehaviorUpdate3246,
    /// v0.28.4: nations are `FakeHumanExecution` players placed on their manifest cell and tribes use `BotBehavior` (newer: `NationExecution`, `AiAttackBehavior`).
    FakeHumanNations28,
    /// v0.26.14: nations and tribes use the pre-rewrite `BotBehavior` (alliance answers inside it, no MIRV or retaliation AI, plain land attacks, older boat targets and attack chances; newer: `AllianceBehavior` split and the v0.28 AI).
    PreRewriteNationAi26,
    /// v0.28.4: nation start troops, troop cap and growth follow the v0.28 difficulty table.
    NationDifficultyEconomy28,
    /// v0.28.4: immunity is the spawn phase plus a fixed 5 seconds for everyone (newer: humans only, `canAttackPlayer`).
    ImmunityByConfig28,
    /// v0.28.4: spawn tiles are chosen when the executions are created (`SpawnExecution` always gets a tile; bots retry random land).
    SpawnByTile28,
    /// v0.28.4: boats, rails and shipping run on the bidirectional `MiniAStar` (newer: HPA water graph, stepper finders).
    LegacyAStar28,
    /// v0.28.4: shells and SAM missiles use one PRNG per finder, nukes and MIRVs a control-point Bezier; MIRVs scatter by `nextInt`.
    LegacyProjectiles28,
    /// v0.28.4: relation penalties of attacks, betrayals and insults, and `allRelationsSorted` keeps dead players.
    RelationPenalties28,
    /// v0.28.4: warships treat friendly owners as untargetable, patrol anywhere and drop a lost target tile.
    WarshipLegacy28,
    /// v0.28.5 and v0.28.7 (aac3bb37e): trade ship distance bonus 100 -> 150 gold per tile (before PR 3232 formula).
    TradeShipDistanceGold150,
    /// PRs 2552 and 2609 (v0.28.0 to v0.28.3, reverted in v0.28.4): bots beyond the first 20 take names from the community lists.
    CommunityBotNames2552,
    /// PR 2697 (v0.28.0 and v0.28.1, reverted in v0.28.4): bots may attack humans, and nothing attacks humans on Easy via random targets.
    BotsAttackHumans2697,
    /// v0.27.10: nation start troops 2,500 / 5,000 / 20,000 / 50,000, cap 0.5 / 1 / 1.5 / 2 and growth 0.9 / 1 / 1.1 / 1.2 by difficulty.
    NationDifficultyEconomy27,
    /// PR 2539 and 2614: cluster removal finds the largest cluster by a linear scan over DFS clusters (older: a stable size sort over BFS clusters).
    ClusterLinearScan2614,
    /// v0.28: tribes and fake humans pick attacks statelessly and fake humans answer alliances in `AllianceBehavior` (older: a stateful enemy in `BotBehavior`, alliance replies there).
    BotBehaviorStateless28,
    /// PR 2378: no `Construction` unit; a structure is built at once and flagged under construction (older: a `Construction` unit stands in until the structure is built).
    NoConstructionUnit2378,
    /// PR 2665: SAM launchers always hit (older: a per-launcher PRNG draw against the hit chance, 0.8 and 0.5 for MIRV warheads).
    SamNoRng2665,
    /// v0.27: a SAM's range grows with its level and detection scales with the maximum (v0.26.14: one fixed range of 70).
    SamRangeByLevel27,
    /// v0.27: a retreating transport ship loses 25% of its troops (v0.26.14: none).
    TransportRetreatMalus27,
    /// v0.27: a retreating attack with a source tile keeps the troop correction only when it has none (v0.26.14: always corrects).
    AttackRetreatSourceTile27,
    /// v0.27: `maxTimerValue` ends a game with the leader as winner (v0.26.14: no timer win).
    WinMaxTimer27,
    /// v0.26.14: `PathFinder.nextTile` keeps its path when the goal is within range (v0.27 clears it).
    PathFinderKeepsPath26,
    /// PR 2203: a disconnected player's ships follow a teammate who conquers them (transport takeover and retreat source check, warships and transports captured in `conquerPlayer`).
    DisconnectedShipHandover2203,
    /// v0.26.14: a retreating attack that never removed its start troops subtracts them first (v0.26.15: it does not).
    AttackRetreatStartTroops26,
    /// v0.26.14: warships treat a disconnected player as a friend when picking targets (v0.26.15: they shoot them).
    WarshipAfkFriendly2203,
    /// v0.26.15: a warship of a disconnected player is deleted.
    WarshipDeleteDisconnected26,
    /// v0.26.15: a deleted unit is marked and removed after `deletionMarkDuration`; the delete cooldown is 30 s (v0.26.13: deleted at once, cooldown 5 s).
    UnitDeletionMark26,
    /// v0.26.15: trade routes near a port get no bonus within `tradeShipShortRangeDebuff` (300) and the trade gold sigmoid uses it (v0.26.13: no debuff, sigmoid centre 200).
    TradeShortRangeDebuff26,
    /// v0.26.15: upgrading needs a valid unit (not under construction, not marked, owned) (v0.26.13: type, funds and the `upgradable` flag only).
    UpgradeUnitValidity26,
    /// v0.26.11: train spawn rate `(factories + 10) * 20` (v0.26.13: 16).
    TrainSpawnRate20V26,
    /// v0.26.9: the clan is a leading `[TAG]` of 2 to 5 letters in the written case (v0.26.11: the first `[TAG]` of letters or digits, upper-cased).
    ClanTagLeading26,
    /// v0.26.11: trade ship gold is the flat `floor(100000 + 100 * dist)` (v0.26.13: the sigmoid).
    TradeShipFlatGold26,
    /// PR 2621: a MIRV costs 25M plus 15M per MIRV launched (older: a flat 35M).
    MirvCostScales2621,
    /// Meta change: a train between allies pays 35,000 gold (older: 50,000).
    TrainAllyGold35kMeta,
    /// v0.28: an alliance reply is answered in `init`, at the end of the tick it was added (older: on its first tick).
    AllianceReplyInit28,
    /// v0.27.10: a nuke with a source tile launches from it without checking `canBuild` (MIRV warheads start at their MIRV).
    NukeSrcFirst27,
    /// PR 2622: a player keeps the name it was given (older: `sanitizeUsername` strips other characters and pads to 3).
    NoUsernameSanitation2622,
    /// v0.27 and v0.28: the game runner returns a good name unchanged (the older `censorNameWithClanTag` rebuilt `[TAG] name`).
    CensorKeepsGoodNames27,
    /// v0.27.5 to v0.27.8: trains spawn at (factories + 10) * 16 (v0.27.10: 18).
    TrainSpawnRate16V27_5,
    /// PR 2522: the emoji table has a factory and train row and another order (v0.27.0 and v0.27.1: 55 emojis).
    EmojiTableFactoryRow2522,
    /// v0.27.0 and v0.27.1: the game runner replaces a profane name whole after `sanitize` (v0.27.2 and later: `censorNameWithClanTag`).
    SanitizeThenFixProfane27_0,
}
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

impl Change {
    /// The change called `name` (its variant name), or None.
    pub fn from_name(name: &str) -> Option<Change> {
        Change::ALL
            .iter()
            .copied()
            .find(|c| format!("{c:?}") == name)
    }
}

/// Changes a class adds to / removes from its parent class (one `Edges/<class>.toml`).
struct Edge {
    add: Vec<Change>,
    remove: Vec<Change>,
}

// `EDGE_FILES: &[(&str, &str)]` of (class id, file text), generated from `Edges/*.toml`.
include!(concat!(env!("OUT_DIR"), "/EdgeFiles.rs"));

/// Parses one edge file; an unknown flag name panics (covered by a unit test over all files).
fn parse_edge(class: &str, text: &str) -> Edge {
    let value: toml::Value = text
        .parse()
        .unwrap_or_else(|e| panic!("Edges/{class}.toml: {e}"));
    let list = |key: &str| -> Vec<Change> {
        value[key]
            .as_array()
            .unwrap_or_else(|| panic!("Edges/{class}.toml: `{key}` array"))
            .iter()
            .map(|v| {
                let name = v.as_str().expect("flag name");
                Change::from_name(name)
                    .unwrap_or_else(|| panic!("Edges/{class}.toml: unknown change {name}"))
            })
            .collect()
    };
    Edge {
        add: list("add"),
        remove: list("remove"),
    }
}

/// The edge of a ported class, parsed once.
fn edge_of(class: &str) -> Option<&'static Edge> {
    static EDGES: OnceLock<BTreeMap<&'static str, Edge>> = OnceLock::new();
    EDGES
        .get_or_init(|| {
            EDGE_FILES
                .iter()
                .map(|(class, text)| (*class, parse_edge(class, text)))
                .collect()
        })
        .get(class)
}

/// Changes of the baseline class C000.
const ROOT_CHANGES: &[Change] = &[
    Change::ClusterLinearScan2614,
    Change::BotBehaviorStateless28,
    Change::NoConstructionUnit2378,
    Change::SamNoRng2665,
    Change::SamRangeByLevel27,
    Change::TransportRetreatMalus27,
    Change::AttackRetreatSourceTile27,
    Change::WinMaxTimer27,
    Change::DisconnectedShipHandover2203,
    Change::AttackRetreatStartTroops26,
    Change::UnitDeletionMark26,
    Change::TradeShortRangeDebuff26,
    Change::UpgradeUnitValidity26,
    Change::WarshipAfkFriendly2203,
    Change::MirvCostScales2621,
    Change::TrainAllyGold35kMeta,
    Change::AllianceReplyInit28,
    Change::NoUsernameSanitation2622,
    Change::EmojiTableFactoryRow2522,
    Change::SpawnPhaseTimer3199,
    Change::NationSpawnRetry4117,
    Change::DefensePostDestroyed1563,
    Change::SelfActionGuards4014,
    Change::SiloInactiveGuard3997,
    Change::WarshipPatrolComponent4151,
    Change::AttackForEachNeighbor3820,
    Change::TrainRange110Pr4079,
    Change::AbstractAStarMinHeap4149,
    Change::AiTroopCap4239,
    Change::NationsIgnoreImmunity4282,
    Change::NukeSpeed10Meta,
    Change::TradeShipSigmoid400Meta,
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
    Change::WaterHpaSharedFinder4979,
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
    Change::NextIntFloorsArgs31,
    Change::ShuffleArrayStopsAtOne2046,
    Change::SamCooldown90V31,
    Change::SamCooldown120V30,
    Change::SiloCooldown90V31,
    Change::PortBuild50V31,
    Change::BotAttackMalus07V31,
    Change::AttackerLoss6040V31,
    Change::AttackerLoss7030V30,
    Change::BotTroopGrowth05V31,
    Change::NukeSpeed8V31,
    Change::ConquerGoldHalfHuman31,
    Change::CityTroopsFinishedOnly31,
    Change::AttackSkipsWater31,
    Change::DonateTroopZeroInactive31,
    Change::AiAttackRework31,
    Change::AllianceNukeCancelBlast31,
    Change::TradeWaterComponents31,
    Change::SamReloadThenFire31,
    Change::NationExecutionTweaks31,
    Change::IncomingAttacksAliveOnly31,
    Change::NationStructureRework31,
    Change::NationNukeRework31,
    Change::NationWarshipRework31,
    Change::NationBehaviorTweaks31,
    Change::NationNamesExpanded31,
    Change::WarshipDocking31,
    Change::ClanTagField31,
    Change::ClusterLocalEnemyBox3455,
    Change::ClusterCalcSmallPlayers3448,
    Change::NoPublicDonation3437,
    Change::RandomSpawnShortPhase3433,
    Change::AftergameTeammateNukes3410,
    Change::TrainTradeRetune30,
    Change::TrainDistanceGold30Beta,
    Change::NationDisabledAlliances31,
    Change::DisableAlliancesOption30,
    Change::HardTimeLimit170Min30,
    Change::AllianceSpawnReject30,
    Change::BotSpawnerOwnSeed30,
    Change::NationCreationConfigurable30,
    Change::TransportRejectsAlliances31,
    Change::AttackerLossBlend3227,
    Change::NationDifficultyRebalance3184,
    Change::NationSpawnImmunity3222,
    Change::TradeShipMeta3232,
    Change::ConquerGoldSkipInactive3020,
    Change::MirvRelationBoth3206,
    Change::AttackFriendlyNeighbor2954,
    Change::NukeExplosionBulk3176,
    Change::BotStructureFarming2895,
    Change::AllianceRemovedOnDeath3168,
    Change::AllianceRadialAccept3155,
    Change::AllianceIncomingBypass3092,
    Change::NukeBreakAllyList2912,
    Change::NukeBreakAlliedStructures2878,
    Change::BotTerraNulliusLand2896,
    Change::NavalInvasionPathing2932,
    Change::LobbyGoldOptions2915,
    Change::MirvWarheadsWhenDead3134,
    Change::PathfindingRefinements2959,
    Change::ShoreCoerceBestNeighbor2937,
    Change::RandomSpawnLock2958,
    Change::RailPathfindingFix2950,
    Change::NationTeamRejectHarder2971,
    Change::NationAiRework2984,
    Change::EasyNationsEasier3072,
    Change::SamMissileImmunity3167,
    Change::TransportRetreatNearest3286,
    Change::BorderUpdateForEach3124,
    Change::TribeNamesRevised30,
    Change::ServerUsernameCensoring3122,
    Change::NukeTeammateStructureGuard3105,
    Change::RailSnapOverlap3003,
    Change::TrainDestinationReservoir3130,
    Change::SamTargetingFix3223,
    Change::SpawnValidation3009,
    Change::TeamSpawnAreas3263,
    Change::NationStructureBehavior3152,
    Change::NationBehaviorUpdate3246,
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
    /// Bitset mirror of `changes` for the hot `has` lookups.
    mask: Box<[u64]>,
}

/// Bitset over `Change` discriminants.
fn mask_of(changes: &BTreeSet<Change>) -> Box<[u64]> {
    let mut mask = vec![0u64; Change::ALL.len().div_ceil(64)];
    for &c in changes {
        mask[c as usize / 64] |= 1 << (c as usize % 64);
    }
    mask.into_boxed_slice()
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
        let changes: BTreeSet<Change> = ROOT_CHANGES.iter().copied().collect();
        Self {
            class: "C000".to_string(),
            mask: mask_of(&changes),
            changes,
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
            let edge = edge_of(id)?;
            changes.extend(edge.add.iter().copied());
            for removed in &edge.remove {
                changes.remove(removed);
            }
        }
        toggle_for_debugging(&mut changes);
        Some(Self {
            class: class.to_string(),
            mask: mask_of(&changes),
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
    #[inline]
    pub fn has(&self, change: Change) -> bool {
        let i = change as usize;
        self.mask[i / 64] & (1 << (i % 64)) != 0
    }
}

/// `OFSIM_TOGGLE=Name,Name` flips the named changes of every rule set (bisecting a failing record).
fn toggle_for_debugging(changes: &mut BTreeSet<Change>) {
    let Ok(names) = std::env::var("OFSIM_TOGGLE") else {
        return;
    };
    for name in names.split(',') {
        if let Some(change) = Change::from_name(name.trim())
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
    #[test]
    fn every_edge_file_names_known_changes() {
        for (class, _) in EDGE_FILES {
            assert!(edge_of(class).is_some(), "{class}");
        }
    }

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
        assert!(is_ported("C040"));
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
    fn v36_classes_add_their_changes() {
        assert!(is_ported("C088") && is_ported("C089"));
        let c087 = Rules::for_commit("09aa423bceb9").unwrap();
        assert!(!c087.has(Change::PlayerClusterRewrite36) && !c087.has(Change::RailNetworkHash36));
        let c088 = Rules::for_commit("0211f17b71ef").unwrap();
        assert!(c088.has(Change::PlayerClusterRewrite36) && c088.has(Change::MirvTargetGrid36));
        assert!(!c088.has(Change::TribeMapData361));
        let c089 = Rules::for_commit("bc3e3334c573").unwrap();
        assert!(c089.has(Change::PlayerClusterRewrite36) && c089.has(Change::TribeMapData361));
    }

    #[test]
    fn era_s_head_classes_switch_the_generator_and_drop_meta_changes() {
        for c in ["C036", "C037", "C038", "C039"] {
            assert!(is_ported(c), "{c}");
        }
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        let c036 = rules("26d8a314aec4");
        assert!(!c036.has(Change::Sfc32PseudoRandom4233) && !c036.has(Change::AiTroopCap4239));
        assert!(
            !c036.has(Change::TradeShipSigmoid400Meta) && c036.has(Change::SpawnPhaseTimer3199)
        );
        let c039 = rules("f17ca9bd6551");
        assert!(
            c039.has(Change::TradeShipSigmoid400Meta) && !c039.has(Change::Sfc32PseudoRandom4233)
        );
        let c037 = rules("aeb8d60224e3");
        assert!(
            !c037.has(Change::SpawnPhaseTimer3199) && !c037.has(Change::AttackForEachNeighbor3820)
        );
        assert!(
            !c037.has(Change::TradeShipSigmoid400Meta)
                && !c037.has(Change::AbstractAStarMinHeap4149)
        );
        assert!(!rules("5d6c48ebf23f").has(Change::SpawnPhaseTimer3199));
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
        assert!(is_ported("C040"));
    }

    #[test]
    fn v30_classes_drop_v31_changes_and_their_own_betas() {
        for n in 40..=48 {
            assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
        }
        let rules = |commit: &str| Rules::for_commit(commit).unwrap();
        let c040 = rules("d809c25d1cdd");
        assert!(
            !c040.has(Change::NextIntFloorsArgs31) && !c040.has(Change::NationStructureRework31)
        );
        assert!(!c040.has(Change::WarshipDocking31) && !c040.has(Change::NukeSpeed8V31));
        assert!(c040.has(Change::SamCooldown120V30) && c040.has(Change::TrainTradeRetune30));
        assert!(
            !c040.has(Change::BouncingNukes30)
                && rules("f2507c38979c").has(Change::BouncingNukes30)
        );
        assert!(rules("3e7088cb2a2b").has(Change::ClusterLocalEnemyBox3455));
        assert!(!rules("dd2c239aa166").has(Change::ClusterLocalEnemyBox3455));
        assert!(!rules("5fb7f75f3d5b").has(Change::NoPublicDonation3437));
        assert!(!rules("ebe7f76cd312").has(Change::TrainTradeRetune30));
        assert!(!rules("741a38c62b29").has(Change::AftergameTeammateNukes3410));
        let c047 = rules("df5fad1a1f6d");
        assert!(!c047.has(Change::SamCooldown120V30) && c047.has(Change::TrainSpawnRate25Beta2));
        let c048 = rules("339ace0bd677");
        assert!(
            !c048.has(Change::TrainSpawnRate25Beta2)
                && !c048.has(Change::NationCreationConfigurable30)
        );
    }
}
