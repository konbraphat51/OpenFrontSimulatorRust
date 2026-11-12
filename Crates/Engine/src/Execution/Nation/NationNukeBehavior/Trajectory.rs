// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: trajectory): nuke price memory and trajectories.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// Simulates saving up for a MIRV: the cost the nation perceives for a nuke.
    pub(super) fn get_perceived_nuke_cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        if game.has(Change::NationsLikeHumans5899) {
            return self.get_perceived_nuke_cost_v2(game, t);
        }
        // With only 2 players left, use the actual cost (no point saving for a MIRV).
        if game.has(Change::NationBehaviorUpdate3246) && game.players().len() == 2 {
            return self.cost(game, t);
        }

        if game.config().is_unit_disabled(UnitType::MIRV) {
            return self.cost(game, t);
        }

        // A limited amount in team games, synced with NationStructureBehavior.
        let gold = game.player(self.player).gold();
        if Self::game_mode(game) == GameMode::Team
            && (gold > self.cost(game, UnitType::HydrogenBomb)
                || !game.has(Change::NationNukeRework31))
        {
            return self.cost(game, t);
        }

        // The actual cost if there is already enough gold for both a MIRV and a hydro.
        if gold > self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb) {
            return self.cost(game, t);
        }

        // On Hard & Impossible, ignore the perceived cost when under heavy attack (go all-in on nukes).
        let difficulty = Self::difficulty(game);
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && self.is_under_heavy_attack(game)
        {
            return self.cost(game, t);
        }

        if t == UnitType::AtomBomb {
            self.atom_bomb_perceived_cost
        } else {
            self.hydrogen_bomb_perceived_cost
        }
    }

    pub(super) fn is_under_heavy_attack(&self, game: &GameImpl) -> bool {
        let player = game.player(self.player);
        let mut total_incoming_troops = 0.0;
        for attack in player.incoming_attacks() {
            total_incoming_troops += game.attack(attack).troops();
        }
        total_incoming_troops >= player.troops()
    }

    pub(super) fn remove_old_nuke_events(&mut self, game: &GameImpl) {
        let max_age = 600.0; // 600 ticks = 1 minute
        let tick = game.ticks() as f64;
        while !self.recently_sent_nukes.is_empty()
            && (self.recently_sent_nukes[0].0 as f64) + max_age < tick
        {
            self.recently_sent_nukes.remove(0);
        }
    }

    pub(super) fn is_teammate_already_nuking_this_spot(
        &self,
        game: &GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
    ) -> bool {
        let our_inner_radius = game.config().nuke_magnitudes(nuke_type).inner;

        // Check whether any teammate's nuke blast radius overlaps with ours.
        for nuke in game.units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb]) {
            let unit = game.unit_any(nuke);
            let nuke_owner = unit.owner;

            // Skip our own nukes and non-teammate nukes.
            if nuke_owner == self.player || !game.player(self.player).is_friendly(nuke_owner, false)
            {
                continue;
            }

            let Some(target_tile) = unit.target_tile() else {
                continue;
            };

            let teammate_inner_radius = game.config().nuke_magnitudes(unit.unit_type).inner;

            // The blast zones overlap if the distance between targets < the sum of the two radii.
            let dist_squared = game.map.euclidean_dist_squared(tile, target_tile) as f64;
            let sum_radius = our_inner_radius + teammate_inner_radius;
            if dist_squared <= sum_radius * sum_radius {
                return true;
            }
        }

        false
    }

    /// Flight path of an AI nuke from `from` to `to` (distance-based height, always "up").
    pub(super) fn nuke_trajectory(game: &GameImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let speed = game.config().nuke_speed(UnitType::AtomBomb);
        let path_finder = ParabolaUniversalPathFinder::new(ParabolaOptions {
            increment: Some(speed),
            distance_based_height: Some(true),
            direction_up: Some(true),
            ignore_map_bounds: None,
            legacy_curve: !game.has(Change::MirvNormalize5188),
            legacy_reset: !game.has(Change::SamTargetingFixes4770),
            ..Default::default()
        });
        path_finder.find_path(&game.map, from, to)
    }

    /// Mirrors NukeTrajectoryPreviewLayer.ts: can an enemy SAM intercept the flight from `spawn_tile` to `target_tile`?
    pub(super) fn is_trajectory_interceptable_by_sam(
        &self,
        game: &GameImpl,
        spawn_tile: TileRef,
        target_tile: TileRef,
        excluded_sam_ids: Option<&HashSet<UnitId>>,
    ) -> bool {
        let trajectory = Self::nuke_trajectory(game, spawn_tile, target_tile);
        if trajectory.is_empty() {
            return false;
        }

        let target_range = game.config().default_nuke_targetable_range();
        let target_range_squared = target_range * target_range;
        let dist2 = |a: TileRef, b: TileRef| game.map.euclidean_dist_squared(a, b) as f64;

        let mut untargetable_start: i64 = -1;
        let mut untargetable_end: i64 = -1;
        for (i, &tile) in trajectory.iter().enumerate() {
            if untargetable_start == -1 {
                if dist2(tile, spawn_tile) > target_range_squared {
                    if dist2(tile, target_tile) < target_range_squared {
                        // Overlapping spawn and target range: no untargetable segment.
                        break;
                    } else {
                        untargetable_start = i as i64;
                    }
                }
            } else if dist2(tile, target_tile) < target_range_squared {
                untargetable_end = i as i64;
                break;
            }
        }

        let max_sam_range = game.config().max_sam_range();
        let mut i: i64 = 0;
        while i < trajectory.len() as i64 {
            // Skip the mid-air untargetable portion.
            if untargetable_start != -1 && untargetable_end != -1 && i == untargetable_start {
                i = untargetable_end - 1;
                i += 1;
                continue;
            }

            let tile = trajectory[i as usize];
            let nearby_sams =
                game.nearby_units(tile, max_sam_range, &[UnitType::SAMLauncher], None, false);

            for (sam, sam_dist_squared) in nearby_sams {
                let unit = game.unit_any(sam);
                let owner = unit.owner;
                if owner == self.player || game.player(self.player).is_friendly(owner, false) {
                    continue;
                }
                // Skip SAMs we are intentionally overwhelming.
                if excluded_sam_ids.is_some_and(|ids| ids.contains(&sam)) {
                    continue;
                }
                let range = game.config().sam_range(unit.level as f64);
                if sam_dist_squared <= range * range {
                    return true;
                }
            }
            i += 1;
        }

        false
    }

    pub(super) fn is_valid_nuke_tile(
        &self,
        game: &GameImpl,
        t: TileRef,
        nuke_target: SmallId,
    ) -> bool {
        let difficulty = Self::difficulty(game);
        if game.has(Change::NationsLikeHumans5899) {
            let owner = game.owner_id(t);
            if owner == nuke_target {
                return true;
            }
            // Hard and Impossible: terra nullius (small islands) and other players unless teammates or allies (even AFK ones).
            return (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
                && (owner == 0 || !game.player(self.player).is_friendly(owner, true));
        }

        let owner = game.owner_id(t);
        if owner == nuke_target {
            return true;
        }
        // On Hard & Impossible, allow terra nullius (small islands) and, in team games, other non-friendly players.
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && (owner == 0
                || (Self::game_mode(game) == GameMode::Team
                    && owner != 0
                    && !game.player(self.player).is_friendly(owner, false)))
        {
            return true;
        }
        // On Easy & Medium, only tiles of the target player (nuke away from the border) to reduce nuke usage.
        false
    }
}
