# Implementation plan

Japanese version: [Plan.ja.md](Plan.ja.md)

Responsibility of this file: the phases of the port, what each delivers, and its status. Update the status column when a phase lands.

The first milestone is hash-exact replay of the baseline build v0.34.22 (`4e837520e886`). Every later build is a diff against it.

| Phase | Deliverable | Done when | Status |
| --- | --- | --- | --- |
| P0 | Workspace, docs, licence, submodule | `cargo build` passes | done |
| P1a | `Compat`, `Core` (PseudoRandom, DetMath, Util), `GameMap`, `TileSet`, `Configuration` of the baseline | unit tests match node-computed values | done (V8 fdlibm too) |
| P1b | `ofsim-record`, `ofsim-map` | baseline records and maps load; turns expand like `decompressGameRecord` | done |
| P1c | Reference tracer and record fetcher | trace of a baseline record written from the upstream engine | done |
| P2 | Game core: `GameImpl`, `PlayerImpl`, `UnitImpl`, attacks, alliances, `Executor`, `GameRunner`, spawn, tribes, attacks, troop growth, win check | hashes match on baseline records through the end of the spawn phase, then through the first unit build | core landed (game state, spawn, player upkeep, win check, `ofsim verify`/`trace`); attacks, alliances, tribes and nations are `NotPorted`, so records match through the spawn phase of games without nations |
| P3 | Units: construction, structures, ports and trade ships, transport ships, warships, rails and trains, nukes, MIRV, SAM, shells, water nukes, pathfinding | every baseline record without nations passes `ofsim verify` | |
| P4 | Nation AI (`NationExecution` and behaviours, `NationCreation`) | every baseline record passes | `NationExecution`, emoji, alliance, warship behaviours and `NationUtils` landed (inert traces identical, RNG draws identical); structure, nuke and MIRV behaviours (N2) and `AiAttackBehavior` (A2) pending |
| P5 | `ofsim replay` snapshots, `batch`, profiling | output identical to the upstream dumper on sample games; throughput measured | done (byte-identical on 3 records, see [Output.md](Output.md); batch resumable and parallel; throughput in the table below) |
| P6 | Build rules for every build from 2025-10-01 (v0.25.21) to the newest, newest first along our data, then backwards | each build passes on sampled records | in progress: framework (`Builds/Rules.rs`, `Builds/Maps.rs`) and era D classes C001 to C012 done (see below); C013 onward pending; C081 to C089 (monorepo layout, v0.35.x and v0.36.x) done |

## Throughput (P5)

Single thread, release build, `ofsim verify` on a 4-core machine shared with other jobs (load about 3); upstream is `ReplayDumper.ts` on the baseline worktree (Node 22, `--every 1000000`) on the same machine. Hot spots now are naval path finding (hierarchical A*), territory clusters (`PlayerExecution`) and rail clusters; they are the ported algorithms themselves. The 10x target was not reached: about 3.3x to 3.7x.

| Record | Ticks | Rust s | Rust ticks/s | Upstream ticks/s | Speed-up |
| --- | --- | --- | --- | --- | --- |
| cQtf3wZ8VA | 8,503 | 34.1 | 250 | 75 (114 s) | 3.3x |
| caYteuCaea | 29,013 | 35.4 | 820 | 226 (128 s) | 3.6x |
| Q94sNurv | 58,511 | 34.6 | 1,690 | 450 (130 s, 100-tick snapshots) | 3.8x |

Speed changes to engine code (behaviour-preserving, 40 of 41 baseline records still pass, `c6dYiNx281` still stops at 3810): the rail A* reuses its stamped buffers between queries instead of allocating 10 MB per query; `RailNetwork::find_station` uses a per-unit index.

## P6 status by class

Class ids, parents and builds are in [BuildPlan.md](BuildPlan.md) (status column) and `Crates/Engine/src/Builds/BuildClasses.toml`. A class is supported when `Rules.rs` has an edge for it and its ancestors; every other class fails with `build <commit> (class Cnnn) not ported yet`. "Pass" counts sampled records (`Cache/Records/<commit12>/`, up to 12,900 turns) replayed with every recorded hash matching.

