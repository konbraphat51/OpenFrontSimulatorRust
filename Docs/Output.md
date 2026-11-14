# Replay output

Japanese version: [Output.ja.md](Output.ja.md)

Responsibility of this file: the files written by `ofsim replay` and `ofsim batch`, and how they relate to the upstream-based dumper of the OpenFrontDs project (`Engine/ReplayDumper.ts`), so consumers can switch engines.

## Files

`ofsim replay --record R --maps M --out DIR --every N` writes into `DIR`:

| File | Content |
| --- | --- |
| `Tiles/T{tick:06}.u16.zst` | zstd frame (level 3, content size set, no checksum) of the little-endian `u16` tile state, `width * height` entries, row-major. Bits 0 to 11 owner small id, bit 13 fallout, bit 14 defense bonus. `tick` is `ticks()` after the snapshot's tick ran. |
| `Entities.ndjson` | One JSON line per snapshot: `tick`, `inSpawnPhase`, `players`, `units`, `attacks`, `alliances` (field names and order as in `ReplayDumper.ts`). Human players have `name: null`; nation and bot names are written; `clientId` is written as upstream does. |
| `Result.json` | `status` (`ok`, `desync`, `error`), `gameID`, `gitCommit`, `finalTick`, `hashesChecked`, `firstMismatchTick`, `width`, `height`, `snapshotEvery`, `snapshotTicks`, `error`. |

Snapshots are taken when `ticks() % N == 0` after a tick, and once more after the last turn (also after a hash mismatch, not after an error). Recorded hashes are compared on the ticks where the engine hashes (every 10th); a mismatch stops the replay with `status: desync`.

## Compatibility with the upstream dumper

Checked with `Tools/CompareReplayOutput.py <upstream dir> <ofsim dir>` on three baseline records (`cLVr8coTsN`, `cj1KGD54xA`, `Q94sNurv`, `--every 100`): `Result.json`, `Entities.ndjson` and every tile frame are byte-identical.

Intentional differences: the `error` text of a failed run (Rust panic message and location instead of a JS stack) and the zstd frames are only guaranteed identical with the same libzstd version (both used 1.5.7 here).

## Batch

`ofsim batch --records <dir|glob> --maps M --out DIR [--every N] [--jobs J] [--verify-only] [--force]`

- `--records` is a directory (every `*.json`/`*.json.gz`) or `dir/pattern` with `*` and `?`.
- Each record writes to `DIR/<name>/` like `replay`; records whose `Result.json` exists are reported as `skipped` unless `--force`.
- `--verify-only` writes nothing and always runs.
- Parallel over records (`--jobs`, default one per core; largest records are started first, and each worker pulls the next record from a shared counter). A panic or error in one record is reported on its line and does not stop the others.
- One JSON line per record on stdout: `record`, `status` (`ok`, `desync`, `error`, `skipped`), `ticks`, `seconds`, `ticksPerSecond`, and when available `hashesChecked`, `snapshots`, `firstMismatchTick`, `error`.
- Exit code 1 when any record is neither `ok` nor `skipped`.
