# Performance

Japanese version: [Performance.ja.md](Performance.ja.md)

Responsibility of this file: how throughput is measured, the current benchmark table, what the P6-P optimisation pass changed, and what is left. The invariant for every change: no state hash changes (same iteration order, same floating-point operation order); check with `Tools/QuickRegression.sh` and, before publishing, `Tools/VerifyClass.sh` over every ported class.

## Measuring

```bash
cargo build --release                      # fat LTO, about 90 s
Tools/Bench.py                             # verify and replay, 7 fixed long records (C000, C023, C040, C049, C059, C081, C088)
Tools/Bench.py --mode verify --only C081   # one era
Tools/Bench.py --runs 3 --json out.json    # best of 3, machine-readable
Tools/Bench.py --scale                     # also times `ofsim batch --jobs 1,2,4` over the whole set
Tools/Bench.py path/to/record.json.gz      # any record
```

- Each measurement is one `ofsim batch --jobs 1` process on one record, so the numbers are single-thread ticks per second (engine plus record loading, about 0.1 s) and the peak RSS of that process. `replay` writes snapshots every 100 ticks (`--every`) into a temporary directory.
- Env: `OFSIM_BIN` (default `target/release/ofsim`), `OFSIM_MAPS`, `OFSIM_UPSTREAM`, `OFSIM_RECORDS`, `OFSIM_CACHE`.
- The machine is a shared VM: wall-clock times vary by about 5 to 10 percent between runs. For before/after comparisons of a change use instruction counts, which are deterministic: `valgrind --tool=callgrind --callgrind-out-file=cg.out target/release/ofsim verify --record R --maps M --until 3000`, then `callgrind_annotate cg.out` (self cost) or `--inclusive=yes`. The release profile keeps line tables, so `--auto=yes` shows the hot lines.
- `Tools/BuildPgo.sh` builds a profile-guided binary into `target/pgo/release/ofsim` (needs `rustup component add llvm-tools`); 0 to 8 percent faster on C081 depending on the run (noisy VM), hashes unaffected. `-C target-cpu=x86-64-v3` was measured and gives nothing (the code is branch and memory bound), so it is not used.

## Benchmark (release profile, one thread, 4-core shared VM)

| Class | Record | Ticks | Before (verify tick/s) | After | Speed-up | Before (replay) | After | Peak RSS before to after |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C000 | dqD3tRRz64 | 11,102 | 423 | 516 | 1.22x | 404 | 510 | 536 to 354 MB |
| C023 | tEJEgWSc | 12,932 | 318 | 397 | 1.25x | 298 | 403 | 921 to 628 MB |
| C040 | HQ4dbm2d | 4,362 | 413 | 535 | 1.30x | 410 | 554 | 343 to 255 MB |
| C049 | FMHzJ8XF | 12,875 | 718 | 888 | 1.24x | 687 | 808 | 472 to 323 MB |
| C059 | 67986TJw | 7,002 | 244 | 388 | 1.59x | 239 | 384 | 525 to 376 MB |
| C081 | cjPSE6enhb | 10,712 | 519 | 655 | 1.26x | 512 | 641 | 413 to 285 MB |
| C088 | cxzGcV5tJm | 9,022 | 291 | 388 | 1.33x | 285 | 403 | 516 to 362 MB |
| all | | 68,007 | 387 | 506 | 1.31x | 373 | 504 | |

"Before" is the integration branch at `aa545d3`. Instruction counts over the first 3000 to 5000 ticks fell by 25 to 40 percent (C081 31.5 G to 22.8 G, C000 39.6 G to 28.9 G, C088 86.1 G to 54.2 G for 5000 ticks); wall time falls less because the remaining time is dominated by cache misses in the tile sets and the tile arrays. With the previous measurement against the upstream engine (3.3x to 3.8x on other records) this puts the engine at roughly 4.3x to 5x upstream single-threaded; upstream was not re-measured.

Snapshots: `ofsim replay` compresses and writes tile frames on 3 background threads (1 per job in `batch`); with `--every 1` on HQ4dbm2d this took 59.8 s before and 33.9 s after, the rest being zstd level 3 itself (frames must stay byte-identical to the upstream dumper, so the level cannot change).

## What changed in the P6-P pass

