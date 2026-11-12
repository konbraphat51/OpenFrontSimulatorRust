//! OpenFront map resources: terrain binaries, mini maps and manifests (nations, spawn areas).

#![allow(non_snake_case)]

pub mod MapTable;
pub mod TerrainMapLoader;

pub use TerrainMapLoader::{
    AdditionalNation, MapData, MapLayer, MapManifest, MapMetadata, MapSize, Nation, SpawnArea,
    TerrainMap, TerrainMapData,
};

use anyhow::{Result, anyhow};

/// Directory key of a map given its wire name (`GameMapType` value, for example "Black Sea" gives "blacksea").
///
/// Mirrors the lookup of the OpenFrontDs `FileSystemMapLoader`: find the enum key whose value is the name, lowercase it.
pub fn directory_key_of(wire_name: &str) -> Result<String> {
    MapTable::MAP_TABLE
        .iter()
        .find(|(_, value)| *value == wire_name)
        .map(|(key, _)| key.to_lowercase())
        .ok_or_else(|| anyhow!("unknown map: {wire_name}"))
}

#[cfg(test)]
mod tests {
    use super::directory_key_of;

    #[test]
    fn directory_keys_follow_the_enum_key() {
        assert_eq!(directory_key_of("Black Sea").unwrap(), "blacksea");
        assert_eq!(directory_key_of("Hecate Strait").unwrap(), "hecatestrait");
        assert_eq!(directory_key_of("Tourney 2 Teams").unwrap(), "tourney1");
        assert_eq!(
            directory_key_of("Gulf of St. Lawrence").unwrap(),
            "gulfofstlawrence"
        );
        assert!(directory_key_of("Nope").is_err());
    }
}
