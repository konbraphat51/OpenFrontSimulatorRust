# Performance

Japanese version: [Performance.ja.md](Performance.ja.md)

Responsibility of this file: how throughput is measured, the current benchmark table, what the P6-P and P7 optimisation passes changed, and what is left. The invariant for every change: no state hash changes (same iteration order, same floating-point operation order); check with `Tools/QuickRegression.sh` and, before publishing, `Tools/VerifyClass.sh` over every ported class.

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
- The machine is a shared VM: wall-clock times vary by about 5 to 10 percent between runs (more while other jobs run). For A/B timing of two binaries run them alternately and compare user plus system CPU time (`resource.getrusage(RUSAGE_CHILDREN)`), which is far less sensitive to the other jobs. For before/after comparisons of a change use instruction counts, which are deterministic: `valgrind --tool=callgrind --callgrind-out-file=cg.out target/release/ofsim verify --record R --maps M --until 3000`, then `callgrind_annotate cg.out` (self cost) or `--inclusive=yes`. The release profile keeps line tables, so `--auto=yes` shows the hot lines.
- `Tools/BuildPgo.sh` builds a profile-guided binary into `target/pgo/release/ofsim` (needs `rustup component add llvm-tools`); 0 to 8 percent faster on C081 depending on the run (noisy VM), hashes unaffected. `-C target-cpu=x86-64-v3` was measured and gives nothing (the code is branch and memory bound), so it is not used.

## Benchmark (release profile, one thread, 4-core shared VM)

P7 pass, `ofsim verify` of one record per process. "Before" is the integration branch at `b53d2ee` (the end of the P6-P pass), "after" is the P7 branch; both binaries were run alternately, best of 2, and the rate is ticks per CPU second (user plus system). The peak RSS is that of the process.

| Class | Record | Ticks | Before (tick/s) | After | Speed-up | Replay before (wall tick/s) | Replay after | Peak RSS before to after |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| C000 | dqD3tRRz64 | 11,102 | 633 | 771 | 1.22x | 621 | 716 | 350 to 214 MB |
| C023 | tEJEgWSc | 12,932 | 484 | 631 | 1.30x | 446 | 595 | 627 to 273 MB |
| C040 | HQ4dbm2d | 4,362 | 656 | 792 | 1.21x | 616 | 782 | 251 to 160 MB |
| C049 | FMHzJ8XF | 12,875 | 1,022 | 1,290 | 1.26x | 967 | 1,293 | 318 to 144 MB |
| C059 | 67986TJw | 7,002 | 435 | 515 | 1.18x | 424 | 505 | 375 to 276 MB |
| C081 | cjPSE6enhb | 10,712 | 762 | 986 | 1.29x | 731 | 971 | 283 to 138 MB |
| C088 | cxzGcV5tJm | 9,022 | 464 | 555 | 1.20x | 437 | 538 | 360 to 224 MB |
| all | | 68,007 | 601 | 747 | 1.24x | 571 | 720 | |

Instruction counts (callgrind, first ticks): C081 22.79 G to 19.79 G (3000 ticks), C059 36.54 G to 30.74 G (3000 ticks), C088 39.25 G to 34.80 G (3500 ticks). The CPU-time gain (1.18x to 1.30x) is larger than the instruction gain (13 to 16 percent) because the player tile sets no longer hash: the per-tile position arrays are read next to the tile state, which removes cache misses. Replay output is byte-identical to the previous binary (`Tools/CompareReplayOutput.py` on HQ4dbm2d, 67986TJw, cjPSE6enhb and cxzGcV5tJm: Result.json, Entities.ndjson and every tile frame).

