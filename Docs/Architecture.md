# Architecture

Japanese version: [Architecture.ja.md](Architecture.ja.md)

Responsibility of this file: the design of the simulator and the porting rules every contributor (human or agent) follows. Read it before writing any engine code.

## Goal

Replay any public OpenFront game record from 2025-10-01 onward, tick by tick, with the game state **bit-identical** to the upstream TypeScript engine of the record's `gitCommit`, and do it much faster than the upstream engine. Correctness is proven by the state hash the upstream engine writes into every record every 10 turns (`GameImpl.hash()`), plus a differential trace against the upstream engine for debugging.

## Workspace

| Crate | Path | Responsibility |
| --- | --- | --- |
| `ofsim-record` | `Crates/Record` | Record JSON schema (serde), gzip loading, expansion of sparse turns (`decompressGameRecord`), the start info the engine consumes (`toWireGameStartInfo`). No engine logic. |
| `ofsim-map` | `Crates/Map` | Reading a map directory (`map.bin`, `map4x.bin`, `map16x.bin`, `manifest.json`) into plain data, and the `GameMapType` name to directory table. No engine logic. |
| `ofsim-engine` | `Crates/Engine` | The deterministic simulation: game state, executions, pathfinding, AI, hashing, build rules. |
| `ofsim` | `Crates/Cli` | The `ofsim` CLI: `verify`, `replay` (snapshots), `trace`, `trace-diff`, `batch`. |
| reference tracer | `Reference/` | TypeScript scripts copied into an upstream worktree that emit the same trace as `ofsim trace`, for differential debugging. |

## Engine module map

Rust modules mirror upstream `src/core/` so that every Rust file has one upstream counterpart named in its header comment (`//! Port of src/core/game/PlayerImpl.ts`).

```
Crates/Engine/src/
  Lib.rs
  Compat/        JS semantics: number helpers, insertion-ordered sets and maps, stable sort
  Core/          PseudoRandom (sfc32 or seedrandom ARC4), DetMath, Util (simpleHash, toInt, ...)
  Configuration/ Config (DefaultConfig values of the build), unit infos
  Game/          GameMap, TileSet, Game (shared types and enums), GameImpl, PlayerImpl, UnitImpl,
                 AttackImpl, AllianceImpl, AllianceRequestImpl, UnitGrid, RailNetwork, TeamAssignment,
                 NationCreation, WaterManager, ...
  Execution/     one file per upstream execution; Nation/ and Alliance/ subfolders as upstream
  Pathfinding/   as upstream
  Runner/        GameRunner, Executor (ExecutionManager.ts)
  Builds/        Rules: which upstream behaviour a given build has
```

Naming follows the repository convention: directories and files in UpperCamelCase (crate-level `non_snake_case` is allowed for module names); functions, methods and variables in Rust snake_case; types in UpperCamelCase. A module and a type must not share a name inside the same parent module (they share the type namespace), so never `pub use X::X` in the parent of module `X`; import types by their full path instead (`use crate::Game::GameMap::GameMap;`).

## Core data model

The upstream engine is an object graph (players hold units, units hold owners, executions hold everything). Rust uses one owner, `GameImpl`, and integer handles:

| Upstream object | Rust handle | Storage |
| --- | --- | --- |
| `TileRef` | `TileRef = u32` | `GameMap` arrays |
| `Player` / `TerraNullius` | `SmallId = u16` (0 = terra nullius) | `GameImpl.players: Vec<PlayerImpl>`, index `small_id - 1`, in insertion order (players are never removed, so this is also the JS `Map` order) |
| `Unit` | `UnitId = u32` (upstream `nextUnitID`) | `GameImpl.units` keyed by id; each player keeps its own ordered `Vec<UnitId>` exactly like `PlayerImpl._units` |
| `Attack` | `AttackId` (arena index) | arena in `GameImpl`; players keep ordered id lists like `_incomingAttacks` |
| `Alliance`, `AllianceRequest` | arena ids | arenas in `GameImpl`, ordered lists where upstream keeps arrays |

