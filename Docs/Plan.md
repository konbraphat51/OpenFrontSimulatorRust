# Implementation plan

Japanese version: [Plan.ja.md](Plan.ja.md)

Responsibility of this file: the phases of the port, what each delivers, and its status. Update the status column when a phase lands.

The first milestone is hash-exact replay of the baseline build v0.34.22 (`4e837520e886`). Every later build is a diff against it.

| Phase | Deliverable | Done when | Status |
| --- | --- | --- | --- |
| P0 | Workspace, docs, licence, submodule | `cargo build` passes | done |
| P1a | `Compat`, `Core` (PseudoRandom, DetMath, Util), `GameMap`, `TileSet`, `Configuration` of the baseline | unit tests match node-computed values | done (V8 fdlibm too) |
| P1b | `ofsim-record`, `ofsim-map` | baseline records and maps load; turns expand like `decompressGameRecord` | done |
| P1c | Reference tracer and record fetcher | trace of a baseline record written from the upstream engine | done |
| P2 | Game core: `GameImpl`, `PlayerImpl`, `UnitImpl`, attacks, alliances, `Executor`, `GameRunner`, spawn, tribes, attacks, troop growth, win check | hashes match on baseline records through the end of the spawn phase, then through the first unit build | |
| P3 | Units: construction, structures, ports and trade ships, transport ships, warships, rails and trains, nukes, MIRV, SAM, shells, water nukes, pathfinding | every baseline record without nations passes `ofsim verify` | |
| P4 | Nation AI (`NationExecution` and behaviours, `NationCreation`) | every baseline record passes | |
| P5 | `ofsim replay` snapshots, `batch`, profiling | output identical to the upstream dumper on sample games; throughput measured | |
| P6 | Build rules for every build from 2025-10-01 (v0.25.21) to the newest, newest first along our data, then backwards | each build passes on sampled records | |