Memory per job: the peak RSS of a single replay is now 138 to 276 MB for the benchmark records (was 251 to 627 MB), so `ofsim batch --jobs 4` needs about 1 GB instead of 2.5 GB. Most of the saving is the hash tables of the tile and border sets (two `HashMap`s per player, each sized for its player's peak) that became two `u32` arrays per map.

### P6-P pass (reference)

Measured against the integration branch at `aa545d3`:

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

Instruction counts over the first 3000 to 5000 ticks fell by 25 to 40 percent (C081 31.5 G to 22.8 G, C000 39.6 G to 28.9 G, C088 86.1 G to 54.2 G for 5000 ticks); wall time falls less because the remaining time is dominated by cache misses in the tile sets and the tile arrays (which the P7 pass then attacked). With the previous measurement against the upstream engine (3.3x to 3.8x on other records) the P6-P engine was roughly 4.3x to 5x upstream single-threaded; upstream was not re-measured, so P7 adds its 1.24x on top.

Snapshots: `ofsim replay` compresses and writes tile frames on 3 background threads (1 per job in `batch`); with `--every 1` on HQ4dbm2d this took 59.8 s before and 33.9 s after, the rest being zstd level 3 itself (frames must stay byte-identical to the upstream dumper, so the level cannot change).

## What changed in the P7 pass

| Change | Where | Effect |
| --- | --- | --- |
| Per-tile position arrays for the players' `tiles` and `border_tiles` sets: the sets of one family partition the map, so one `u32` slot array per family replaces every per-player hash index; the arrays are shared through `TilePositions` and checked against the set's own slot vector, so a stale entry of another player is harmless | `Game/TileSet.rs`, `GameImpl.rs`, `PlayerImpl.rs` | no hashing in `conquer` and the border update; peak RSS -35 to -56 percent; about 3 percent of the instructions and more of the time |
| `OrderedU32Set`, an insertion-ordered `u32` set over a `SlotIndex` (open-addressing table with backward-shift deletion, or the position array); same cursor and compaction semantics as `OrderedSet`, tested against it under random churn | `Compat/OrderedU32Set.rs`, `Game/AttackImpl.rs` | attack border sets: about half the instructions of the `HashMap` index |
| Large-territory sigmoid memoised per tile count (a pure function of the count) | `Configuration/Config/Combat.rs` | the `DetMath` log and exp of `attack_logic` (about 3 percent) are gone after warm-up |
| libm-free `floor`/`ceil` (the baseline x86-64 target has no `roundsd`); exact integer remainder instead of `fmod` in the unit grid | `Compat/JsNumber.rs`, `Game/UnitGrid.rs`, `Core/PseudoRandom.rs` | about 2 percent (defense-post query per conquered tile, every `next_int`) |
| Conquest loop: owner test before the terrain reads, one column computation per border update (`neighbors4_with_x`, `is_border_at`) | `Execution/AttackExecution.rs`, `Game/GameMap.rs`, `Game/GameImpl.rs` | about 2 percent |
| Exactly-sized `TileSet::iter` (one allocation for `collect`) | `Compat/OrderedU32Set.rs` | fewer reallocations when border tiles are copied |
| Bresenham walks of the water smoothing track only the tile index | `Pathfinding/Transformers/SmoothingWaterTransformer.rs` | about 0.6 percent on C023 |
| Rail A* heuristic with the division-free `x`/`y`; bounded water A* with 32-bit division and the libm-free `floor` | `Pathfinding/Algorithms/` | fewer cycles per expanded node (no instruction-count change worth reporting; 64-bit division is the slow one) |
| ARC4: integer accumulation of the six output bytes (exact below 2^48), `u8` indices | `Core/Arc4.rs` | `PseudoRandom::next` of the seedrandom builds about 2 percent |
| Legacy28 `SerialAStar`: `gScore` and `cameFrom` in one lookup-only map (one hash operation per relaxation), neighbours written into a reused buffer instead of a `Vec` per expansion | `Pathfinding/Legacy28/SerialAStar.rs`, `MiniAStar.rs`, `Game/RailNetwork.rs` | C059 -10 percent of the instructions |

Tried and not kept:

- A cache of finished water paths by (source, target, water version): only 7 percent of the queries of C081 and C023 repeat (ships start from different tiles), so the hit rate does not pay for the invalidation risk (the HPA* graph caches edge paths and is swapped on rebuilds).
- An interior fast path (no bounds checks, known neighbour coordinates) in `flood_fill_with_gen`: the instruction count went up by 0.5 percent.
- Incremental cluster maintenance for `PlayerExecution`: not attempted; the V36 territory flood and the border clusters depend on the stamps left by earlier passes, so an incremental version cannot be shown to give identical stamps and orders without re-deriving the whole pass.

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

Callgrind, inclusive, first 3000 to 5000 ticks after the P7 pass; percentages of the run, they overlap.

1. `AttackExecution::tick` 48 to 56 percent (C081, C023, C088): `add_neighbors` 16 to 19 (neighbour walk, `PseudoRandom` draws, attack border set, heap `enqueue`), `GameImpl::conquer` 14 to 18 of which `update_borders` 10 to 12 (five `is_border_at` evaluations per conquered tile), heap `dequeue` 4 to 5 (priority ties force the exact upstream sift order, so the heap cannot be swapped), `attack_logic` 3 to 4, `refresh_to_conquer` 4 to 5 (a re-seeding walk over all border tiles; the draws are order dependent).
2. `PlayerExecution` clusters: 14 percent on C081 and 31 percent on C088. On C088 (v0.36 and later) `remove_clusters_v36` is 22 percent: the scanline territory flood is about 18 percent (visited stamp plus owner test per tile, memory bound) and `calculate_clusters` 8 percent. A different algorithm (or proving that stamps and orders survive an incremental update) is the only way further.
3. Water paths 15 percent on C023, 8 percent on C081, 6 percent on C088: hierarchical A* 4, bounded local A* 3 to 4, `los_smooth` 5 on C023 (the binary search calls `can_see` many times over long lines), trade ship `find_path` about 1.2 M instructions each.
4. Trade ship 10 percent and warship 7 percent on C023 (`hunt_down_trade_ship`, `find_best_target`, `can_attack_player`); train station ticks and `connect_station` about 6 percent on C081.
5. Legacy28 builds (C059): the bidirectional A* is 43 percent of the run (`FastPriorityQueue::poll` 8, hash maps 5, `expand_node` 11); ARC4 draws about 4 percent.
6. Attack border sets (`OrderedU32Set` with the hash index) about 4 percent: their tiles are not exclusive between attacks, so they cannot use a shared position array.

## Batch

`ofsim batch --jobs N` sorts the records by size (largest first) and every worker takes the next record from a shared counter, so a long game never starts last. Peak memory is per job (about 140 to 280 MB for the benchmark records, growing with the number of owned tiles; 250 to 630 MB before the P7 pass), so 4 jobs need about 1 GB.

Scaling on the 4-core VM before the P7 pass (`Tools/Bench.py --scale`, `batch --verify-only` over the 7 benchmark records, about 136 s of work):

| `--jobs` | Wall | Ticks/s (all jobs) | Speed-up | Peak RSS of one job process set |
| --- | --- | --- | --- | --- |
| 1 | 136.2 s | 499 | 1.0x | 678 MB |
| 2 | 67.9 s | 1,002 | 2.0x | 968 MB |
| 4 | 53.8 s | 1,265 | 2.5x | 1,554 MB |

Seven records of unequal length cannot fill 4 workers (ideal 36 s), and the VM itself does not scale linearly: four copies of the same single-thread run take 13.9 s instead of 9.7 s (about 2.8x for 4 processes) because of shared memory bandwidth and cache. With a long list of records `--jobs 4` therefore gives about 2.8x here; the code has no shared state between jobs. Lower memory traffic per tick (smaller tile sets, see the list above) is what would improve the parallel scaling further.