| Class | Builds | Changes behind `Change` flags | Pass |
| --- | --- | --- | --- |
| C000 | v0.34.19 to v0.34.23 | baseline | 40 of 41 (c6dYiNx281, open) |
| C001 | v0.34.24, v0.34.25 | PR 5823 attack with nothing to conquer retreats in `init` | 44 of 44 |
| C002 to C005 | v0.34.18, v0.34.17, v0.34.16, v0.34.13 | none (wire schemas, stats, display messages only) | 7 of 7, 9 of 9, 6 of 6, 6 of 6 |
| C006 | v0.34.12 | PR 5564 (Channel Islands tribe names; map files differ, read from git) | 8 of 8 |
| C007 | v0.34.11 | PR 5545 hole cluster test in `PlayerExecution`, PR 5587 saturation midpoints (trade 230, train 500) | 11 of 11 |
| C008 | v0.34.8 to v0.34.10 | PR 5525 nuke kill credit for friendly units | 13 of 13 |
| C009 | v0.34.7 | none | 6 of 6 |
| C010 | v0.34.3, v0.34.4 | PR 5462 train saturation midpoint 300 | 18 of 18 |
| C011 | v0.34.0, v0.34.1 | PR 5425 attack speed divisor 7.77, PR 5436 team count without the minimum of 2 | 12 of 12 |
| C012 | v0.34.14 | tribe themes of Canary Islands, New Zealand, Pulicat Lake | 6 of 6 |
| C090 | v0.34.5 | none (data tables and map manifests only; manifests read from git) | 7 of 7 |
| C013 | v0.33.13, v0.33.14 | host `Math.*` instead of DetMath; old attack logic (PR 5173 family), old SAM targeting (5197), old MIRV and float Bezier curve (5188), spawn gates (4867, 4921, 5321, 5347), no train/trade saturation (5302), win bar 95 percent in teams (5223), no MIRV cancel on alliance (5054), no annex enclosure test (5027), no train split resolve (5200), old nation behaviour (5330, 5294, 5290, 5344), duo/trio team fill (4758), rails over impassable land (4903), water nukes without minimap fix (4975) | 49 of 49 |
| C014 | v0.33.12 | none (schemas only) | 50 of 50 |
| C015 | v0.33.11 | PR 5096 a station set to its own cluster leaves it first | 13 of 13 |
| C016 | v0.33.7 to v0.33.10 | no overtime (PR 5062) | 15 of 15 |
| C017 | v0.33.6 | Doomsday Clock without team ladder (5030) and wasteland (5052) | 8 of 8 |
| C018 | v0.33.5 | none (map manifest) | 20 of 20 |
| C019 | v0.33.4 | none | 14 of 14 |
| C020 | v0.33.2, v0.33.3 | no bulk purchase amounts (PR 4640) | 15 of 15 |
| C021 | v0.33.1 | Doomsday Clock without territory rot (PR 4893): fixed 5 percent troop floor, 6-wave ladder up to 55 percent, no idle on a winner (4953) | 16 of 16 (unexercised: no record enables the clock) |
| C022 | v0.33.0 | none | 2 of 3 (xQ449cAw fails in the upstream engine too) |
| C081 | v0.35.0 | PR 5899 nation overhaul (`AiAttackBehavior` retaliation, crown, boat routes, beachheads, bot annexation; nuke strikes and MIRV leak estimate; structures; warship lane clearing; emoji and alliance tweaks; attack relation rule), PR 5907 per-player trade and train spawn boost, PR 5833 emoji window, PR 5805 water graph edges sorted by node pair, PR 5719 train split reconcile, PR 5955 severed-land annexation in `PlayerExecution`, PR 5919 embargo-all skips allies, ranked FFA and clan wars win check | 12 of 12 (Medium and Easy long records; Hard and Impossible code paths unverified) |
| C082 | v0.35.2 | none (map manifests only) | 32 of 32 |
| C083 | v0.35.3, v0.35.4 | PR 6087 (tribes pick allied traitors, alliance renewal count), PR 6089 attack retreat order, PR 6016 water nuke mini tiles; the nation AI class split is behaviour-preserving | 20 of 20 |
| C084 | v0.35.5, v0.35.6 | none (map manifests only) | 16 of 16 |
| C085 | v0.35.7 | tutorial mode (`tutorial` config: nations and tribe only grow into free land, win check) | 8 of 8 (tutorial records have 33 to 44 turns) |
| C086 | v0.35.9 | tutorial silo, collapse and warship (`TutorialNation.rs`), 60,000 starting troops, small atom bombs | 24 of 24 |
| C087 | v0.35.11, v0.35.12, v0.35.14 | PR 6259 friend groups seated back to back in team assignment (`NationCount` pick is behaviour-preserving) | 36 of 36 |
| C088 | v0.36.0 | `PlayerExecution` rewrite (territory-area ranking, pinch pass; `PlayerExecutionV36.rs`), rail network digest in the game hash, FIFO `GameMap.bfs` (spawn tiles, nuke tile order), port spawn on the first valid shore tile, nation home-troop booking and overseas bot attacks, nation warship spare gold, water graph edge search bound, train spawned off rail, MIRV target overlap grid, tribe name data of new maps | 12 of 12 (10 long records) |
| C089 | v0.36.1 | tribe themes of four maps; the lone-cluster flood skip, scanline flood, shore-launch cache and `everyLaunchBlocked` short-cut are behaviour-preserving and not ported | 12 of 12 (8 long records) |
| C023 | v0.32.14 to v0.32.17 | no impassable terrain (4340), cardinal neighbours W, E, N, S (4495), no warship veterancy (4433), MIRV warheads not standard nukes (4706, 4707, 4756), SAM targeting before the 4770 fixes (one target, warhead removal, no final-tick shot, missile starts next tick), nations place ports on small lakes (4429), no reachable-shore transport targets (4577), pirating needs any port (4458), tribe names without themes (4647), water components with the 0xFF marker and node search by component (4479), real one-shot query for trade ship motion plans (4654) | 93 of 93 (9 long records of about 13,000 ticks) |
| C024 | v0.32.12, v0.32.13 | a spawn intent after the spawn phase is not ignored (4657) | 22 of 22 |
| C025 to C027 | v0.32.10 and v0.32.11, v0.32.8 and v0.32.9, v0.32.7 | Doomsday Clock variants (headcount scaled bar, no troop floor, retuned waves): not ported, `DoomsdayClockExecution` panics `not ported` when the clock is enabled | 9 of 9, 29 of 30 (XApT743N: the upstream engine disagrees with the record from tick 1160, ours matches upstream), 26 of 26 |
| C028 | v0.32.6 | no Doomsday Clock (4469) | 12 of 12 |
| C029 | v0.32.5 | cluster offset seeded from the name (4426), random spawn keeps the requested tile (4428), `nearby()` keeps fallout tiles (4422) | 22 of 22 |
| C030 | v0.32.4 | none (stats only) | 10 of 10 |
| C031 | v0.32.2 | railroad limit is the station range (4406) | 20 of 20 |
| C032 | v0.32.3 | no railroad length limit (4337) | 39 of 39 |
| C033 | v0.32.0, v0.32.1 | none (display message) | 11 of 11 |
| C034 | v0.32.0-beta6 | `unitsConstructed` counts buildings under construction twice (4320) | 1 of 1 (no long record exists; differential runs) |
| C035 | v0.32.0-beta5 | nation boat attack troops computed separately (4297) | 1 of 1 (no long record exists; differential runs) |
| C036 | v0.32.0-beta1 | seedrandom (ARC4) `PseudoRandom` (4233), no nation troop cap (4239, 4278), nations respect PVP immunity (4282), nuke speed 8, trade sigmoid 200 | no usable record (the one sample has no players); differential runs identical |
| C037 | v0.31.8 to v0.31.12 | spawn phase while `ticks <= numSpawnPhaseTurns` (3199), nation spawn retry and order (4117), defense post level drop (1563, 4016), no self guards (4014), silo and warship patrol checks (3997, 4151), attack neighbours N S W E (3820), train range 100 and railroad 120 (4079), bucket-queue abstract A* (4149) | 238 of 238 (14 long); differential runs identical |
| C038 | v0.31.0 to v0.31.6 | none (turnstile config) | 121 of 121 (87 long) |
| C039 | v0.32.0-beta3 | trade sigmoid 400 again | 1 of 1 (short, no hashes) |
| C040 | v0.30.15 to v0.30.28 | PRNG `nextInt` without floored bounds; v0.30 balance (SAM cooldown 120, silo 75, port build 20 ticks, bot attack malus 0.8, loss blend 0.7/0.3, bot troop growth 0.6, nuke speed 6, conquering a human takes all its gold, unfinished cities raise the troop cap); the nation AI before the v0.31 rework (`AiAttackRework31`: no lake shores, no nearby-player scan, a chosen attack ends the turn; `NationStructureRework31`: defense posts in the build order, no pacing; `NationNukeRework31`, `NationWarshipRework31`, `NationBehaviorTweaks31`, `NationExecutionTweaks31`, `NationDisabledAlliances31`); the plain warship (`WarshipExecutionV30.rs`: no docking, fractional patrol coordinates rejected); no water-component checks for trade, captured trade ships and warship spawn (`TradeWaterComponents31`); a SAM that reloads skips the tick; smaller nation name tables (`data/NationNamesV30.json`); clan tags parsed from usernames; single-id `move_warship` intents | 158 of 158 (17 long) |
| C041 | v0.30.11 to v0.30.13 | none (schemas and flag data) | 32 of 32 (5 long) |
| C042 | v0.30.10 | PRs 3448 and 3455: `PlayerExecution` recomputes clusters of players under 100 tiles every tick and compares the enemy's local box (older: only on the cluster interval, enemy border box) | 8 of 8 (5 long) |
| C043 | v0.30.6 to v0.30.9 | PR 3433 random-spawn games get a 150 turn spawn phase, PR 3437 nations do not donate troops in public games (older: neither) | 21 of 21 (5 long) |
| C044 | v0.30.0, v0.30.1 | trade and train retune (train spawn 18, 5 free cities, trade ship scale 50000; older than the 15, 9 and 75000 of C043) | 97 of 98 (35 long; EsfbsR7h mismatches in the upstream engine at tick 0 too) |
| C045 | v0.30.0-beta5 | PR 3410: no teammate nuking and no SAM shots at teammate nukes after a win | no usable record (the only sample is a one-player team game that throws `Too few teams` upstream too) |
| C046 | v0.30.22 | one nuke in six bounces past its target (`BouncingParabolaPathFinder`, detonation at the bounce tile); five maps added | 9 of 9 (5 long) |
| C047 | v0.30.0-beta2 | SAM cooldown 75, attacker loss blend 0.5/0.5, flat train gold, train spawn 25, no `disableAlliances` option | 1 of 1 (short, 279 turns) |
| C048 | v0.30.0-beta1 | `disableNations` config instead of `nations` (nations on every compact map), train spawn 18, bots spawned from the game seed, no 170 minute limit, no spawn-phase alliance reject | 4 of 4 (short, no hashes beyond the spawn phase) |
| C059 | v0.28.4 | **Phase 1, records not yet matching (needs C049 to C058 from P6-S3):** `FakeHumanNations28` (nations are `FakeHumanExecution` on their manifest cell, tribes run `BotExecutionV28` on `BotBehaviorV28`, alliances on `AllianceBehaviorV28`, no `NationCreation`), `NationDifficultyEconomy28` (start troops, cap and growth by difficulty), `ImmunityByConfig28` (spawn phase plus 5 s immunity for every player, `canAttack` without `canAttackPlayer`), `SpawnByTile28` (spawn tiles drawn when the executions are created; bots retry random land 30 apart), `LegacyAStar28` (`Pathfinding/Legacy28/`: bidirectional `SerialAStar` on the mini map for boats, trade ships, warships, rails and stations; `TransportShipUtilsV28`; no water graph), `LegacyProjectiles28` (shells and SAM missiles with one PRNG per finder, nukes and MIRVs on control-point Beziers, blast tiles cached at launch, tile-count alliance breaks, MIRVs scatter by `nextInt`), `RelationPenalties28` (attack -80, betrayal -200 and -40, middle finger, donations without emoji replies), `WarshipLegacy28` | Hand-checked against the upstream worktree with the C048 chain plus these flags: the 8 singleplayer records match (spawns of nations and bots), the 10 public records match 30 to 44 hashes (through tick 300 to 400) before diverging on rules of C049 to C058 (boat `src`, border order, attack and `PlayerExecution` rules) |

