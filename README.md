# OfSimRust

Unofficial replay simulator for OpenFront game records.

**Not affiliated with or endorsed by OpenFront Inc.** © OpenFront and Contributors (ported game logic); Rust port © OfSimRust contributors.

Japanese version: [README.ja.md](README.ja.md)

OfSimRust (binary: `ofsim`) is a fast, deterministic Rust re-implementation of the [OpenFront](https://github.com/openfrontio/OpenFrontIO) game simulation. Given a game record from the public OpenFront API, it replays the game tick by tick and reproduces the exact game state of the upstream engine build that played it, verified against the state hashes stored in the record. Use it to rebuild territory, troops, units, attacks and alliances at any tick for research or tooling.

Status: under construction. Supported builds: the baseline v0.34.19 to v0.34.23 (class C000) and the v0.34 builds of classes C001 to C012 (v0.34.0 to v0.34.18, v0.34.24, v0.34.25); other builds fail with `build <commit> (class Cnnn) not ported yet`. See [Docs/Plan.md](Docs/Plan.md) and [Docs/BuildPlan.md](Docs/BuildPlan.md) for the per-class status.

## Setup

```sh
git clone https://github.com/konbraphat51/OfSimRust
cd OfSimRust
cargo build --release
```

Map files are not bundled. Clone the upstream repository once; `ofsim` reads the map files of each record's build from it:

```sh
git clone https://github.com/openfrontio/OpenFrontIO Cache/OpenFrontIO
```

## Usage

```sh
# Verify a record against its stored hashes. --maps is the map directory of the baseline build (v0.34.22);
# for builds whose map files differ, ofsim reads them from --upstream (default Cache/OpenFrontIO) with git
# and caches them in Cache/Maps/<commit12>/.
cargo run --release --bin ofsim -- verify --record game.json.gz --maps <OpenFrontIO>/resources/maps

# Replay and write snapshots every 100 ticks
cargo run --release --bin ofsim -- replay --record game.json.gz --maps <OpenFrontIO>/resources/maps --out Out/game --every 100
```

## Development

- Checks: `cargo fmt --all && cargo clippy --all-targets && cargo test && Tools/CheckHeaders.sh`
- Licenses of dependencies: `cargo deny check licenses` (needs cargo-deny; config in `deny.toml`)
- Design and porting rules: [Docs/Architecture.md](Docs/Architecture.md)
- Differential debugging against the upstream engine (`Reference/Setup.sh`, `TraceDumper.ts`, `TraceDiff.ts`) and fetching records (`Tools/FetchRecords.py`): [Reference/README.md](Reference/README.md)

## License

AGPL-3.0-only, because the engine is a port of the AGPL-3.0 OpenFrontIO source. The upstream additional terms (Section 7) apply: the notice "© OpenFront and Contributors" is preserved, and this is an unofficial port that does not use "OpenFront" as its title or imply endorsement. Map files and game records are not included; the licenses of your upstream map copy apply to it.

See [LICENSE](LICENSE), [NOTICE](NOTICE) and [Docs/Licensing.md](Docs/Licensing.md). This is not legal advice.