Upstream methods of `PlayerImpl`, `UnitImpl`, `AttackImpl` that touch other objects are ported onto short-lived handles that borrow the game:

```rust
pub struct PlayerRef<'a> { game: &'a GameImpl, id: SmallId }      // read-only methods
pub struct PlayerMut<'a> { game: &'a mut GameImpl, id: SmallId }  // mutating methods
// game.player(id).troops(), game.player_mut(id).conquer(tile)
```

Method names are the upstream names in snake_case, so ported code reads like the original. The same pattern applies to units (`UnitRef`/`UnitMut`).

## Executions

```rust
pub trait Execution {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick);
    fn tick(&mut self, game: &mut GameImpl, ticks: Tick);
    fn is_active(&self) -> bool;
    fn active_during_spawn_phase(&self) -> bool;
}
```

`GameImpl` stores `execs: Vec<Box<dyn Execution>>` and `un_init_execs`. `execute_next_tick` takes `execs` out with `std::mem::take` while ticking them, so executions may freely borrow the game mutably; executions added meanwhile go to `un_init_execs`, as upstream. The upstream order of operations in `GameImpl.executeNextTick` (tick active execs, init uninitialised ones, compact, append, player updates, hash every 10 ticks, water manager, `_ticks++`) is reproduced exactly, including its quirks (for example, executions added during an `init` call in the same loop are dropped because upstream reassigns `unInitExecs`). Never "fix" an upstream quirk: a fix is a desync.

### Game and execution API (P2)

- `GameImpl::new(humans, nations, map, mini_map, config, team_spawn_areas)`; `GameImpl::execute_next_tick() -> Option<f64>` runs one tick and returns the hash computed inside it every 10th tick (what the record stores). Everything is also reachable as public fields and accessors: `game.map` (the `GameMapImpl`), `game.config()`, `game.stats_mut()`, `game.ticks()`.
- Players: `game.player(id) -> PlayerRef` (read) and `game.player_mut(id) -> PlayerMut` (write); both deref to `PlayerImpl` for plain getters. `id` is a small id; `0` is terra nullius. Units: `game.unit(id)` (None once removed), `game.unit_any(id)` (also deleted ones), `game.unit_mut(id) -> UnitMut`. Attacks, alliances and requests: `game.attack(id)`, `game.alliance(id)`, `game.alliance_request(id)` and their `_mut` forms; `PlayerMut::create_attack`, `GameImpl::delete_attack`.
- Executions implement `Execution { init, tick, is_active, active_during_spawn_phase }` (all take `&mut GameImpl`) and are added with `game.add_execution(Box::new(..))`. `Runner/Executor.rs` maps intents to executions: replace the `NotPorted::at_creation("<Name>")` arm of the intent when an execution lands, and add the intent to `PORTED_INTENTS` in `Reference/InertPatch.ts`. `GameRunner::init` creates the executions every game starts with.
- `NotPorted` executions panic with `not ported: <Name>`; `ofsim verify` reports the tick and the message. The real `WaterManager`, the water executions (transport ship, boat retreat, trade ship, warship, move warship) and the rail network (stations, railroads, clusters and trains in `Game/RailNetwork.rs` and `Game/TrainStation.rs`) are ported; `Reference/NavalScenarioDump.ts` (`ofsim naval-scenario`) is the differential scenario of the water executions, run with `OFSIM_INERT_CLASSES=Attack,Shell` on the tracer side.
- Client-only plumbing is not ported (updates, name placement, UI queries, most `stats()` calls); the simulation reads back only the attacks a human sent and the MIRVs launched, kept in `StatsImpl`.
- Debugging: `Reference/RngLog.ts` plus `OFSIM_RNG_LOG=<file>` (every PRNG draw as `<counter word> <raw u32>`; compare per stream, not by global line, since stream creation order differs) and `OFSIM_RNG_STACK_AT=<n>` (stack of draw n), `Reference/ExecLog.ts` (executions the upstream engine adds per tick), `OFSIM_UNIT_LOG=<smallId>` for `ofsim trace`, `Tools/VerifyAll.sh` (summary table), `Tools/TraceCompare.sh` (reference trace, `ofsim trace`, `TraceDiff`; `--inert` makes the not-ported executions inert on both sides), the seeded territory scenario (`ofsim scenario`, `Reference/ScenarioDump.ts`) and the seeded nuke, SAM and Doomsday Clock scenario (`ofsim nuke-scenario`, `Reference/NukeScenarioDump.ts`, `Tools/NukeScenarioCompare.sh`; it also compares fallout and water conversion through a per-tick tile-state digest). `Crates/Engine/tests/VerifyRecordsTest.rs` keeps the matched-hash counts of `tests/Golden/VerifyMinimums.json` from regressing.