Era F math: builds before PR 5191 call the host `Math.exp/log/pow/atan2`. `Compat/HostMath.rs` runs V8's fdlibm port for `exp`, `log` and `atan2` and lets `Config` pick `Math.pow` between the V8 fdlibm port and the platform `powf` (`OFSIM_POW=fdlibm|platform`, default platform). All sampled records of C013 to C022 match with either pow, so the records do not separate them; the default follows the V8 13.3+ `std::pow` finding of [BuildPlan.md](BuildPlan.md). Differential runs against the upstream engine (Node 22, fdlibm pow) on records rewritten to enable nations at Hard and Impossible with large starting gold, water nukes and Duos/Quads teams (about 25 runs of 1,500 to 3,700 ticks) are identical tick by tick, except a stress run with 900M starting gold and water nukes on C013 that diverged at tick 2224 (cause and fix below). `OFSIM_TOGGLE=<Change>,...` flips named changes of every rule set for bisecting. C080 (a test release between C013 and the baseline) is not ported: its only sampled record has no hashes. The Doomsday Clock variants of C025 to C027 are not ported (they panic when the clock is enabled; no sampled record enables it).

Water finders keep the chain of the graph version they were created or refreshed under (`SharedWaterChain` handles in `PathFinder.rs`), as upstream's per-ship stepper does; before this a staggered trade ship that computed a fresh path right after a water-nuke graph rebuild used the new graph and diverged (found by a 9,000 tick water-nuke differential run on C023). A differential run with 300M starting gold, Hard nations and water nukes is now identical to upstream on C000 and C026.

