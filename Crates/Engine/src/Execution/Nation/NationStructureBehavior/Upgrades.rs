// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts (part: upgrades): structure upgrades.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationStructureBehavior {
    /// Tries to upgrade an existing structure when the density threshold is exceeded.
    pub(super) fn maybe_upgrade_structure(
        &mut self,
        game: &mut GameImpl,
        structures: &[UnitId],
    ) -> bool {
        if game.has(Change::NationsLikeHumans5899) {
            let best = self.find_best_structure_to_upgrade(game, structures);
            return self.upgrade_structure(game, best);
        }
        if self.get_total_structure_density(game) <= UPGRADE_DENSITY_THRESHOLD {
            return false;
        }
        if structures.is_empty() {
            return false;
        }
        if let Some(structure) = self.find_best_structure_to_upgrade(game, structures) {
            self.log.queue(
                game,
                self.player,
                Decision::Upgrade {
                    unit: structure,
                    amount: 1,
                },
            );
            return true;
        }
        false
    }

    pub(super) fn upgrade_structure(
        &mut self,
        game: &mut GameImpl,
        structure: Option<UnitId>,
    ) -> bool {
        let Some(structure) = structure else {
            return false;
        };
        // canUpgradeUnit was already checked by the caller and is checked again in UpgradeStructureExecution.
        let amount = self.upgrade_amount(game, game.unit_any(structure).unit_type());
        self.log.queue(
            game,
            self.player,
            Decision::Upgrade {
                unit: structure,
                amount,
            },
        );
        true
    }

    /// One level, or as many as the gold beyond the save-up target pays for and the structure ratios ask for.
    pub(super) fn upgrade_amount(&self, game: &GameImpl, t: UnitType) -> u32 {
        let difficulty = Self::difficulty(game);
        // Cities set the ratios, so only gold limits them.
        let due = if t == UnitType::City {
            f64::INFINITY
        } else {
            self.levels_due(
                game,
                t,
                self.city_count(game),
                self.shared_water_components.is_some(),
            )
        };
        let max = f64::min(max_upgrades_at_once(difficulty), due);
        let mut spare = game.player(self.player).gold() - self.get_save_up_target(game);
        let mut amount = 0.0;
        while amount < max {
            let cost = self.cost_extra(game, t, amount as i64);
            if cost > spare {
                break;
            }
            spare -= cost;
            amount += 1.0;
        }
        f64::max(1.0, amount) as u32
    }

    /// Whether to upgrade a SAM instead of building one, because SAMs already cover every structure.
    pub(super) fn should_upgrade_sam_instead(&self, game: &GameImpl) -> bool {
        let chance = sam_upgrade_when_covered_chance(Self::difficulty(game));
        let sams = game.player(self.player).units_of(UnitType::SAMLauncher);
        if chance == 0.0 || sams.is_empty() {
            return false;
        }
        let config = game.config();
        for unit in game
            .player(self.player)
            .units_of_types(&SAM_PROTECTED_TYPES)
        {
            let unit_tile = game.unit_any(unit).tile;
            let covered = sams.iter().any(|&sam| {
                let sam_unit = game.unit_any(sam);
                let range = config.sam_range(sam_unit.level as f64);
                game.map.euclidean_dist_squared(unit_tile, sam_unit.tile) as f64 <= range * range
            });
            if !covered {
                return false;
            }
        }
        chance == 100.0 || self.rnd().next_int(0.0, 100.0) < chance
    }

    /// The upgradable SAM with the most structure levels in range, i.e. the likeliest nuke target.
    pub(super) fn sam_protecting_most(&self, game: &GameImpl) -> Option<UnitId> {
        let config = game.config();
        let protectable = game
            .player(self.player)
            .units_of_types(&SAM_PROTECTED_TYPES);
        let mut best: Option<UnitId> = None;
        let mut best_levels = -1.0;
        for sam in game.player(self.player).units_of(UnitType::SAMLauncher) {
            if !game.player(self.player).can_upgrade_unit(sam) {
                continue;
            }
            let sam_unit = game.unit_any(sam);
            let range = config.sam_range(sam_unit.level as f64);
            let range_squared = range * range;
            let mut levels = 0.0;
            for &unit in &protectable {
                let u = game.unit_any(unit);
                if game.map.euclidean_dist_squared(u.tile, sam_unit.tile) as f64 <= range_squared {
                    levels += u.level as f64;
                }
            }
            if levels > best_levels {
                best = Some(sam);
                best_levels = levels;
            }
        }
        best
    }

    /// `isMirvWorthSavingFor`, computed once per tick.
    pub(super) fn is_mirv_worth_saving_for_memo(&self, game: &GameImpl) -> bool {
        let tick = game.ticks();
        if let Some((t, worth)) = self.mirv_worth_saving_memo.get()
            && t == tick
        {
            return worth;
        }
        let worth = is_mirv_worth_saving_for(game, self.player);
        self.mirv_worth_saving_memo.set(Some((tick, worth)));
        worth
    }

    /// Structure count (levels ignored) per owned tile.
    pub(super) fn get_total_structure_density(&self, game: &GameImpl) -> f64 {
        let player = game.player(self.player);
        let tiles_owned = player.num_tiles_owned();
        if tiles_owned > 0 {
            let types: &[UnitType] = if game.has(Change::NationsLikeHumans5899) {
                &DENSITY_STRUCTURE_TYPES
            } else {
                STRUCTURES
            };
            player.units_of_types(types).len() as f64 / tiles_owned as f64
        } else {
            0.0
        }
    }

    /// The best structure to upgrade: preferably one protected by a SAM; in 50% of cases the 2nd or 3rd best.
    pub(super) fn find_best_structure_to_upgrade(
        &self,
        game: &GameImpl,
        structures: &[UnitId],
    ) -> Option<UnitId> {
        if structures.is_empty() {
            return None;
        }
        let player = game.player(self.player);

        let upgradable: Vec<UnitId> = structures
            .iter()
            .copied()
            .filter(|&s| player.can_upgrade_unit(s))
            .collect();
        if upgradable.is_empty() {
            return None;
        }

        // Based on difficulty, a chance to just pick a random structure.
        let random_chance = match Self::difficulty(game) {
            Difficulty::Easy => 70.0,
            Difficulty::Medium => 40.0,
            Difficulty::Hard => 25.0,
            Difficulty::Impossible => 10.0,
        };

        if self.rnd().next_int(0.0, 100.0) < random_chance {
            return Some(*self.rnd().rand_element(&upgradable));
        }

        let sam_launchers = player.units_of(UnitType::SAMLauncher);

        // Score each structure by SAM protection.
        let mut scored: Vec<(UnitId, f64)> = Vec::new();
        for &structure in &upgradable {
            let mut score = 0.0;
            for &sam in &sam_launchers {
                let sam_unit = game.unit_any(sam);
                let sam_range = game.config().sam_range(sam_unit.level as f64);
                let sam_range_squared = sam_range * sam_range;
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(game.unit_any(structure).tile, sam_unit.tile)
                    as f64;
                if dist_squared <= sam_range_squared {
                    score += 10.0;
                    if sam_unit.level > 1 {
                        score += (sam_unit.level as f64 - 1.0) * 7.5;
                    }
                }
            }
            // A small random factor breaks ties.
            score += self.rnd().next_int(0.0, 5.0);
            scored.push((structure, score));
        }

        // Descending by score (stable).
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));

        // 50% of the time, pick the second or third best for variety.
        if scored.len() >= 2 && self.rnd().chance(2.0) {
            let pick_index = if scored.len() >= 3 {
                self.rnd().next_int(1.0, 3.0) as usize
            } else {
                1
            };
            return Some(scored[pick_index].0);
        }

        Some(scored[0].0)
    }
}
