//! Port of src/core/game/TerrainMapLoader.ts (`loadTerrainMap`, `genTerrainFromBin`) onto plain data.

use crate::directory_key_of;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Upstream `GameMapSize`: selects which binaries are the main and the mini map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MapSize {
    /// Main map is `map.bin`, mini map is `map4x.bin`.
    Normal,
    /// Main map is `map4x.bin`, mini map is `map16x.bin`; coordinates are halved.
    Compact,
}

impl MapSize {
    /// Parses the wire value of `GameMapSize` ("Normal" or "Compact").
    pub fn from_wire(name: &str) -> Result<Self> {
        match name {
            "Normal" => Ok(Self::Normal),
            "Compact" => Ok(Self::Compact),
            other => bail!("unknown map size: {other}"),
        }
    }
}

/// Upstream `MapMetadata`: dimensions of one binary.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapMetadata {
    /// Width in tiles.
    pub width: u32,
    /// Height in tiles.
    pub height: u32,
    /// Number of land tiles.
    pub num_land_tiles: u32,
}

/// Upstream `Nation` (and `AdditionalNation`, which has the same shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nation {
    /// Spawn coordinates (x, y); absent for nations placed by the engine.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coordinates: Option<[i64; 2]>,
    /// Flag name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flag: Option<String>,
    /// Display name.
    pub name: String,
}

/// Upstream `AdditionalNation`.
pub type AdditionalNation = Nation;

/// Upstream `SpawnArea`: a rectangle where a team may spawn.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpawnArea {
    /// Left edge.
    pub x: i64,
    /// Top edge.
    pub y: i64,
    /// Width.
    pub width: i64,
    /// Height.
    pub height: i64,
}

/// Upstream `MapLayer` (render layer; validated like upstream, not used by the simulation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapLayer {
    /// Layer id.
    pub id: String,
    /// "land" or "water".
    pub placement: String,
    /// Destroyed by nukes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nukeable: Option<bool>,
    /// Default opacity.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alpha: Option<f64>,
}

/// Upstream `MapManifest` (`manifest.json`); other keys (themes, categories, ...) are ignored.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapManifest {
    /// Map name.
    pub name: String,
    /// Metadata of `map.bin`.
    pub map: MapMetadata,
    /// Metadata of `map4x.bin`.
    pub map4x: MapMetadata,
    /// Metadata of `map16x.bin`.
    pub map16x: MapMetadata,
    /// Nations with coordinates.
    pub nations: Vec<Nation>,
    /// Fallback nation names.
    #[serde(
        default,
        rename = "additionalNations",
        skip_serializing_if = "Option::is_none"
    )]
    pub additional_nations: Option<Vec<AdditionalNation>>,
    /// Team spawn rectangles keyed by team count.
    #[serde(
        default,
        rename = "teamGameSpawnAreas",
        skip_serializing_if = "Option::is_none"
    )]
    pub team_game_spawn_areas: Option<BTreeMap<String, Vec<SpawnArea>>>,
    /// Render layers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layers: Option<Vec<MapLayer>>,
}

/// One terrain grid exactly as upstream hands it to `GameMapImpl`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TerrainMap {
    /// Width in tiles.
    pub width: u32,
    /// Height in tiles.
    pub height: u32,
    /// Land tile count from the manifest.
    pub num_land_tiles: u32,
    /// Raw terrain bytes (`width * height`), one per tile, undecoded as in the file.
    pub data: Vec<u8>,
}

impl TerrainMap {
    /// Port of `genTerrainFromBin`: checks the buffer length against the metadata.
    pub fn from_bin(metadata: &MapMetadata, data: Vec<u8>) -> Result<Self> {
        let expected = metadata.width as u64 * metadata.height as u64;
        if data.len() as u64 != expected {
            bail!(
                "Invalid data: buffer size {} incorrect for {}x{} terrain plus 4 bytes for dimensions.",
                data.len(),
                metadata.width,
                metadata.height
            );
        }
        Ok(Self {
            width: metadata.width,
            height: metadata.height,
            num_land_tiles: metadata.num_land_tiles,
            data,
        })
    }
}

