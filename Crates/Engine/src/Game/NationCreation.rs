// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/NationCreation.ts: the nations of a game and their generated names.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The name tables (`NAME_TEMPLATES`, `NOUNS`) are generated from upstream into
//! `data/NationNames.json` by `Tools/GenEngineData.mts`; the draw sequence of `PseudoRandom`
//! (and therefore every later `nextID()`) depends on their exact sizes.

use super::Game::{
    Cell, GameMapSize, GameMode, GameType, Nation, PlayerInfo, PlayerType, TeamCountConfig,
};
use crate::Configuration::Config::{GameConfig, NationsSetting};
use crate::Core::PseudoRandom::PseudoRandom;
use ofsim_map::Nation as ManifestNation;
use serde::Deserialize;
use std::collections::HashSet;
use std::sync::OnceLock;

const PLURAL_MARK: &str = "$PLURAL";
const NOUN_MARK: &str = "$NOUN";

#[derive(Deserialize)]
struct NameTables {
    templates: Vec<Vec<String>>,
    nouns: Vec<String>,
}

fn tables() -> &'static NameTables {
    static TABLES: OnceLock<NameTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/NationNames.json"))
            .expect("NationNames.json is valid")
    })
}

fn to_nation(n: &ManifestNation, random: &mut PseudoRandom) -> Nation {
    Nation {
        spawn_cell: n.coordinates.map(|c| Cell::new(c[0] as i32, c[1] as i32)),
        player_info: PlayerInfo::new(
            n.name.clone(),
            PlayerType::Nation,
            None,
            random.next_id(),
            false,
            None,
            vec![],
            None,
            n.flag.clone(),
        ),
    }
}

/// `createNationsForGame`.
pub fn create_nations_for_game(
    config: &GameConfig,
    manifest_nations: &[ManifestNation],
    additional_nations: &[ManifestNation],
    num_humans: usize,
    random: &mut PseudoRandom,
) -> Vec<Nation> {
    let is_compact_map = config.game_map_size == GameMapSize::Compact;
    let is_humans_vs_nations = config.game_mode == GameMode::Team
        && config.player_teams == Some(TeamCountConfig::HumansVsNations);

    match config.nations {
        NationsSetting::Disabled => return vec![],
        NationsSetting::Count(n) => {
            return create_random_nations(n as usize, manifest_nations, additional_nations, random);
        }
        NationsSetting::Default => {}
    }

    if config.game_type == GameType::Public {
        if is_humans_vs_nations {
            return create_random_nations(num_humans, manifest_nations, additional_nations, random);
        }
        if is_compact_map {
            let target = get_compact_map_nation_count(manifest_nations.len(), true);
            let shuffled = random.shuffle_array(manifest_nations);
            return shuffled
                .iter()
                .take(target)
                .map(|n| to_nation(n, random))
                .collect();
        }
    }
    manifest_nations
        .iter()
        .map(|n| to_nation(n, random))
        .collect()
}

fn create_random_nations(
    target_count: usize,
    manifest_nations: &[ManifestNation],
    additional_nations: &[ManifestNation],
    random: &mut PseudoRandom,
) -> Vec<Nation> {
    let shuffled = random.shuffle_array(manifest_nations);
    if target_count <= manifest_nations.len() {
        return shuffled
            .iter()
            .take(target_count)
            .map(|n| to_nation(n, random))
            .collect();
    }
    let mut nations: Vec<Nation> = shuffled.iter().map(|n| to_nation(n, random)).collect();
    let mut used_names: HashSet<String> =
        nations.iter().map(|n| n.player_info.name.clone()).collect();
    let mut remaining = target_count - manifest_nations.len();

    if remaining > 0 && !additional_nations.is_empty() {
        let candidates: Vec<ManifestNation> = additional_nations
            .iter()
            .filter(|n| !used_names.contains(&n.name))
            .cloned()
            .collect();
        let shuffled_extras = random.shuffle_array(&candidates);
        let picked: Vec<&ManifestNation> = shuffled_extras.iter().take(remaining).collect();
        for extra in &picked {
            nations.push(to_nation(extra, random));
            used_names.insert(extra.name.clone());
        }
        remaining -= picked.len();
    }

    for _ in 0..remaining {
        let name = generate_unique_nation_name(random, &used_names);
        used_names.insert(name.clone());
        nations.push(Nation {
            spawn_cell: None,
            player_info: PlayerInfo::new(
                name,
                PlayerType::Nation,
                None,
                random.next_id(),
                false,
                None,
                vec![],
                None,
                None,
            ),
        });
    }
    nations
}

/// `getCompactMapNationCount`.
pub fn get_compact_map_nation_count(manifest_nation_count: usize, is_compact_map: bool) -> usize {
    if manifest_nation_count == 0 {
        return 0;
    }
    if is_compact_map {
        return usize::max(1, (manifest_nation_count as f64 * 0.25).floor() as usize);
    }
    manifest_nation_count
}

fn generate_unique_nation_name(random: &mut PseudoRandom, used: &HashSet<String>) -> String {
    for _ in 0..1000 {
        let name = generate_nation_name(random);
        if !used.contains(&name) {
            return name;
        }
    }
    let mut counter = 1;
    let base = generate_nation_name(random);
    while used.contains(&format!("{base} {counter}")) {
        counter += 1;
    }
    format!("{base} {counter}")
}

fn generate_nation_name(random: &mut PseudoRandom) -> String {
    let t = tables();
    let template = &t.templates[random.next_int(0.0, t.templates.len() as f64) as usize];
    let noun = &t.nouns[random.next_int(0.0, t.nouns.len() as f64) as usize];
    let parts: Vec<String> = template
        .iter()
        .map(|part| match part.as_str() {
            PLURAL_MARK => pluralize(noun),
            NOUN_MARK => noun.clone(),
            other => other.to_string(),
        })
        .collect();
    parts.join(" ")
}

fn pluralize(noun: &str) -> String {
    const O_TO_OES: [&str; 4] = ["Potato", "Tomato", "Volcano", "Torpedo"];
    const SPECIAL: [(&str, &str); 11] = [
        ("Cactus", "Cacti"),
        ("Platypus", "Platypuses"),
        ("Moose", "Moose"),
        ("Octopus", "Octopi"),
        ("Cyclops", "Cyclopes"),
        ("Samurai", "Samurai"),
        ("Fish", "Fish"),
        ("Salmon", "Salmon"),
        ("Cod", "Cod"),
        ("Enderman", "Endermen"),
        ("Mitochondria", "Mitochondria"),
    ];
    if let Some((_, p)) = SPECIAL.iter().find(|(s, _)| *s == noun) {
        return (*p).to_string();
    }
    if noun.ends_with('s')
        || noun.ends_with("ch")
        || noun.ends_with("sh")
        || noun.ends_with('x')
        || noun.ends_with('z')
    {
        return format!("{noun}es");
    }
    if let Some(stem) = noun.strip_suffix('y') {
        let before = stem.chars().last().expect("noun longer than one letter");
        if !"aeiou".contains(before) {
            return format!("{stem}ies");
        }
    }
    if O_TO_OES.contains(&noun) {
        return format!("{noun}es");
    }
    format!("{noun}s")
}