## Determinism rules (the porting contract)

1. **Numbers.** A JS `number` is an `f64`. Keep `f64` unless the value is provably an integer that fits the Rust type at every use, and convert exactly where upstream truncates (`Math.floor`, `Math.trunc`, `| 0`, `toInt`). Upstream `bigint` (troops, gold) becomes `i64` or `i128` as range requires. Reproduce bitwise operators with JS `ToInt32`/`ToUint32` semantics (`Compat::JsNumber`). `Math.round` is `floor(x + 0.5)`, not Rust `round`. Keep the upstream order of floating-point operations; `a * b / c` and `a * (b / c)` differ.
2. **Transcendentals.** Builds with `DetMath` use the ported `Core::DetMath` bit for bit. Builds before it call V8's `Math.exp/log/pow/atan2/sin/cos`; those are ported from V8's fdlibm (`src/base/ieee754.cc`) in `Compat::V8Math`, never Rust `f64` methods (`sqrt` is exact in both and may use Rust).
3. **Iteration order.** JS `Map` and `Set` iterate in insertion order, a delete plus re-add moves to the end, and entries added during iteration are visited. Use `Compat` ordered collections (or the ported `TileSet`), never `HashMap`/`HashSet` iteration where order can reach the state. `HashMap` is fine for pure lookups.
4. **Sorting.** `Array.prototype.sort` is stable; use stable sorts with the same comparator. Where a comparator can be inconsistent (NaN, ties broken by nothing), port V8's TimSort (`Compat::JsSort`) instead of assuming equivalence.
5. **Randomness.** Every `PseudoRandom` instance is seeded exactly as upstream (game, executor, player, tribe spawner, per execution with `mg.ticks()` and so on) and is drawn in the same order. A missed or extra draw desyncs everything after it. Builds before PR 4233 (era S, C036 and older) use npm `seedrandom` (ARC4, `Core/Arc4.rs`) seeded with `String(seed)`; every site creates its generator with `PseudoRandom::for_rules(rules, seed)`, and the `Change::Sfc32PseudoRandom4233` flag selects sfc32 or ARC4.
6. **Strings.** Player ids, `simpleHash` inputs and `nextID()` output must be byte-identical (`toString(36)`, `padStart`). `simpleHash` iterates UTF-16 code units.
7. **No client-only code.** Rendering, stats sent to the client, emojis and chat that never reach the state are not ported, unless simulation logic reads them (check before skipping: for example nation AI reads emojis and relations).
8. **Errors.** Where upstream throws inside a tick, the runner reports the error for that tick; do not paper over it.

## Build rules (versions)

Every record carries the upstream `gitCommit` of the server that played it. `Builds/Rules.rs` maps a commit to a `Rules` value: a set of named upstream changes (`Change::DetMath5191`, named after the upstream PR) that this build contains. Release branches receive cherry-picks, so membership is per build, not a linear version order.

The mapping has two layers. `Builds/BuildClasses.toml` (generated by `Tools/BuildPlanGen.py`, checked in, embedded with `include_str!` and parsed once) lists every known build (12-character commit prefix) with its class (builds with identical simulation code) and every class with its parent in the diff tree rooted at the baseline class C000. The `EDGES` table in `Rules.rs` says which `Change`s a ported class adds to or removes from its parent; a class's set is its parent's set plus the edge. A class without an edge, or whose ancestors lack one, is not ported: `Rules::for_commit` fails with `build <commit> (class Cnnn) not ported yet`; a commit absent from the table fails with `unsupported build <commit>` and names the known build sharing the longest prefix.

