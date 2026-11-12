//! Integration test: the Rust record and map layers reproduce the upstream digests in `Golden/*.json`.
//!
//! Needs the local cache (`Cache/Records/<commit>`, baseline worktree maps); skipped when it is absent.
//! Regenerate the goldens with `Golden/GoldenDigests.ts` (see its header).

#![allow(non_snake_case)]

use openfront_map::{MapData, MapSize};
use openfront_record::Canonical::canonical_json;
use openfront_record::Records::GameRecord;
use openfront_record::StartInfo::GameStartInfo;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

const BASELINE: &str = "4e837520e886";

/// Finds the simulator `Cache` directory by walking up from the crate (works from the main checkout and from worktrees).
fn find_cache() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OFSIM_CACHE") {
        return Some(PathBuf::from(path));
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|ancestor| {
            let nested = ancestor.join("Cache");
            if nested.join("Records").is_dir() {
                Some(nested)
            } else if ancestor.join("Records").is_dir() && ancestor.join("Worktrees").is_dir() {
                Some(ancestor.to_path_buf())
            } else {
                None
            }
        })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn digest_of<T: serde::Serialize>(value: &T) -> String {
    sha256_hex(canonical_json(&serde_json::to_value(value).unwrap()).as_bytes())
}

fn golden(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/Golden")
        .join(name);
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

fn record_files(cache: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(cache.join("Records").join(BASELINE))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.to_string_lossy().ends_with(".json.gz"))
        .collect();
    files.sort();
    files
}

fn maps_root(cache: &Path) -> PathBuf {
    cache
        .join("Worktrees")
        .join(BASELINE)
        .join("resources/maps")
}

#[test]
fn records_match_upstream_digests() {
    let Some(cache) = find_cache() else {
        eprintln!("skipped: no Cache/Records");
        return;
    };
    let expected = golden("Records.json");
    let files = record_files(&cache);
    assert_eq!(files.len(), expected.as_object().unwrap().len());
    let mut unknown = 0;
    for file in files {
        let mut record = GameRecord::load(&file).unwrap();
        let sparse = record.turns.len();
        record.decompress();
        let id = record.info.game_id.clone();
        let want = &expected[&id];
        assert_eq!(want["gitCommit"], json!(record.git_commit), "{id}");
        assert_eq!(want["gameMap"], json!(record.info.config.game_map), "{id}");
        assert_eq!(
            want["gameMapSize"],
            json!(record.info.config.game_map_size.as_str()),
            "{id}"
        );
        assert_eq!(want["numTurns"], json!(record.info.num_turns), "{id}");
        assert_eq!(want["expandedTurns"], json!(record.turns.len()), "{id}");
        assert_eq!(want["sparseTurns"], json!(sparse), "{id}");
        assert_eq!(
            want["hashes"],
            json!(record.turns.iter().filter(|t| t.hash.is_some()).count()),
            "{id}"
        );
        assert_eq!(
            want["turnsSha256"],
            json!(digest_of(&record.turns)),
            "turns of {id}"
        );

        let mut counts: BTreeMap<String, u64> = BTreeMap::new();
        for intent in record.turns.iter().flat_map(|t| &t.intents) {
            if intent.intent.is_unknown() {
                unknown += 1;
            }
            *counts
                .entry(intent.intent.type_name().to_string())
                .or_default() += 1;
        }
        assert_eq!(want["intentCounts"], json!(counts), "{id}");

        let start = GameStartInfo::from_record(&record);
        assert_eq!(want["players"], json!(start.players.len()), "{id}");
        assert_eq!(
            want["tribes"],
            json!(start.tribes.as_ref().map_or(0, Vec::len)),
            "{id}"
        );
        assert_eq!(
            want["startInfoSha256"],
            json!(digest_of(&start)),
            "start info of {id}"
        );
    }
    assert_eq!(
        unknown, 0,
        "every baseline intent must deserialize into a known variant"
    );
}

#[test]
fn typed_records_round_trip_losslessly() {
    let Some(cache) = find_cache() else {
        eprintln!("skipped: no Cache/Records");
        return;
    };
    for file in record_files(&cache) {
        let bytes = std::fs::read(&file).unwrap();
        let mut text = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&bytes[..]), &mut text)
            .unwrap();
        let raw: Value = serde_json::from_slice(&text).unwrap();
        let record = GameRecord::from_bytes(&bytes).unwrap();
        assert_eq!(
            canonical_json(&raw),
            canonical_json(&serde_json::to_value(&record).unwrap()),
            "{}",
            file.display()
        );
    }
}

#[test]
fn maps_match_upstream_digests() {
    let Some(cache) = find_cache() else {
        eprintln!("skipped: no Cache/Records");
        return;
    };
    let root = maps_root(&cache);
    let expected = golden("Maps.json");
    for (key, want) in expected.as_object().unwrap() {
        let (name, size) = key.split_once('|').unwrap();
        let size = MapSize::from_wire(size).unwrap();
        let data = MapData::load(&root, name, size).unwrap();
        for (label, grid) in [("map", &data.game_map), ("mini", &data.mini_game_map)] {
            let w = &want[label];
            assert_eq!(w["width"], json!(grid.width), "{key} {label}");
            assert_eq!(w["height"], json!(grid.height), "{key} {label}");
            assert_eq!(
                w["numLandTiles"],
                json!(grid.num_land_tiles),
                "{key} {label}"
            );
            assert_eq!(
                w["terrainSha256"],
                json!(sha256_hex(&grid.data)),
                "{key} {label}"
            );
        }
        assert_eq!(want["nations"], json!(data.nations.len()), "{key}");
        assert_eq!(
            want["nationsSha256"],
            json!(digest_of(&data.nations)),
            "{key} nations"
        );
        assert_eq!(
            want["additionalNationsSha256"],
            json!(digest_of(&data.additional_nations)),
            "{key}"
        );
        assert_eq!(
            want["teamSpawnAreasSha256"],
            json!(digest_of(&data.team_game_spawn_areas)),
            "{key}"
        );
    }
}
