# OpenFront Simulator (Rust)

Japanese version: [README.ja.md](README.ja.md)

A fast, deterministic Rust re-implementation of the [OpenFront](https://github.com/openfrontio/OpenFrontIO) game simulation. Given a game record from the public OpenFront API, it replays the game tick by tick and reproduces the exact game state of the upstream engine build that played it, verified against the state hashes stored in the record. Use it to rebuild territory, troops, units, attacks and alliances at any tick for research or tooling.

Status: under construction. See [Docs/Plan.md](Docs/Plan.md) for which builds are supported.

## Setup

```sh
git clone https://github.com/konbraphat51/OpenFrontSimulatorRust
cd OpenFrontSimulatorRust
cargo build --release
```

Map files are not bundled. Clone the upstream repository once; `ofsim` reads the map files of each record's build from it:

```sh
git clone https://github.com/openfrontio/OpenFrontIO Cache/OpenFrontIO
```

## Usage

```sh
# Verify a record against its stored hashes
cargo run --release --bin ofsim -- verify --record game.json.gz --maps <OpenFrontIO>/resources/maps

# Replay and write snapshots every 100 ticks
cargo run --release --bin ofsim -- replay --record game.json.gz --maps <OpenFrontIO>/resources/maps --out Out/game --every 100
```

## Development

- Checks: `cargo fmt --all && cargo clippy --all-targets && cargo test`
- Design and porting rules: [Docs/Architecture.md](Docs/Architecture.md)
- Differential debugging against the upstream engine (`Reference/Setup.sh`, `TraceDumper.ts`, `TraceDiff.ts`) and fetching records (`Tools/FetchRecords.py`): [Reference/README.md](Reference/README.md)

## License

AGPL-3.0-only, because the engine is a port of the AGPL-3.0 OpenFrontIO source. See [LICENSE](LICENSE).