Engine code branches on `game.config().rules().has(Change::X)` at the exact place upstream changed; `has` is true when the build contains the changed behaviour, so baseline (newest) code is the `true` branch and edges for older builds mostly remove changes. When a subsystem was rewritten wholesale (for example pathfinding), the old and new implementations live side by side (`AStarWaterV1.rs`, `AStarWaterV2.rs`) behind one trait. Configuration constants that change between builds live in `Configuration` and are selected by `Rules`. The baseline build is v0.34.22 (`4e837520e886`); other builds are added by diffing upstream `src/core` against it.

Maps and name tables also change between builds. `Builds/Maps.rs` compares the git tree of `resources/maps/<map>` in the record's commit with the baseline's; when they differ, `ofsim verify/trace/replay` extract that map once from the upstream clone (`--upstream`, default `Cache/OpenFrontIO`) into `Cache/Maps/<commit12>/` and read it from there. Per-map tribe name data that differs between builds is selected by `Change` flags in `Execution/Utils/TribeNames.rs`.

From v0.35.0 (C081 on) the upstream sources moved from `src/core/` to `packages/{engine,engine-lib,engine-api,shared}/src/` and are imported as `@openfront/<package>/...`. Port headers keep the baseline `src/core/...` path. The reference tooling bridges the layouts: `Reference/MakeCoreShim.mjs` writes `src/core/` re-export shims into such a worktree, `Reference/EngineSupport.ts` (`createRunnerFor`) passes loaded map files instead of a map loader, and `Tools/GenEngineData.mts` searches both layouts. Mapping: `Schemas` is `engine-api/Schemas`, `Util` splits into `engine-lib/Util` and `shared/SharedUtil`, `Game` into `engine/game/Game` and `engine-api/game/GameTypes`, `GameMap` into `engine-api/game/GameMap` and `engine-lib/game/GameMapImpl`, `Config` into `shared/configuration` and `engine/configuration/EngineConfig`; execution, pathfinding and game files keep their relative paths under `engine/src/`.

## Verification

- `ofsim verify <record>` replays and compares the computed hash with the record at every turn that has one (every 10 turns). A record passes when every hash matches through its last turn.
- `Reference/TraceDumper.ts` runs the upstream engine and `ofsim trace` runs ours; both write one NDJSON line per tick (hash and per-player troops, tiles, gold and unit hash; optionally full tile owners at chosen ticks). `ofsim trace-diff` reports the first tick and the first player or unit that differ, which is where to look.
- Unit tests port upstream tests where they exist (`tests/` in the upstream repository) and check `Compat` and `Core` against values computed with node.
- Real records are never committed (they contain usernames). Tests that need them read a local cache directory and are skipped when it is absent.

## Output

`ofsim replay --every N` writes the same files as the upstream-based replay dumper of the OpenFrontDs project, so consumers can switch engines: `Tiles/T{tick:06}.u16.zst` (zstd little-endian `u16` tile state), `Entities.ndjson` (players, units, attacks and alliances per snapshot; no human names) and `Result.json` (verdict, hashes checked, first mismatch).

## Licensing

The engine is a port of the AGPL-3.0 OpenFrontIO code, so the whole repository is AGPL-3.0-only. Map files and records are not redistributed here. See [Licensing.md](Licensing.md) and `NOTICE`.

File header convention: every Rust and TypeScript source file starts with `// SPDX-License-Identifier: AGPL-3.0-only`, then the `//!` responsibility comment. In ported files that comment names the upstream path, for example `//! Port of src/core/game/PlayerImpl.ts (OpenFrontIO, © OpenFront and Contributors).` or a following `//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.` line. `Tools/CheckHeaders.sh` fails when a `.rs`, `.ts`, `.mts` or `.mjs` file lacks the SPDX line.