| Change | Where | Effect |
| --- | --- | --- |
| `Rules::has` as a bitset instead of a `BTreeSet` lookup | `Builds/Rules.rs` | 10 to 12 percent on C000/C081 (it sat in every neighbour walk) |
| FxHash for lookup-only maps (Legacy28 `SerialAStar`, shore coercion, BFS, spatial query) | `Compat/OrderedMap.rs` (`FxHashMap`, `FxHashSet`), `Pathfinding/` | C059 -35 percent (SipHash was 40 percent of its instructions) |
| `OrderedMap::insert` with one hash operation, `u32` slot index | `Compat/OrderedMap.rs` | fewer instructions, tile-set tables half the size (RSS -25 percent) |
| Division-free `x(tile)`/`y(tile)` (Lemire fastmod, tested exhaustively over widths) | `Game/GameMap.rs` | fewer cycles in every neighbour walk and A* |
| Per-type unit counts short-circuit `has_unit_nearby`, `nearby_units`, `any_unit_nearby` | `Game/UnitGrid.rs` | skips the defense-post scan per conquered tile when none exist |
| `border_owner` per tile: border-set add/delete only on a status change | `Game/GameImpl.rs` | fewer hash operations per conquest |
| Packed `FlatBinaryHeap` entries | `Execution/Utils/FlatBinaryHeap.rs` | about 2 percent |
| Scanline fill with row slices for the territory flood of `PlayerExecutionV36` | `Execution/PlayerExecutionV36.rs` | C088 -20 percent instructions (the flood visits about 100,000 tiles per tick there) |
| Free the border set of a deleted attack | `Game/AttackImpl.rs` | -60 MB RSS on long games |
| Libm-free `floor` in `DetMath::exp`, one terrain read in `can_see`, monomorphised flood predicate | various | about 1 percent each |
| Background tile compression for `replay`, longest-first dispatch for `batch` | `Crates/Cli` | replay with dense snapshots 1.8x; no straggler at the end of a batch |

## Where the time goes now

Callgrind, inclusive, first 5000 to 7000 ticks; percentages of the run, they overlap.

1. `AttackExecution::tick` 44 to 51 percent: conquest loop (`add_neighbors` 14 to 18, `GameImpl::conquer` 13 to 15, of which `update_borders` 9 to 10, heap 3, `attack_logic` 5 to 6 with `DetMath` log and exp, defense-post query 2 to 4).
2. Water paths 13 to 15 percent (C081, C023): shore coercion, line-of-sight smoothing (`los_smooth` 4), bounded local A* (3 to 4), hierarchical A* (3).
3. `PlayerExecution` clusters 12 to 15 percent (`calculate_clusters`, the V36 flood and pinch passes).
4. `OrderedSet` hash operations on the tile and border sets about 8 percent (`insert`, `remove`, `maybe_compact`): a dedicated open-addressing `u32` set, or a shared per-tile position array for the `tiles` and `border_tiles` sets, would remove most of it.
5. Warship and trade ship ticks 8 to 11 percent (`hunt_down_trade_ship`, `find_best_target`), train stations 6 to 8 percent, rail `connect_station` 4 to 7 percent.
6. `PseudoRandom::next` of the seedrandom (ARC4) builds 5 percent on C049.

Ideas that need care because they touch ordering: keeping `tiles` and `border_tiles` in per-tile position arrays, caching `large_territory_bonus` per attack, incremental cluster maintenance instead of recomputation on every calculation.

## Batch

`ofsim batch --jobs N` sorts the records by size (largest first) and every worker takes the next record from a shared counter, so a long game never starts last. Peak memory is per job (about 250 to 650 MB for the benchmark records, growing with the number of owned tiles), so 4 jobs need about 2.5 GB.

Scaling on the 4-core VM (`Tools/Bench.py --scale`, `batch --verify-only` over the 7 benchmark records, about 136 s of work):

| `--jobs` | Wall | Ticks/s (all jobs) | Speed-up | Peak RSS of one job process set |
| --- | --- | --- | --- | --- |
| 1 | 136.2 s | 499 | 1.0x | 678 MB |
| 2 | 67.9 s | 1,002 | 2.0x | 968 MB |
| 4 | 53.8 s | 1,265 | 2.5x | 1,554 MB |

Seven records of unequal length cannot fill 4 workers (ideal 36 s), and the VM itself does not scale linearly: four copies of the same single-thread run take 13.9 s instead of 9.7 s (about 2.8x for 4 processes) because of shared memory bandwidth and cache. With a long list of records `--jobs 4` therefore gives about 2.8x here; the code has no shared state between jobs. Lower memory traffic per tick (smaller tile sets, see the list above) is what would improve the parallel scaling further.