/// Upstream `TerrainMapData` without render layers and images.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainMapData {
    /// Nations (coordinates halved for compact maps).
    pub nations: Vec<Nation>,
    /// Fallback nations, empty when the manifest has none.
    pub additional_nations: Vec<AdditionalNation>,
    /// Main game map.
    pub game_map: TerrainMap,
    /// Mini map used for pathfinding.
    pub mini_game_map: TerrainMap,
    /// Team spawn areas (scaled for compact maps).
    pub team_game_spawn_areas: Option<BTreeMap<String, Vec<SpawnArea>>>,
    /// Render layers from the manifest.
    pub layers: Option<Vec<MapLayer>>,
}

/// Namespace of the loader entry points.
pub struct MapData;

impl MapData {
    /// Directory holding one map's files: `<maps_root>/<directory key>`.
    pub fn directory_of(maps_root: &Path, wire_name: &str) -> Result<PathBuf> {
        Ok(maps_root.join(directory_key_of(wire_name)?))
    }

    /// Port of `loadTerrainMap`; `map` is the `GameMapType` wire name, `size` the config's `gameMapSize`.
    pub fn load(maps_root: &Path, map: &str, size: MapSize) -> Result<TerrainMapData> {
        let directory = Self::directory_of(maps_root, map)?;
        let read = |name: &str| -> Result<Vec<u8>> {
            let path = directory.join(name);
            std::fs::read(&path).with_context(|| format!("reading {}", path.display()))
        };
        let manifest_bytes = read("manifest.json")?;
        let manifest: MapManifest = serde_json::from_slice(&manifest_bytes)
            .with_context(|| format!("parsing manifest of {map}"))?;

        let (game_map, mini_game_map) = match size {
            MapSize::Normal => (
                TerrainMap::from_bin(&manifest.map, read("map.bin")?)?,
                TerrainMap::from_bin(&manifest.map4x, read("map4x.bin")?)?,
            ),
            MapSize::Compact => (
                TerrainMap::from_bin(&manifest.map4x, read("map4x.bin")?)?,
                TerrainMap::from_bin(&manifest.map16x, read("map16x.bin")?)?,
            ),
        };

        let mut nations = manifest.nations;
        let mut additional_nations = manifest.additional_nations.unwrap_or_default();
        let mut team_game_spawn_areas = manifest.team_game_spawn_areas;
        if size == MapSize::Compact {
            for nation in nations.iter_mut().chain(additional_nations.iter_mut()) {
                if let Some([x, y]) = nation.coordinates {
                    nation.coordinates = Some([x.div_euclid(2), y.div_euclid(2)]);
                }
            }
            if let Some(areas) = team_game_spawn_areas.as_mut() {
                for list in areas.values_mut() {
                    for area in list.iter_mut() {
                        *area = SpawnArea {
                            x: area.x.div_euclid(2),
                            y: area.y.div_euclid(2),
                            width: 1.max(area.width.div_euclid(2)),
                            height: 1.max(area.height.div_euclid(2)),
                        };
                    }
                }
            }
        }

        if let Some(layers) = &manifest.layers {
            for layer in layers {
                if layer.placement != "land" && layer.placement != "water" {
                    bail!(
                        "Map {map}: layer \"{}\" has invalid placement \"{}\" (must be \"land\" or \"water\")",
                        layer.id,
                        layer.placement
                    );
                }
                if let Some(alpha) = layer.alpha
                    && (!alpha.is_finite() || !(0.0..=1.0).contains(&alpha))
                {
                    bail!(
                        "Map {map}: layer \"{}\" has invalid alpha {alpha} (must be a finite number between 0 and 1)",
                        layer.id
                    );
                }
            }
        }

        Ok(TerrainMapData {
            nations,
            additional_nations,
            game_map,
            mini_game_map,
            team_game_spawn_areas,
            layers: manifest.layers,
        })
    }
}
