// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/TribeNames.ts: tribe name pools per map.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The themes (`resources/tribeNameThemes.json`) and the per-map theme lists and custom tribes
//! (`Maps.gen.ts`) are generated into `data/TribeNames.json` by `Tools/GenEngineData.mts`; the
//! pool sizes decide how many `PseudoRandom` values a tribe name consumes.

use crate::Builds::Rules::{Change, Rules};
use crate::Game::Game::GameMapType;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

/// A custom tribe of a map.
#[derive(Clone, Debug, Deserialize)]
pub struct CustomTribe {
    /// Tribe name.
    pub name: String,
    /// Fixed spawn coordinates, if any.
    pub coordinates: Option<[i64; 2]>,
}

/// Name pools of a game.
#[derive(Clone, Debug)]
pub struct TribeNameData {
    /// Prefix pool.
    pub prefixes: Vec<String>,
    /// Suffix pool.
    pub suffixes: Vec<String>,
    /// Custom tribes of the map, when it has any.
    pub custom_tribes: Option<Vec<CustomTribe>>,
}

#[derive(Deserialize)]
struct Theme {
    prefixes: Vec<String>,
    suffixes: Vec<String>,
}

#[derive(Deserialize)]
struct MapInfo {
    themes: Option<Vec<String>>,
    #[serde(rename = "customTribes")]
    custom_tribes: Option<Vec<CustomTribe>>,
}

#[derive(Deserialize)]
struct Tables {
    themes: HashMap<String, Theme>,
    maps: HashMap<String, MapInfo>,
}

fn tables() -> &'static Tables {
    static TABLES: OnceLock<Tables> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/TribeNames.json"))
            .expect("TribeNames.json is valid")
    })
}

/// Per-map theme list and custom tribes of the build (the data file is the baseline's).
fn map_entry(map: GameMapType, rules: &Rules) -> (Option<Vec<String>>, Option<Vec<CustomTribe>>) {
    let name = map.as_str();
    if name == "Channel Islands" && !rules.has(Change::ChannelIslandsTribes5564) {
        return (Some(vec!["north_america".to_string()]), None);
    }
    if rules.has(Change::NewMapThemes5547) {
        let theme = match name {
            "Canary Islands" => Some("africa"),
            "New Zealand" => Some("oceania"),
            "Pulicat Lake" => Some("asia"),
            _ => None,
        };
        if let Some(theme) = theme {
            return (Some(vec![theme.to_string()]), None);
        }
    }
    match tables().maps.get(name) {
        Some(info) => (info.themes.clone(), info.custom_tribes.clone()),
        None => (None, None),
    }
}

/// `resolveTribeNameData(mapType)`.
pub fn resolve_tribe_name_data(map: GameMapType, rules: &Rules) -> TribeNameData {
    let t = tables();
    let default_theme = t
        .themes
        .get("default")
        .expect("Default theme is missing from tribeNameThemes.json");
    let (themes, custom_tribes) = map_entry(map, rules);
    let theme_names: Vec<String> = match themes {
        Some(v) if !v.is_empty() => v,
        _ => vec!["default".to_string()],
    };
    let mut prefixes = Vec::new();
    let mut suffixes = Vec::new();
    for name in &theme_names {
        let Some(theme) = t.themes.get(name) else {
            continue;
        };
        prefixes.extend(theme.prefixes.iter().cloned());
        suffixes.extend(theme.suffixes.iter().cloned());
    }
    if prefixes.is_empty() || suffixes.is_empty() {
        prefixes.extend(default_theme.prefixes.iter().cloned());
        suffixes.extend(default_theme.suffixes.iter().cloned());
    }
    TribeNameData {
        prefixes,
        suffixes,
        custom_tribes: custom_tribes.filter(|c| !c.is_empty()),
    }
}
