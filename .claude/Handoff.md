# Handoff: OfSimRust state after PR #1

Written 2026-11-14 for any later session that extends the simulator (new upstream releases, open items below). Continues issue konbraphat51/OpenFrontDs#4: every release since 2025-10-01, correctness verified by record hashes, general-purpose code.

## State

- PR #1 (`claude/wizardly-shannon-9zmxvz` into `main`) covers every build class except C080: C000 to C079 (v0.25.20 to v0.34.x, eras S, F and D) and C081 to C090 (v0.35.x to v0.36.1).
- Full `Tools/VerifyClass.sh` over all 90 ported classes (2026-11-14, `ee8a5d1`): 2,687 local records, 2,677 match every hash (773,262 hashes). The 10 others are unusable, not failures:
  - upstream itself mismatches: `xQ449cAw` (C022), `XApT743N` (C026), `EsfbsR7h` (C044, tick 0), `aUI84xqB` (C079, tick 0);
  - team games with 0 or 1 players that upstream rejects (`Too few teams`): `sAwSgnzN` (C036), `Tti9gRBW` (C045), `tFnwz6CW` (C068), `DpHztPui` and `X9p9eqbZ` (C074), `Hl8PC8LC` (C078).
- Performance: about 600 to 1,300 verify ticks per CPU second single-threaded (`Docs/Performance.md`), roughly 5 to 6x upstream (upstream last measured before two optimisation passes); peak RSS 140 to 280 MB per job.
- A new upstream release needs a class: run `Tools/BuildClassify.py` and `Tools/BuildPlanGen.py` (they regenerate `BuildClasses.toml` and `Docs/BuildPlan.md`), then follow the porting steps in `CLAUDE.md`.

## Read first

1. `Docs/Architecture.md`: binding porting rules, variant naming (`<Name>V<NN>.rs`, `V<NN>Beta` for pre-releases, shared traits for identical AI code).
2. `Docs/Plan.md`: phases and per-class status. `Docs/BuildStatus.json`: one line per class.
3. `Docs/BuildPlan.md`: one row per class (builds, parent, changed files, PRs, risk).
4. `Crates/Engine/src/Builds/Rules.rs`: the `Change` flags in area sections; class edges in `Builds/Edges/<class>.toml`.
5. `Crates/Engine/src/Builds/BuildClasses.toml`: commit to class and class to parent. Generated; do not edit by hand.
6. `Docs/Performance.md`: benchmark method and ranked remaining hot spots.

## Method that worked

- **One `Change` per upstream behaviour change.**
  - Name it after the upstream PR number (for example `WaterHpaSharedFinder4979`).
  - Use a version suffix only when no PR can be found.
  - `game.has(X)` is true for the newer behaviour. Branch at the exact place where upstream diverged.
  - Define each class edge in `Edges/<class>.toml` relative to its parent.
- **Large rewrites become side-by-side variant modules.** Examples: `PlayerExecutionV36.rs`, `FakeHumanExecutionV28.rs`, `Utils/BotBehaviorV28.rs`, `Pathfinding/Legacy28/`. Do not thread many flags through one function.
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

- **Resources:** 4 cores, about 15 GB RAM, and about 6 GB of free disk at handoff.
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
  - After a merge, run `Tools/QuickRegression.sh`; before publishing, `Tools/VerifyClass.sh` over every class.
- **Remote sessions** (`create_session` with this repository, the integration branch as source and an outcome branch `claude/port-<class>`) worked for parallel porting; their pushes need the user's approval in each session.

## Known open items

- **C080** (`115da0327634`, `v0.34.0-test-release`): not ported; its one record has no hashes.
- **C025 to C027:** Doomsday Clock variants are unported and panic when enabled (no sampled record enables it).
- **C058:** the v0.29.0-beta1 water pathfinding rewrite is unported; its two short records pass.
- **Weakly verified:**
  - C076 and C077: records replay but carry no hashes. C078: one hash compared.
  - Hard and Impossible nation play in FFA (C040 differential runs once found a latent divergence).
  - Classes without long records: C036, C039, C045, C047 and C048.
- **v0.36.2 (`8b5d071eed95`)** is not in `BuildClasses.toml`; it differs from v0.36.1 only in `WireSchemas`.
- **Upstream tracer for v0.25 and v0.26 builds** needs `miniMapBin` served by `Reference/EngineSupport.ts` (old map layout `mini_map.bin`).
- **Performance:** the 10x-upstream goal is not reached. Remaining hot spots (ranked in `Docs/Performance.md`): the attack conquest loop (about half the time), `PlayerExecution` clusters, water paths, Legacy28 A*.
