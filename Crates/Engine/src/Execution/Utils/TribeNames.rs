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
    /// Maps that are new or changed as of v0.36.0 (full entries).
    #[serde(rename = "mapsV36")]
    maps_v36: HashMap<String, MapInfo>,
    /// Maps changed again as of v0.36.1.
    #[serde(rename = "mapsV361")]
    maps_v361: HashMap<String, MapInfo>,
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
    let t = tables();
    let newer = if rules.has(Change::TribeMapData361) {
        t.maps_v361.get(name)
    } else {
        None
    };
    let newer = newer.or_else(|| {
        rules
            .has(Change::TribeMapData36)
            .then(|| t.maps_v36.get(name))
            .flatten()
    });
    match newer.or_else(|| t.maps.get(name)) {
        Some(info) => (info.themes.clone(), info.custom_tribes.clone()),
        None => (None, None),
    }
}

#[derive(Deserialize)]
struct LegacyNames {
    prefixes: Vec<String>,
    suffixes: Vec<String>,
}

/// The single prefix and suffix pool of builds before PR 4647 (`TRIBE_NAME_PREFIXES`, `TRIBE_NAME_SUFFIXES`).
fn legacy_names() -> &'static LegacyNames {
    static NAMES: OnceLock<LegacyNames> = OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/TribeNamesLegacy.json"))
            .expect("TribeNamesLegacy.json is valid")
    })
}

/// `BOT_NAME_PREFIXES` and `BOT_NAME_SUFFIXES` of `BotNames.ts` (builds v0.30.0-beta2 and older).
fn bot_names_v29() -> &'static LegacyNames {
    static NAMES: OnceLock<LegacyNames> = OnceLock::new();
    NAMES.get_or_init(|| {
        serde_json::from_str(include_str!("../../../data/BotNamesV29.json"))
            .expect("BotNamesV29.json is valid")
    })
}

/// `resolveTribeNameData(mapType)`.
pub fn resolve_tribe_name_data(map: GameMapType, rules: &Rules) -> TribeNameData {
    if !rules.has(Change::TribeNamesRevised30) {
        let names = bot_names_v29();
        return TribeNameData {
            prefixes: names.prefixes.clone(),
            suffixes: names.suffixes.clone(),
            custom_tribes: None,
        };
    }
    if !rules.has(Change::TribeNameThemes4647) {
        let names = legacy_names();
        return TribeNameData {
            prefixes: names.prefixes.clone(),
            suffixes: names.suffixes.clone(),
            custom_tribes: None,
        };
    }
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
