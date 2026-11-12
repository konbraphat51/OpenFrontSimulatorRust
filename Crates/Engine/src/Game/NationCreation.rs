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

/// The smaller name tables of builds before v0.31 (`data/NationNamesV30.json`).
fn tables_v30() -> &'static NameTables {
    static TABLES: OnceLock<NameTables> = OnceLock::new();
    TABLES.get_or_init(|| {
        serde_json::from_str(include_str!("../../data/NationNamesV30.json"))
            .expect("NationNamesV30.json is valid")
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
    expanded_names: bool,
    configurable: bool,
    random: &mut PseudoRandom,
) -> Vec<Nation> {
    if !configurable {
        return create_nations_v30_beta1(
            config,
            manifest_nations,
            num_humans,
            expanded_names,
            random,
        );
    }
    let is_compact_map = config.game_map_size == GameMapSize::Compact;
    let is_humans_vs_nations = config.game_mode == GameMode::Team
        && config.player_teams == Some(TeamCountConfig::HumansVsNations);

    match config.nations {
        NationsSetting::Disabled => return vec![],
        NationsSetting::Count(n) => {
            return create_random_nations(
                n as usize,
                manifest_nations,
                additional_nations,
                expanded_names,
                random,
            );
        }
        NationsSetting::Default => {}
    }

    if config.game_type == GameType::Public {
        if is_humans_vs_nations {
            return create_random_nations(
                num_humans,
                manifest_nations,
                additional_nations,
                expanded_names,
                random,
            );
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

/// `createNationsForGame` of v0.30.0-beta1: `disableNations`, a quarter of the nations on every compact map.
fn create_nations_v30_beta1(
    config: &GameConfig,
    manifest_nations: &[ManifestNation],
    num_humans: usize,
    expanded_names: bool,
    random: &mut PseudoRandom,
) -> Vec<Nation> {
    if matches!(config.nations, NationsSetting::Disabled) {
        return vec![];
    }
    let is_compact_map = config.game_map_size == GameMapSize::Compact;
    let is_humans_vs_nations = config.game_mode == GameMode::Team
        && config.player_teams == Some(TeamCountConfig::HumansVsNations);

    let mut effective: Vec<ManifestNation> = manifest_nations.to_vec();
    if is_compact_map && !is_humans_vs_nations {
        let target = get_compact_map_nation_count(manifest_nations.len(), true);
        let shuffled = random.shuffle_array(manifest_nations);
        effective = shuffled.into_iter().take(target).collect();
    }
    if !is_humans_vs_nations {
        return effective.iter().map(|n| to_nation(n, random)).collect();
    }
    let target_count = if config.game_type == GameType::Singleplayer {
        1
    } else {
        num_humans
    };
    if target_count == 0 {
        return vec![];
    }
    if manifest_nations.len() >= target_count {
        let shuffled = random.shuffle_array(manifest_nations);
        return shuffled
            .iter()
            .take(target_count)
            .map(|n| to_nation(n, random))
            .collect();
    }
    let mut nations: Vec<Nation> = manifest_nations
        .iter()
        .map(|n| to_nation(n, random))
        .collect();
    let mut used_names: HashSet<String> =
        nations.iter().map(|n| n.player_info.name.clone()).collect();
    for _ in 0..target_count - manifest_nations.len() {
        let name = generate_unique_nation_name(random, &used_names, expanded_names);
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

fn create_random_nations(
    target_count: usize,
    manifest_nations: &[ManifestNation],
    additional_nations: &[ManifestNation],
    expanded_names: bool,
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
        let name = generate_unique_nation_name(random, &used_names, expanded_names);
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

fn generate_unique_nation_name(
    random: &mut PseudoRandom,
    used: &HashSet<String>,
    expanded: bool,
) -> String {
    for _ in 0..1000 {
        let name = generate_nation_name(random, expanded);
        if !used.contains(&name) {
            return name;
        }
    }
    let mut counter = 1;
    let base = generate_nation_name(random, expanded);
    while used.contains(&format!("{base} {counter}")) {
        counter += 1;
    }
    format!("{base} {counter}")
}

fn generate_nation_name(random: &mut PseudoRandom, expanded: bool) -> String {
    let t = if expanded { tables() } else { tables_v30() };
    let template = &t.templates[random.next_int(0.0, t.templates.len() as f64) as usize];
    let noun = &t.nouns[random.next_int(0.0, t.nouns.len() as f64) as usize];
    let parts: Vec<String> = template
        .iter()
        .map(|part| match part.as_str() {
            PLURAL_MARK => pluralize(noun, expanded),
            NOUN_MARK => noun.clone(),
            other => other.to_string(),
        })
        .collect();
    parts.join(" ")
}

fn pluralize(noun: &str, expanded: bool) -> String {
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
    if expanded && let Some((_, p)) = SPECIAL.iter().find(|(s, _)| *s == noun) {
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
    let o_to_oes: &[&str] = if expanded { &O_TO_OES } else { &O_TO_OES[..2] };
    if o_to_oes.contains(&noun) {
        return format!("{noun}es");
    }
    format!("{noun}s")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plurals_follow_upstream() {
        assert_eq!(pluralize("Cactus", true), "Cacti");
        assert_eq!(pluralize("Potato", true), "Potatoes");
        assert_eq!(pluralize("Puppy", true), "Puppies");
        assert_eq!(pluralize("Monkey", true), "Monkeys");
        assert_eq!(pluralize("Sandwich", true), "Sandwiches");
        assert_eq!(pluralize("Snail", true), "Snails");
    }

    #[test]
    fn compact_maps_use_a_quarter() {
        assert_eq!(get_compact_map_nation_count(0, true), 0);
        assert_eq!(get_compact_map_nation_count(3, true), 1);
        assert_eq!(get_compact_map_nation_count(40, true), 10);
        assert_eq!(get_compact_map_nation_count(40, false), 40);
    }

    #[test]
    fn name_tables_load() {
        assert_eq!(tables().templates.len(), 194);
        assert_eq!(tables().nouns.len(), 236);
    }
}
