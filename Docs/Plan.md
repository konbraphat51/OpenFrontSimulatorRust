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
| P6 | Build rules for every build from 2025-10-01 (v0.25.21) to the newest, newest first along our data, then backwards | each build passes on sampled records | in progress: framework (`Builds/Rules.rs`, `Builds/Maps.rs`) and era D classes C001 to C012 done (see below); C013 onward and C088 onward pending; C081 to C087 (monorepo layout) done |

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
| C081 | v0.35.0 | PR 5899 nation overhaul (`AiAttackBehavior` retaliation, crown, boat routes, beachheads, bot annexation; nuke strikes and MIRV leak estimate; structures; warship lane clearing; emoji and alliance tweaks; attack relation rule), PR 5907 per-player trade and train spawn boost, PR 5833 emoji window, PR 5805 water graph edges sorted by node pair, PR 5719 train split reconcile, PR 5955 severed-land annexation in `PlayerExecution`, PR 5919 embargo-all skips allies, ranked FFA and clan wars win check | 12 of 12 (Medium and Easy long records; Hard and Impossible code paths unverified) |
| C082 | v0.35.2 | none (map manifests only) | 32 of 32 |
| C083 | v0.35.3, v0.35.4 | PR 6087 (tribes pick allied traitors, alliance renewal count), PR 6089 attack retreat order, PR 6016 water nuke mini tiles; the nation AI class split is behaviour-preserving | 20 of 20 |
| C084 | v0.35.5, v0.35.6 | none (map manifests only) | 16 of 16 |
| C085 | v0.35.7 | tutorial mode (`tutorial` config: nations and tribe only grow into free land, win check) | 8 of 8 (tutorial records have 33 to 44 turns) |
| C086 | v0.35.9 | tutorial silo, collapse and warship (`TutorialNation.rs`), 60,000 starting troops, small atom bombs | 24 of 24 |
| C087 | v0.35.11, v0.35.12, v0.35.14 | PR 6259 friend groups seated back to back in team assignment (`NationCount` pick is behaviour-preserving) | 36 of 36 |
| C088 | v0.36.0 | not ported: `PlayerExecution` rewrite (pinch pass, territory-area ranking), `RailNetworkImpl`, MIRV and nuke changes | fails at tick 210 |
| C089 | v0.36.1 | not ported (needs C088) | |

Notes: the `AiAttackBehavior` random boat target fix (D1, nation boat targets use `owner.isFriendly(nation)`) is merged; the counts above include it. C081 to C087 (v0.35.x, files in `packages/*/src/`) share the rules set `NationsLikeHumans5899` for the nation overhaul; it is branched on in `AiAttackBehavior.rs`, `NationNukeBehavior.rs`, `NationStructureBehavior.rs`, `NationMIRVBehavior.rs`, `NationWarshipBehavior.rs`, `NationEmojiBehavior.rs` and `NationAllianceBehavior.rs` (in place, not as a variant file). New modules: `Nation/BotAnnexation.rs`, `Execution/SeveredLand.rs` (fallout-severed land annexation), `Nation/TutorialNation.rs`. Debugging aids: `Reference/RngLog.ts` with `OFSIM_RNG_LOG` and `OFSIM_RNG_STACK_AT` (per-draw PRNG logs and the stack of one draw), `Reference/ExecLog.ts` (executions added per tick upstream), `OFSIM_UNIT_LOG=<smallId>` in `ofsim trace`. C088 needs the new `PlayerExecution` algorithm (pinch pass, `floodTerritoryAreas`, `executeSecondaryPass`, rewritten `removeCluster`), the rewritten `RailNetworkImpl`, and the MIRV, nuke, port and SAM execution changes; its records first differ at tick 210 on public games.
