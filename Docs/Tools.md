# Porting tools

Four small tools that cut the tokens spent per ported class. All are stdlib-only Python 3 or bash, derive the repository root from their own location, and take `Cache/` paths from flags or environment variables (a worktree under `Cache/Wt/<name>` uses the main checkout's `Cache/`). Every tool has `--help`, terse output by default and `--verbose`.

| Tool | Question it answers |
| --- | --- |
| `Tools/ClassDigest.py <class>` | What did upstream change for this class, and which Rust files port it? |
| `Tools/VerifyClass.sh <class>` | Does every record of the class's builds pass? |
| `Tools/BatchSummary.py <ndjson...>` | What happened in this batch run? |
| `Tools/Triage.sh <record>` | Where does this record first diverge, and which flag moves it? |

## ClassDigest

```bash
Tools/ClassDigest.py C064                  # about 470 lines for a 31-file class
Tools/ClassDigest.py C064 --files-only     # 35 lines: files, +/- counts, Rust mapping
Tools/ClassDigest.py C064 --max-lines 150  # smaller hunk budget
```

- Resolves the first commit listed for the class and for its parent in `BuildClasses.toml`, and diffs upstream between them (`--upstream`, `OFSIM_UPSTREAM`, default `Cache/OpenFrontIO`), restricted to the engine roots of `BuildPlanGen.py`, ignoring whitespace.
- Drops files that cannot change the simulation: tests, `*Schemas.ts`, `Stats*.ts`, `GameView.ts`, `GameUpdates.ts`, `UserSettings.ts`, colour themes, `Env.ts`, map loaders, `lang/`, `i18n/`, json, md. The footer lists them (full paths with `--verbose`). Check the footer when a class looks too small.
- Groups hunks by file and enclosing `Class.method`. Imports, comments and brace-only lines are hidden unless `--verbose`.
- `--max-lines N` is a total hunk-line budget; each file gets a share proportional to its size (at least 4 lines), with a truncation marker.
- The `->` mapping comes from `//! Port of src/core/<path>.ts` headers in `Crates/` (index built on the fly). `~` marks a fallback by file name (for example `PlayerExecutionV36.rs`); `(no Rust port found)` means the file is not ported on this branch yet.
- The header line lists commit count, PR numbers between the commits (de-duplicated) and the `Change` flags already in the class's `EDGES` entry.

## VerifyClass

```bash
Tools/VerifyClass.sh C040 --jobs 2
```

Runs `ofsim batch --verify-only` over `Cache/Records/<commit12>/` of every build of the class with the baseline maps and an upstream path, writes NDJSON to a temporary directory (removed unless `--keep` or `--verbose`) and prints the BatchSummary. It builds nothing; rebuild `target/release/ofsim` first. Env: `OFSIM_BIN`, `OFSIM_MAPS`, `OFSIM_UPSTREAM`, `OFSIM_RECORDS`, `OFSIM_CACHE`, `OFSIM_TMP`.

## BatchSummary

```bash
Tools/BatchSummary.py batch.ndjson [--by-build Cache/Records]
```

One totals line (`records ok desync error hashes`), one line per non-ok record (record, status, first tick or error, cut to `--width`, default 120) and, with `--by-build`, one line per commit and class. Reads stdin without arguments.

## Triage

```bash
Tools/Triage.sh Cache/Records/<commit12>/<id>.json.gz [--trace] [--bisect-flags [--depth 2]] [--until T]
```

- Default: one line with commit, class, verdict, hashes matched and first mismatching tick.
- `--trace`: runs `Tools/TraceCompare.sh` up to that tick and prints the last 20 lines (first differing fields). Needs `OFSIM_WORKTREE`, the upstream worktree of the record's commit with `node_modules` and a current `ofsim-reference/`.
- `--bisect-flags`: reruns verify once per `Change` flag of the class's `EDGES` entry (`--depth 2` adds the parent's) with `OFSIM_TOGGLE=<flag>`. Each line says `REMOVES mismatch`, `MOVES a -> b`, `no effect`, or (when the record passes) `BREAKS at tick`. `--until T` speeds this up.
- Caveat: `OFSIM_TOGGLE` is applied in `Rules::for_class`; toggling flags of class C000 has no effect on its own builds.
