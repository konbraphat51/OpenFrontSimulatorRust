# Nation behaviours (N2): structures, nukes, MIRVs

Japanese version: [NationBehaviors.ja.md](NationBehaviors.ja.md)

Responsibility of this file: what the three nation behaviours of `Crates/Engine/src/Execution/Nation/` port, how they plug into `NationExecution`, and how they are verified.

## Files

| Rust file | Upstream | Called by `NationExecution` as |
| --- | --- | --- |
| `NationStructureBehavior.rs` | `execution/nation/NationStructureBehavior.ts` | `handle_structures(game) -> bool` |
| `NationNukeBehavior.rs` | `execution/nation/NationNukeBehavior.ts` | `maybe_send_nuke(game)`, `find_best_nuke_target(game)` |
| `NationMIRVBehavior.rs` | `execution/nation/NationMIRVBehavior.ts` | `consider_mirv(game) -> bool` |
| `NationBehaviorPorts.rs` | (none) | traits and the decision log, see below |

All three take the nation's shared `SharedRandom` (upstream passes one `PseudoRandom` to every behaviour, so draws interleave) and a `&GameImpl` in the constructor, and `&mut GameImpl` in every method (see `NationApi` of N1).

## Design points

- **Value functions** (`SpawnValue` in the structure behaviour) are created once per placement search, like upstream's closures. Creating one can draw from the random stream (`shouldUseConnectivityScore`, the SAM coverage roll), so the order of creation versus tile sampling is part of the contract.
- **`recentMirvTargets`** is a static of the upstream class. Here it is `GameImpl::recent_mirv_targets`, so replays in one process do not leak into each other (player ids are random per game upstream, so this is equivalent for one game per process).
- **Collaborators** are held as `Rc<RefCell<dyn EmojiPort>>` and `Rc<RefCell<dyn AttackPort>>` (`NationBehaviorPorts.rs`). `NationEmojiBehavior` (N1) and `AiAttackBehavior` (A2) implement them there; N1 passes its `Shared<..>` values and Rust coerces them. The tests pass doubles.
- **Created executions** (`ConstructionExecution`, `UpgradeStructureExecution`, `NukeExecution`, `MIRVExecution`) belong to A3 and A4. `DecisionLog` can switch a behaviour to dry-run mode (`behaviour.log.set_dry_run(true)`): it then records the decision instead of queueing the execution, which is how the goldens observe it.
- **Train network**: the connectivity score reads `RailNetwork::station_clusters()` (unit, cluster id). The stub returns an empty list (no station can exist yet); A3's `RailNetworkImpl` must return every station in `stationManager().getAll()` order with a cluster id that is equal for stations of one cluster and `None` for isolated stations.
- `GameImpl::shared_water_components(player)` owns the `SharedWaterCache` and implements `SharedWaterView`.

## Verification

`Crates/Engine/tests/NationBehaviorGoldenTest.rs` runs every `tests/Golden/NationBehaviors/<name>.spec.json` (a scripted scenario: players, conquered squares, built structures, relations, attacks, ticks, behaviour calls) on the Rust engine and compares each step with `<name>.golden.ndjson`, which the upstream engine produced through `Reference/NationBehaviorDump.ts`. A step compares the return value, the queued executions (type, tile or unit, wait ticks) and the game state hash. The test is skipped when the local cache (`Cache/Worktrees/4e837520e886/resources/maps`) is missing.

Regenerating a golden (from the baseline worktree, after copying `Reference/*.ts` into `ofsim-reference-n2/`):

```sh
npx tsx ofsim-reference-n2/NationBehaviorDump.ts <spec.json> <name>.golden.ndjson
```

The scenarios cover all four difficulties, FFA and team mode, high starting gold, incoming land and boat attacks (defense posts), dense territories (upgrades), the crown and strongest-team targets, SAM trajectories, the atom-bomb salvo against SAMs, the MIRV counter, victory-denial and steamroll rules.
