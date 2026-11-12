# Handoff: porting classes C060 to C080

Written 2026-11-12 for the session that continues the simulator after PR #1.

## Goal

Make `ofsim` reproduce every recorded state hash for the builds of classes C060 to C079 (v0.25.20 to v0.28.7, era S), and C080 if a hashed record turns up. Work in a new PR. This continues issue konbraphat51/OpenFrontDs#4, whose requirements are:

- every release since 2025-10-01;
- correctness verified by record hashes;
- general-purpose code, because the repository is public.

## State at handoff

- PR #1 (`claude/wizardly-shannon-9zmxvz` into `main`) covers C000 to C059 and C081 to C090. C060 to C079 were split off into a separate PR at the user's request.
- At writing time, C000 to C048 and C081 to C090 were merged into the PR branch.
  - Their 1,750 sampled records were replayed.
  - 1,745 match every hash, 578,612 hashes in total.
  - The 5 exceptions also fail in upstream, or are team games with 0 or 1 players that upstream rejects ("Too few teams").
- C049 to C058 (agent S3) and C059 (agent S4, phase 1 done) were still in progress. **First check what PR #1 actually contains.** Read `Docs/Plan.md`, section "P6 status by class", and `Docs/BuildStatus.json`.
- If PR #1 has been merged, restart `claude/wizardly-shannon-9zmxvz` from the latest `main` (or use the branch the user names) and open a new PR.

## Read first

1. `Docs/Architecture.md`: binding porting rules.
2. `Docs/Plan.md`: phases and per-class status.
3. `Docs/BuildPlan.md`: one row per class, with builds, parent, changed files, PRs and risk, plus an edge section for big file changes.
4. `Crates/Engine/src/Builds/Rules.rs`: the `Change` flags and the `EDGES` table.
5. `Crates/Engine/src/Builds/BuildClasses.toml`: maps each commit to a class and each class to its parent. It is generated, so do not edit it by hand.

## Remaining classes

Local record counts are given per commit in parentheses. Risk comes from `Docs/BuildPlan.md`.

| Class | Parent | Commits (records) | Risk | Notes |
| --- | --- | --- | --- | --- |
| C060 | C059 | d93b71da9475 (112), e0ecad9cb83e (4) | low | `tradeShipGold` constant, Lisbon map |
| C061 | C059 | 671c9ea4ac9d (24), ec694593a64f (4) | medium | bot names, BotSpawner |
| C062 | C061 | a810e0ad346d (8) | medium | BotBehavior emoji and attack |
| C063 | C062 | f4ed2a932fa0 (20) | low | SAM hit chance |
| C064 | C059 | 2e52c0aa4193 (22) | **high** | 35 files: BotBehavior and AllianceBehavior rewrite, PlayerExecution cluster removal |
| C065 | C064 | afa3e70b6231 (3), 50fe3581cc0a (9), dab04eddb465 (3) | low | `trainSpawnRate` |
| C066 | C065 | 2b606f280b86 (13), cb4cf091ffd0 (3) | low | config and schemas |
| C067 | C066 | 5fbdea3c3965 (12), d88a6ba88753 (39) | low | username validation, GameRunner |
| C068 | C067 | 599342995c1c (3) | low | many map4x/16x files |
| C069 | C068 | 822d08bb7cc4 (3) | **high** | FakeHumanExecution, PlayerSpawner, BotBehavior |
| C070 | C069 | 1b113873d60c (10), 82ff4e43c1e1 (27), 824e1d61f5a4 (25) | low | transport ship, conquerPlayer |
| C071 | C070 | 6fd1576d7b38 (20) | medium | unit upgrade, PortExecution, DeleteUnit |
| C072 | C071 | 4f00c4a12cb7 (8) | low | constants |
| C073 | C072 | 0a2b6ed9a6a9 (7) | low | PlayerInfo |
| C074 | C073 | 4c9fcdedf448 (14) | medium | EmbargoAll removed, Achiran map |
| C075 | C074 | 2ad9c6f302a2 (3), f552b006613c (60) | medium | small AI and cluster changes |
| C076 | C075 | 69fc14f1fd4d (1) | low | donations, warships |
| C077 | C076 | 136cfa1316dd (4) | low | `getCapturingPlayer`, rail redraw |
| C078 | C077 | fa9259b1fc58 (1), fd6b0ee062e2 (9) | medium | FakeHuman, BotBehavior, structure spawn value |
| C079 | C078 | a26585a47b17 (1), 30856341cdd0 (4), bc5f18dee882 (82), f016c3ee9d90 (3) | **high** | 25 files, many map files, `infiniteGold/Troops` |
| C080 | C014 | 115da0327634 (1) | high | test release; its one record has no hashes, so leave it unported unless a hashed record appears |