Notes: the `AiAttackBehavior` random boat target fix (D1, nation boat targets use `owner.isFriendly(nation)`) is merged; the counts above include it. C081 to C087 (v0.35.x, files in `packages/*/src/`) share the rules set `NationsLikeHumans5899` for the nation overhaul; it is branched on in `AiAttackBehavior.rs`, `NationNukeBehavior.rs`, `NationStructureBehavior.rs`, `NationMIRVBehavior.rs`, `NationWarshipBehavior.rs`, `NationEmojiBehavior.rs` and `NationAllianceBehavior.rs` (in place, not as a variant file). New modules: `Nation/BotAnnexation.rs`, `Execution/SeveredLand.rs` (fallout-severed land annexation), `Nation/TutorialNation.rs`. Debugging aids: `Reference/RngLog.ts` with `OFSIM_RNG_LOG` and `OFSIM_RNG_STACK_AT` (per-draw PRNG logs and the stack of one draw), `Reference/ExecLog.ts` (executions added per tick upstream), `OFSIM_UNIT_LOG=<smallId>` in `ofsim trace`. C088 and C089 (v0.36.x) are side-by-side variants selected by `Change::*36` flags: `Execution/PlayerExecutionV36.rs` (shares the game's generation-stamped scratch exactly, since stamps left by one pass are read by the next), `GameMap::bfs_fifo`, the MIRV overlap grid in `MIRVExecution.rs`, and `home_troops` / `send` / `overseas_bots` in `AiAttackBehavior.rs`. The map name data of maps added since the baseline lives in `data/TribeNames.json` (`mapsV36`, `mapsV361`). The `RailNetworkImpl` rewrite is client preview code except for the new `hash()`; the nuke trajectory and port weight changes are behaviour-preserving. Differential runs (`ofsim scenario` territory fuzz with walled-in players, `ofsim nuke-scenario` with MIRVs) against the v0.36.0 tracer are identical when the run stays below tick 200 (later, tribes and nations launch executions the inert mode does not cover).

Era S tail (C040 to C048, v0.30.x; all `*31` flags are named after the release that introduced the newer behaviour because the upstream PR numbers were not looked up): the era S head classes remove the v0.32 meta changes, C040 removes everything v0.31 added to the nation AI, warships and water handling. Debugging an era S build needs `Reference/RngLogArc4.ts` (per-stream draw logs, the stream key is the first drawn value, so every attack's `PseudoRandom(123)` and every `PseudoRandom(tick)` merge into one key), `OFSIM_EXEC_LOG=<from>,<to>` and a reference worktree of the build (`OFSIM_SHARE_NODE_MODULES=<donor> Reference/Setup.sh <commit>`; builds before v0.31 need the `/api/env` stub of `EngineSupport.ts`). Old `PseudoRandom.nextInt` does not floor its bounds, so a patrol range of 112.5 yields fractional coordinates that index no tile (`WarshipExecutionV30.rs` rejects them like upstream). Not verified: the C045 build (its only record throws upstream) and C047 and C048 beyond their spawn phase (no long record exists); differential runs on rewritten records (Impossible and Hard nations, 50M to 300M starting gold) are identical for HvN (2,200 ticks) but two FFA runs diverge (a tile swap between two nations racing for free land at tick 311, a train spawned upstream only at tick 441), so Hard and Impossible FFA nation play of C040 to C044 is unproven.
