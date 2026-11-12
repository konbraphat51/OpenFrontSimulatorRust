# Licensing

Japanese version: [Licensing.ja.md](Licensing.ja.md)

This is not legal advice. It describes how this repository relates to upstream OpenFront licensing and what we believe follows from it. Check the upstream `LICENSE`, `LICENSING.md`, `LICENSE-ASSETS` and `CREDITS.md` yourself.

## Why AGPL-3.0-only

OfSimRust is a translation of OpenFrontIO's `src/core` into Rust, so it is a derivative work of that code. Upstream code since 2025-09-04 is AGPL-3.0-only (copyright OpenFront Inc. and contributors); earlier upstream phases were MIT and then GPL. Every build this simulator supports (2025-10-01 onward) is AGPL-3.0-only, so no supported build predates the switch, and the whole repository is AGPL-3.0-only. The root `LICENSE` is upstream's `LICENSE`, including its additional terms.

## Section 7 additional terms

Upstream appended AGPL Section 7 terms (since 2025-12-16). How this repository meets them:

| Term | Requirement | How it is met |
| --- | --- | --- |
| 7(b) | Preserve the notice "(c) OpenFront and Contributors" in reasonably visible locations | `NOTICE`, the README, and the `ofsim --version` / `--help` output; ported files name their upstream source in the header |
| 7(c) | Modified versions must not misrepresent their origin, use "OpenFront" as their primary title or imply official endorsement | The project is titled OfSimRust (binary `ofsim`), described as an unofficial port, with a non-affiliation statement in the README, `NOTICE` and the CLI |


## Obligations of users

- Redistribution (source or binary) must keep `LICENSE` and `NOTICE`, the file headers, and provide the corresponding source under AGPL-3.0-only.
- Section 13: if you run a modified ofsim as a network service that users interact with, you must offer those users the corresponding source of your modified version.
- Modifications must be marked as such (Section 5(a)) and must not use "OpenFront" as the primary title or imply endorsement (7(c)).
- Contributions are accepted under AGPL-3.0-only (inbound equals outbound). New source files carry the header described in [Architecture.md](Architecture.md#licensing).

## Map data and assets

This repository includes no map files, assets or proprietary content. You obtain maps from an upstream checkout (`resources/maps`) and the licenses of that copy apply to you:

- `/resources` is CC BY-SA 4.0, attribution "OpenFront" / "OpenFront Inc.".
- Map data additionally derives from OpenStreetMap (ODbL), Natural Earth, Bedmap3 (CC BY 4.0), Copernicus DEM and others, as listed in upstream `CREDITS.md`.
- `/proprietary` is All Rights Reserved; ofsim does not read it.

## Outputs

Snapshots (`Tiles/*.u16.zst`, `Entities.ndjson`) and derived datasets are computed from map data and game records.

- When you publish them, credit "OpenFront". If they contain map-tile data (terrain, land or water masks, tile ownership on a map), CC BY-SA 4.0 share-alike and ODbL implications of the underlying maps may apply; check them before choosing a license for the data.
- Never publish usernames. Game records from the public API contain player usernames; this repository never stores or redistributes records or names, and `Entities.ndjson` contains no human names.
- Game records come from the public OpenFront API, which states no explicit license. Do not redistribute them.

## Third-party code

- V8 / Sun fdlibm math ported for exact JavaScript results: [ThirdPartyNotices/V8.md](../ThirdPartyNotices/V8.md) (BSD-3-Clause and the fdlibm notice).
- seedrandom (ARC4 generator ported for the era S builds): [ThirdPartyNotices/Seedrandom.md](../ThirdPartyNotices/Seedrandom.md) (MIT).
- Rust crates keep their own licenses. `deny.toml` allows only AGPL-compatible ones; run `cargo deny check licenses`.

## File headers

Every `.rs` and `.ts` source file starts with `// SPDX-License-Identifier: AGPL-3.0-only`; `Tools/CheckHeaders.sh` enforces it.
