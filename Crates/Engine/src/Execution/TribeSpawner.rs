// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TribeSpawner.ts: creates the spawn executions of the tribes (bots).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use super::Utils::TribeNames::{CustomTribe, TribeNameData, resolve_tribe_name_data};
use crate::Builds::Rules::Change;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Game::Game::{Cell, GameMapSize, PlayerInfo, PlayerType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::collections::HashSet;

/// Builds the tribe spawns of a game.
pub struct TribeSpawner<'a> {
    game: &'a GameImpl,
    game_id: String,
    random: PseudoRandom,
    tribe_name_data: TribeNameData,
    used_custom_tribes: HashSet<String>,
    nation_tiles: HashSet<TileRef>,
}

impl<'a> TribeSpawner<'a> {
    /// `new TribeSpawner(gs, gameID, nationCells)`.
    pub fn new(game: &'a GameImpl, game_id: &str, nation_cells: &[Cell]) -> Self {
        let nation_tiles = nation_cells
            .iter()
            .filter(|c| game.is_valid_coord(c.x, c.y))
            .map(|c| game.tile_ref(c.x, c.y))
            .collect();
        Self {
            game,
            game_id: game_id.to_string(),
            // A different seed than createGameRunner, to avoid id collisions.
            random: PseudoRandom::for_rules(
                game.config().rules(),
                simple_hash(game_id) as f64 + 2.0,
            ),
            tribe_name_data: resolve_tribe_name_data(
                game.config().game_config().game_map,
                game.config().rules(),
            ),
            used_custom_tribes: HashSet::new(),
            nation_tiles,
        }
    }

    /// `spawnTribes(numTribes, purchasedNames)`.
    pub fn spawn_tribes(
        &mut self,
        num_tribes: usize,
        purchased_names: &[String],
    ) -> Vec<SpawnExecution> {
        let mut tribes: Vec<SpawnExecution> = Vec::new();

        if let Some(custom) = self.tribe_name_data.custom_tribes.clone() {
            for ct in custom.iter().filter(|c| c.coordinates.is_some()) {
                if tribes.len() >= num_tribes {
                    break;
                }
                if let Some(exec) = self.spawn_positioned_tribe(ct) {
                    tribes.push(exec);
                    self.used_custom_tribes.insert(ct.name.clone());
                }
            }
        }

        let remaining = num_tribes - tribes.len();
        let purchased_names: &[String] =
            if self.game.config().rules().has(Change::TribeNameThemes4647) {
                purchased_names
            } else {
                &[]
            };
        let mut purchased_by_slot: OrderedMap<usize, String> = OrderedMap::new();
        if !purchased_names.is_empty() && remaining > 0 {
            let used = &purchased_names[..purchased_names.len().min(remaining)];
            let all_slots: Vec<usize> = (0..remaining).collect();
            let slots = self.random.shuffle_array(&all_slots);
            for (i, slot) in slots.iter().take(used.len()).enumerate() {
                purchased_by_slot.insert(*slot, used[i].clone());
            }
        }

        let mut slot = 0usize;
        while tribes.len() < num_tribes {
            let name = match purchased_by_slot.get(&slot) {
                Some(n) => n.clone(),
                None => self.random_tribe_name(),
            };
            tribes.push(self.spawn_tribe(name));
            slot += 1;
        }
        tribes
    }

    fn spawn_tribe(&mut self, tribe_name: String) -> SpawnExecution {
        let id = self.random.next_id();
        SpawnExecution::new(
            &self.game_id,
            PlayerInfo::new(
                tribe_name,
                PlayerType::Bot,
                None,
                id,
                false,
                None,
                vec![],
                None,
                None,
            ),
            None,
            false,
            self.game.config().rules(),
        )
    }

    fn spawn_positioned_tribe(&mut self, ct: &CustomTribe) -> Option<SpawnExecution> {
        let coords = ct.coordinates?;
        let is_compact = self.game.config().game_config().game_map_size == GameMapSize::Compact;
        let (x, y) = if is_compact {
            (coords[0].div_euclid(2), coords[1].div_euclid(2))
        } else {
            (coords[0], coords[1])
        };
        if !self.game.is_valid_coord(x as i32, y as i32) {
            return None;
        }
        let tile = self.game.tile_ref(x as i32, y as i32);
        if !self.game.is_land(tile)
            || self.game.has_owner(tile)
            || self.game.map.is_impassable(tile)
            || self.nation_tiles.contains(&tile)
        {
            return None;
        }
        let id = self.random.next_id();
        Some(SpawnExecution::new(
            &self.game_id,
            PlayerInfo::new(
                ct.name.clone(),
                PlayerType::Bot,
                None,
                id,
                false,
                None,
                vec![],
                None,
                None,
            ),
            Some(tile),
            false,
            self.game.config().rules(),
        ))
    }

    fn random_tribe_name(&mut self) -> String {
        if let Some(custom) = &self.tribe_name_data.custom_tribes {
            let available: Vec<&CustomTribe> = custom
                .iter()
                .filter(|ct| {
                    ct.coordinates.is_none() && !self.used_custom_tribes.contains(&ct.name)
                })
                .collect();
            if !available.is_empty() {
                let index = self.random.next_int(0.0, available.len() as f64) as usize;
                let chosen = available[index].name.clone();
                self.used_custom_tribes.insert(chosen.clone());
                return chosen;
            }
        }
        let prefix_index =
            self.random
                .next_int(0.0, self.tribe_name_data.prefixes.len() as f64) as usize;
        let suffix_index =
            self.random
                .next_int(0.0, self.tribe_name_data.suffixes.len() as f64) as usize;
        format!(
            "{} {}",
            self.tribe_name_data.prefixes[prefix_index],
            self.tribe_name_data.suffixes[suffix_index]
        )
    }
}