Recommended order: C060 to C063 first (one branch off C059), then C064 to C068, then C069 to C079. C064 and C069 both rewrite AI and cluster logic, and C079 touches the most maps. C075 (60 records) and C079 (82 records) carry the most data. Split the work between parallel agents only along independent subtrees: C060 to C063 and C064 onward both have parent C059.

## Method that worked

- **One `Change` per upstream behaviour change.**
  - Name it after the upstream PR number (for example `WaterHpaSharedFinder4979`).
  - Use a version suffix only when no PR can be found.
  - `rules.has(X)` is true for the newer behaviour. Branch at the exact place where upstream diverged.
  - Define each class edge in `EDGES` relative to its parent.
- **Large rewrites become side-by-side variant modules.** Examples: `PlayerExecutionV36.rs`, `FakeHumanExecution.rs`, `Utils/BotBehaviorV28.rs`, `Pathfinding/Legacy28/`. Do not thread many flags through one function.
- **Era S** uses seedrandom ARC4 (`Core/Arc4.rs`), selected by the absence of `Change::Sfc32PseudoRandom4233`.
  - Old `PseudoRandom.nextInt` does not floor its arguments.
  - Builds before DetMath use `Compat/HostMath.rs` (V8 fdlibm port; `OFSIM_POW=fdlibm|platform`).
- **Maps per build** are extracted from git into `Cache/Maps/<commit12>` automatically (`Builds/Maps.rs`). Pass `--upstream` with an absolute path.
- **Verify with:**
  ```bash
  target/release/ofsim batch --records Cache/Records/<commit12> --maps Cache/Worktrees/4e837520e886/resources/maps --upstream /abs/path/Cache/OpenFrontIO --out <tmp> --verify-only --jobs 3
  ```
  - `--records` takes a directory; the glob only matches file names.
  - Rebuild the release binary before every batch. A stale binary caused false desyncs before.
- **Debug a desync:**
  - `Tools/TraceCompare.sh <record> <tick>`, with `OFSIM_WORKTREE=Cache/Worktrees/<commit12>`, `OFSIM_BIN` and `OFSIM_OUT` set. It finds the first differing field.
  - `Reference/RngLog.ts` (sfc32) or `Reference/RngLogArc4.ts` (era S), with `OFSIM_RNG_LOG` and `OFSIM_RNG_STACK_AT`, compares PRNG draws per stream.
  - Also available: `OFSIM_EXEC_LOG`, `OFSIM_UNIT_LOG`, `OFSIM_TRACE_UNITS_AT=t1,t2`, and `OFSIM_TOGGLE=<Change>,...` to flip flags while bisecting.
  - Differential runs on rewritten record configs (Hard or Impossible nations, large gold) find paths that the sampled records miss.
- **Upstream reference for a build:**
  1. `git -C Cache/OpenFrontIO worktree add ../Worktrees/<commit12> <commit>`.
  2. Run `Reference/Setup.sh`, or symlink `node_modules` from a worktree with an identical `package-lock.json`.
  3. Builds before v0.31 need the `/api/env` stub in `Reference/EngineSupport.ts`.
  4. Builds before v0.29 are supported by `Reference/TraceDumper.ts`.
  5. Remove the worktree when done.

