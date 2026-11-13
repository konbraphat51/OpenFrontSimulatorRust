# OfSimRust: agent notes

Hash-exact Rust replay of the OpenFront engine (upstream `openfrontio/OpenFrontIO`, `src/core`), AGPL-3.0-only. Correct means every state hash in a game record matches. Baseline build v0.34.22 (`4e837520e886`); other builds are selected by the record's `gitCommit` through `Crates/Engine/src/Builds/`.

## Read when needed

- `Docs/Architecture.md`: binding porting rules (JS number semantics, iteration order, PRNG).
- `Docs/Plan.md`, section "P6 status by class": what each class changed and how well it is verified.
- `Docs/BuildPlan.md`: class rows (builds, parent, changed files, PRs, risk).
- `Docs/Tools.md`: the helper tools below.
- `.claude/Handoff.md`: state of the remaining classes and lessons from earlier sessions.

## Commands

- Checks before each commit: `cargo fmt --all && cargo clippy --all-targets -q -- -D warnings && cargo test --lib --bins -q && Tools/CheckHeaders.sh`. The full `cargo test` adds `VerifyRecordsTest` (about 17 min in debug); use release `ofsim batch` for records instead.
- Build for porting and verification: `CARGO_BUILD_JOBS=2 cargo build --profile verify -q` (incremental, about 12 s after an edit; `--release` is for distribution). Read build output with `tail`, never `head` (a pipe closing early can hide failures).
- Upstream diff of a class: `Tools/ClassDigest.py C064` (`--files-only` first).
- Verify a class: `Tools/VerifyClass.sh C064` (summary of every record; rebuild first). Fast regression over all ported classes: `Tools/QuickRegression.sh`.
- First divergence of a record: `Tools/Triage.sh <record> [--trace] [--bisect-flags]`.
- Summarise batch output: `Tools/BatchSummary.py <ndjson>`.
- Throughput: `Tools/Bench.py` (fixed records of seven eras, ticks/s and peak RSS); compare changes by callgrind instruction counts (`Docs/Performance.md`). Optimisations must keep iteration order and float operation order; never swap a hasher on a map that is iterated.

## Porting a class

1. Read `Tools/ClassDigest.py <class>`; open only the upstream files and Rust ports it lists.
2. One `Change` variant per upstream behaviour change in `Builds/Rules.rs`, named after the upstream PR (`WaterHpaSharedFinder4979`); `game.has(X)` is true for the newer behaviour. Branch exactly where upstream diverged.
3. Large rewrites become side-by-side variant modules (`PlayerExecutionV36.rs`, `FakeHumanExecutionV28.rs`; naming rule in `Docs/Architecture.md`, "Variant module naming": `<Name>V<NN>.rs` with the minor version of the variant's build nearest the baseline, `Legacy<NN>/` for whole subsystems), not many flags threaded through one function.
4. Add the class edge as `Builds/Edges/<class>.toml`, run `Tools/VerifyClass.sh`, then `Tools/Triage.sh` on the first failing record.
5. Run `Tools/QuickRegression.sh` and update `Docs/Plan.md`/`.ja.md`, `Docs/BuildStatus.json` and long records in `Crates/Engine/tests/Golden/VerifyMinimums.json`.

## Rules

- Never fetch games from 2026-10-02 UTC (hold-out of the parent research project). Never commit records, traces or map files; they live in git-ignored `Cache/`.
- Space API requests at least 0.5 s apart (`Tools/FetchRecords.py` does).
- File header: line 1 `// SPDX-License-Identifier: AGPL-3.0-only`; line 2 `//! Port of src/core/<path>.ts: <responsibility>.` (or `//! <responsibility>`); line 3 for ports `//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.` UpperCamelCase file names, `///` on public items, short English comments.
- Small commits with Angular prefixes. Agents never push.
- User-facing documents in `Docs/` have a `.ja.md` twin; agent-only files (this file, `.claude/`) are English only.
- Upstream worktrees: `Reference/Setup.sh <commit>` (shared `node_modules` pool); remove them when done.
- Shared machine: at most 2 to 3 agents, one extra upstream worktree each, delete traces after use, never `pkill -f` broad patterns.
