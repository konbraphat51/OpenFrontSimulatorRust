# Reference tracer

Japanese version: [README.ja.md](README.ja.md)

Responsibility of this file: how to run the upstream TypeScript engine as a reference tracer, and the exact trace format that `ofsim trace` must reproduce.

## Setup

```sh
Reference/Setup.sh 4e837520e886          # Linux/macOS; Windows: Reference\Setup.ps1 4e837520e886
```

Clones upstream into `Cache/OpenFrontIO` if missing, adds the worktree `Cache/Worktrees/<commit12>`, runs `npm ci --ignore-scripts --engine-strict=false` (Node 22 works although upstream asks for 24) and copies `Reference/*.ts` into `<worktree>/ofsim-reference/`. Re-run it after editing a script; it is idempotent. The scripts import `../src/core/...`, so they always run against the engine of that worktree's commit (use the worktree matching the record's `gitCommit`).

## TraceDumper

```sh
cd Cache/Worktrees/<commit12>
npx tsx ofsim-reference/TraceDumper.ts <record.json[.gz]> <out.ndjson> [--full-at t1,t2,...] [--until tick]
```

Replays the record exactly as the client/worker does and writes one NDJSON line per tick. A summary JSON goes to stderr (ticks, hashes checked, mismatches, seconds, ticks per second); the exit code is 2 when any recorded hash mismatches. `--until T` stops after tick `T` (inclusive). `--full-at` additionally writes `<out.ndjson>.full.ndjson` with one line per listed tick. Engine console output is silenced (it contains usernames).

### Record to engine wiring

1. `decompressGameRecord(raw)` (`src/core/Util.ts`) fills sparse turns, so `record.turns[i].turnNumber == i`.
2. `toWireGameStartInfo({gameID, lobbyCreatedAt, config, players, tribes})` (`src/core/Util.ts`) builds the `GameStartInfo`.
3. `createGameRunner(gameStart, undefined, mapLoader, callback)` (`src/core/GameRunner.ts`) builds the game and calls `init()`.
4. For each turn in order: `runner.addTurn(turn)`, then `runner.executeNextTick()`.

### Trace format

One JSON object per line, in tick order, starting at tick 0, one line for every record turn:

```json
{"tick":530,"hash":13238919558137502,"recordHash":13238919558137502,"players":[[1,25000,0,"0",0,false,0]]}
```

| Field | Meaning |
| --- | --- |
| `tick` | `game.ticks()` **before** the tick executes. It equals the record turn index (`turn.turnNumber`). Tick `t` consumes turn `t`; afterwards `ticks()` is `t + 1`. |
| `hash` | `GameImpl.hash()` evaluated **after** `executeNextTick()` of tick `t` returned, for every tick. Value: `1 + sum over players (in Map insertion order) of Player.hash()`, where `Player.hash() = simpleHash(playerId) * (troops + numTilesOwned) + unitHashSum` and `Unit.hash() = tile + simpleHash(unitType) * unitId`. It is an integer-valued JS `number` (f64) that can exceed 2^53: write it as a plain decimal integer (no exponent, no decimals) and compare as f64. |
| `recordHash` | `turn.hash` of record turn `t` as stored (written by the server), or `null` when the turn has none (every 10th turn has one). |
| `players` | One array per entry of `game.allPlayers()` (all humans, nations and tribes in insertion order, alive or not): `[smallId, troops, tilesOwned, gold, unitHashSum, isAlive, numUnits]`. `troops` is a number as in JS (integer-valued f64, written without decimals), `tilesOwned` an integer, `gold` a decimal **string** (bigint), `unitHashSum` the sum of `Unit.hash()` over the player's active units (the unit part of the player hash), `isAlive` a boolean, `numUnits` the count of those units. |

Point of reading: upstream computes the stored hash inside `GameImpl.executeNextTick()` when `ticks() % 10 == 0`, after executions and player updates and before the water manager and `_ticks++`. The tracer reads after the whole tick, which agrees because the water manager only touches tile state, not troops, tiles owned or units. For ticks with a `recordHash`, `hash == recordHash` must hold; the dumper counts violations. The `Hash` update emitted by the engine is not used (the private `hash()` is called directly).

### Full dumps (`--full-at`)

`<out.ndjson>.full.ndjson` holds `{"tick":t,"full":{...}}` for each requested tick, read at the same point as the trace line: `width`, `height`, `tileState` (raw `u16` per tile, bits 0-11 owner), `owners` (owner small id per tile), `units` (`id,type,owner,tile,troops,level,active,hash`, in `game.units()` order) and `players` (`smallId,type,troops,gold,tilesOwned,isAlive,hasSpawned,hash`). Large: about 16 MB for a 533 tick game on a big map.

### Inert mode and scenarios

`--inert 1` (`InertPatch.ts`) makes every execution the Rust engine has not ported inert in the upstream engine (intents other than `PORTED_INTENTS`, `NationExecution`, `TribeExecution`, `DoomsdayClockExecution`); run `ofsim` with `OFSIM_INERT=1` for the same behaviour and compare with `Tools/TraceCompare.sh <record> <until> --inert`. Such traces never match the record hashes. `ScenarioDump.ts` runs a seeded territory fuzz (conquer, relinquish, walled-in players) for the same comparison against `ofsim scenario`.

On a work-in-progress branch, list extra ported intent types in `OFSIM_PORTED_INTENTS` (comma separated, e.g. `attack,boat`) and engine-created executions to keep active in `OFSIM_PORTED_EXECS` (`Nation`, `Tribe`, `DoomsdayClock`), so no shared file has to change. Executions created from inside a ported execution (for example a tribe launching an attack) are not covered: keep such paths ported on both sides, or compare only up to the first one.

`OFSIM_INERT_CLASSES` (comma separated) makes single classes inert on the TypeScript side while the Rust side has only a stub (which is inert under `OFSIM_INERT`): executions created from inside ported code (`AllianceRequestExecution`, `AllianceExtensionExecution`, `ConstructionExecution`: `init` and `tick` do nothing) and behaviours (`AiAttackBehavior`, `NationStructureBehavior`, `NationMIRVBehavior`, `NationNukeBehavior`: their decision methods return a default). Extend the tables in `InertPatch.ts` when a new stub needs it.

## TraceDiff

```sh
npx tsx ofsim-reference/TraceDiff.ts a.ndjson b.ndjson
```

Prints the first differing line (tick), the differing hash fields and the first differing player and field name. Exit code 0 when identical, 1 when different.

## FetchRecords

```sh
python Tools/FetchRecords.py --start 2026-10-03T00:00:00Z --end 2026-10-03T00:29:59Z \
  --type Public --mode "Free For All" --commit 4e837520e886 --max 5
```

Lists `/public/games` for the window (`--type`, `--mode` filter the listing), downloads `/public/game/<id>` into `Cache/Records/<commit12>/<gameId>.json.gz` with at least 0.5 s between requests, and keeps only records whose `gitCommit` starts with `--commit` (the commit is only known after the download). HTTP goes through `curl` (replace `http_get` for another transport). Games starting on a day in `--exclude-day` (repeatable, default `2026-10-02` UTC) are never downloaded: this is the hold-out data of the parent research project. Records contain usernames: never commit them.