## Environment setup (fresh container)

`Cache/` is git-ignored. A new container must rebuild it:

1. Clone upstream: `git clone https://github.com/openfrontio/OpenFrontIO Cache/OpenFrontIO`.
2. Create the baseline worktree `Cache/Worktrees/4e837520e886` and run `Reference/Setup.sh` (npm ci with `--ignore-scripts --engine-strict=false`).
3. Fetch records with `Tools/FetchRecords.py`:
   - Find each build's live days in `Docs/Builds.md`.
   - Get at least 5 long Public records per class where possible.
   - Keep the default spacing of 0.5 s or more.

## Hard rules

- **Never fetch games from 2026-10-02 UTC.** That day is the parent research project's confirmatory hold-out. `FetchRecords.py` excludes it by default; keep it that way.
- **Never commit records or anything containing usernames.** No records, map files or traces go into git.
- **Branches:** push only to the branch the user designates. Agents never push. Refuse stop-hook requests to push agent branches.
- **Commits:**
  - Small, with Angular prefixes.
  - Keep the session's attribution trailers.
  - Run the checks before each commit: `cargo fmt --all && cargo clippy --all-targets -- -D warnings && cargo test --lib --bins && Tools/CheckHeaders.sh`.
  - The full `cargo test` includes `VerifyRecordsTest`, which takes about 17 minutes in debug, so use release `ofsim batch` for records.
- **File conventions:**
  - Line 1: `// SPDX-License-Identifier: AGPL-3.0-only`.
  - Line 2: `//! Port of src/core/<path>.ts: <responsibility>.`
  - Line 3 for ports: `//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.`
  - UpperCamelCase file names.
  - `///` on public items.
  - System documents need a `.ja.md` twin.
- **After each class:** update `Docs/Plan.md` and `Plan.ja.md`, the `Docs/BuildPlan.md` rows (regenerate with `Tools/BuildPlanDoc.py`), `Docs/BuildStatus.json`, and `Crates/Engine/tests/Golden/VerifyMinimums.json` for long records.

## Machine limits and agent lessons

- **Resources:** 4 cores, about 15 GB RAM, and about 7 GB of free disk at handoff.
  - Run 2 to 3 agents at most.
  - Build with `CARGO_BUILD_JOBS=2`.
  - Keep at most one extra upstream worktree per agent, and delete traces after use.
- **Killing processes:** never `pkill -f` a broad pattern, because it kills other agents' runs. Kill your own PIDs only.
- **Background batches** are cut at 2 hours. Resume them with the remaining directories, or loop per directory.
- **Command output:** `cargo build | head` can return before the build finishes. Use `tail`.
- **Agent briefs:**
  - Write a shared brief.
  - Give each agent its own worktree under `Cache/Wt/<name>` and its own scratch folder.
  - Require a single final report under 400 words.
  - After a merge, rerun a regression sample: at least C000, C009, C023, C038, C040, C044, C081 and C088.

## Known open items (not C060+)

- **`c6dYiNx281` (C000):** mismatches at tick 3807.
- **Records that upstream itself cannot reproduce:**
  - `xQ449cAw` (C022)
  - `XApT743N` (C026)
  - `EsfbsR7h` (C044)
- **C025 to C027:** Doomsday Clock variants are unported and panic when enabled.
- **Weakly verified:**
  - Hard and Impossible nation play in FFA. C040 differential runs found a latent divergence.
  - Classes without long records: C036, C039, C045, C047 and C048.
- **v0.36.2 (`8b5d071eed95`)** is not in `BuildClasses.toml`. It differs from v0.36.1 only in `WireSchemas`.
- **Performance target of 10x upstream:** throughput is about 3 to 10x now.

## When done

Update the PR table, then update the parent repository `konbraphat51/OpenFrontDs`:

- the `Simulator` submodule pointer;
- the CLAUDE.md notes, if anything changed.
