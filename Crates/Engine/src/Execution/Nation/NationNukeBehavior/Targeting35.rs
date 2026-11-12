// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: targeting35): target choice of v0.35.0 on.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// `maybeSendNuke()` of v0.35.0 on.
    pub(super) fn maybe_send_nuke_v2(&mut self, game: &mut GameImpl, silos: Vec<UnitId>) {
        let Some((nuke_target, strike)) = self.choose_nuke_target(game) else {
            return;
        };
        if game.player(nuke_target).player_type() == PlayerType::Bot
            || game.player(self.player).is_on_same_team(nuke_target)
            || !self
                .attack_behavior
                .borrow_mut()
                .should_attack(game, nuke_target)
        {
            return;
        }

        // While saving up for a MIRV, Hard and Impossible only nuke back at players hitting them, with atom bombs.
        let saving = self.is_saving_for_mirv(game);
        if saving && !self.is_hitting_us(game, nuke_target) {
            return;
        }

        let nuke_types = self.nuke_types_to_try(game, saving);
        if nuke_types.is_empty() {
            return;
        }

        let structures = game.player(nuke_target).units_of_types(STRUCTURES);
        let difficulty = Self::difficulty(game);
        let num_random_tiles = if difficulty == Difficulty::Impossible {
            30
        } else {
            10
        };
        let random_tiles =
            rand_territory_tile_array(&mut self.rnd(), game, nuke_target, num_random_tiles);
        let mut candidates: Vec<TileRef> = Vec::new();
        let mut seen: HashSet<TileRef> = HashSet::new();
        for t in random_tiles
            .into_iter()
            .chain(structures.iter().map(|&u| game.unit_any(u).tile))
        {
            if seen.insert(t) {
                candidates.push(t);
            }
        }
        self.remove_old_nuke_events(game);

        for nuke_type in nuke_types {
            let tile = self.find_best_nuke_tile(
                game,
                nuke_type,
                nuke_target,
                &candidates,
                &silos,
                &structures,
                strike,
            );
            if let Some(tile) = tile {
                self.send_nuke(game, tile, nuke_type, nuke_target, 0.0);
                if strike && Self::follows_up_strikes(game) {
                    self.strikes.push(Strike {
                        target: nuke_target,
                        tile,
                        tick: game.ticks(),
                        nuke: None,
                    });
                }
                return;
            }
        }
        if difficulty == Difficulty::Impossible && !saving {
            self.maybe_destroy_enemy_sam(game, nuke_target);
        }
    }

    /// Hydrogen bomb first; on Hard and Impossible, an atom bomb if no spot is worth a hydrogen bomb.
    pub(super) fn nuke_types_to_try(&self, game: &GameImpl, atom_only: bool) -> Vec<UnitType> {
        let config = game.config();
        let difficulty = config.game_config().difficulty;
        let gold = game.player(self.player).gold();
        let mut types: Vec<UnitType> = Vec::new();
        if !atom_only
            && !config.is_unit_disabled(UnitType::HydrogenBomb)
            && gold >= self.get_perceived_nuke_cost(game, UnitType::HydrogenBomb)
        {
            types.push(UnitType::HydrogenBomb);
        }
        if (types.is_empty()
            || difficulty == Difficulty::Hard
            || difficulty == Difficulty::Impossible)
            && !config.is_unit_disabled(UnitType::AtomBomb)
            && gold >= self.get_perceived_nuke_cost(game, UnitType::AtomBomb)
        {
            types.push(UnitType::AtomBomb);
        }
        types
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn find_best_nuke_tile(
        &self,
        game: &GameImpl,
        nuke_type: UnitType,
        nuke_target: SmallId,
        candidates: &[TileRef],
        silos: &[UnitId],
        structures: &[UnitId],
        strike: bool,
    ) -> Option<TileRef> {
        let difficulty = Self::difficulty(game);
        let range = game.config().nuke_magnitudes(nuke_type).outer;
        let mut scored: Vec<(TileRef, f64)> = Vec::new();

        'outer: for &tile in candidates {
            // Cheap pre-filter, blast_hits_friendly_land() checks the whole blast of the winner.
            let mut bounding_box = bounding_box_tiles(&game.map, tile, range as i32);
            bounding_box.extend(bounding_box_tiles(
                &game.map,
                tile,
                (range / 2.0).floor() as i32,
            ));
            for t in bounding_box {
                if !self.is_valid_nuke_tile(game, t, nuke_target) {
                    continue 'outer;
                }
            }
            let Some(spawn_tile) = game.player(self.player).can_build(nuke_type, tile, None) else {
                continue;
            };

            // Above Easy, skip spots that are already being nuked or where the blast would be mostly wasted.
            let mut coverage = 0.0;
            if difficulty != Difficulty::Easy {
                if self.is_nuke_already_landing_near(game, tile, nuke_type) {
                    continue;
                }
                coverage = self.enemy_land_blast_coverage(game, tile, nuke_type);
                if coverage < min_blast_coverage(nuke_type) {
                    continue;
                }
            }

            // On Hard and Impossible, avoid trajectories that enemy SAMs can intercept.
            if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
                && self.is_trajectory_interceptable_by_sam(game, spawn_tile, tile, None)
            {
                continue;
            }

            let value = self.nuke_tile_score(game, tile, silos, structures, nuke_type);
            // Impossible only nukes spots with structure value, unless it is after the troops.
            let skip = if difficulty == Difficulty::Impossible && !strike {
                value <= 0.0
            } else {
                value < 0.0
            };
            if skip {
                continue;
            }
            scored.push((tile, value + coverage * FULL_BLAST_COVERAGE_VALUE));
        }
        // Stable sort: on ties the earlier candidate wins.
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let margin = if strike { STRIKE_BORDER_MARGIN } else { 0.0 };
        scored
            .into_iter()
            .find(|&(tile, _)| !self.blast_hits_friendly_land(game, tile, nuke_type, margin))
            .map(|(tile, _)| tile)
    }

    /// Who to nuke, and whether it is a strike: a nuke that softens a player up for our attack.
    pub(super) fn choose_nuke_target(&mut self, game: &mut GameImpl) -> Option<(SmallId, bool)> {
        let target = self.find_best_nuke_target(game)?;
        let strike = match Self::difficulty(game) {
            Difficulty::Easy => false,
            Difficulty::Medium => self.troops_attacking(game, target) > 0.0,
            Difficulty::Hard | Difficulty::Impossible => self.can_strike(game, target),
        };
        Some((target, strike))
    }

    pub(super) fn find_best_nuke_target_v2(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        // On Hard and Impossible with only 2 players left, target the only other one.
        let diff = Self::difficulty(game);
        if (diff == Difficulty::Hard || diff == Difficulty::Impossible)
            && game.players().len() == 2
            && let Some(other) = game.players().into_iter().find(|&p| p != self.player)
        {
            return Some(other);
        }

        // Retaliate against incoming attacks (most important).
        let incoming_attack_player = self
            .attack_behavior
            .borrow_mut()
            .find_incoming_attack_player(game);
        if incoming_attack_player.is_some() {
            return incoming_attack_player;
        }

        if let Some(p) = self.find_incoming_nuke_player(game) {
            return Some(p);
        }

        // In Teams, back up teammates in their fights before starting our own.
        if let Some(p) = self.find_teammate_fight_target(game) {
            return Some(p);
        }

        // On Impossible, prioritize nuking the crown if they have more than 50% of the map.
        let game_mode = Self::game_mode(game);
        if diff == Difficulty::Impossible && game_mode == GameMode::FFA {
            let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
            if num_tiles_without_fallout > 0.0 {
                let sorted_by_tiles = Self::sorted_by_tiles(game);
                let crown = sorted_by_tiles[0];
                if crown != self.player && !game.player(self.player).is_friendly(crown, false) {
                    let crown_share =
                        game.player(crown).num_tiles_owned() as f64 / num_tiles_without_fallout;
                    if crown_share > CROWN_NEAR_WIN_SHARE {
                        return Some(crown);
                    }
                }
            }
        }

        // Soften up a neighbour about as strong as us before attacking it.
        if let Some(p) = self.find_strike_target(game) {
            return Some(p);
        }

        // On Impossible, the richest nation hunts very high structure density targets.
        if diff == Difficulty::Impossible
            && self.is_richest_nation(game)
            && self.rnd().chance(2.0)
            && let Some(dense_target) = self.find_high_density_target(game)
        {
            return Some(dense_target);
        }

        // Assist allies: check their targets (as in assistAllies, without emojis).
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if game.player(self.player).relation(ally) < Relation::Friendly {
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    continue;
                }
                if game.player(self.player).is_friendly(target, false) {
                    continue;
                }
                return Some(target);
            }
        }

        // The most hated player; much weaker players are ignored (no nukes needed).
        let my_max_troops = game.player(self.player).max_troops();
        for (other, relation) in game.player(self.player).all_relations_sorted() {
            if relation != Relation::Hostile {
                continue;
            }
            if game.player(self.player).is_friendly(other, false) {
                continue;
            }
            let other_max_troops = game.player(other).max_troops();
            if my_max_troops >= other_max_troops * 2.0 {
                continue;
            }
            return Some(other);
        }

        // In FFAs, nuke the crown if they are far enough ahead.
        if let Some(crown_target) = self.find_ffa_crown_target(game) {
            return Some(crown_target);
        }

        // In Teams, nuke the strongest team.
        self.find_strongest_team_target(game)
    }

    pub(super) fn find_ffa_crown_target_v2(&self, game: &GameImpl) -> Option<SmallId> {
        let difficulty = Self::difficulty(game);
        if Self::game_mode(game) != GameMode::FFA {
            return None;
        }
        if game.players().len() <= 1 {
            return None;
        }
        let sorted_by_tiles = Self::sorted_by_tiles(game);
        let first_place = sorted_by_tiles[0];

        // Don't target ourselves or allies.
        if first_place == self.player || game.player(self.player).is_friendly(first_place, false) {
            return None;
        }
        let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
        if num_tiles_without_fallout <= 0.0 {
            return None;
        }
        let first_place_share =
            game.player(first_place).num_tiles_owned() as f64 / num_tiles_without_fallout;
        if first_place_share > CROWN_NEAR_WIN_SHARE {
            return Some(first_place);
        }

        // Below that, only a runaway leader that is far enough ahead of us.
        if find_runaway_leader(game) != Some(first_place) {
            return None;
        }
        let mut threshold = match difficulty {
            Difficulty::Easy => return None, // Never notices a runaway leader
            Difficulty::Medium => 0.3,
            Difficulty::Hard => 0.2,
            Difficulty::Impossible => 0.1,
        };
        if !game.player(self.player).shares_border_with(first_place) {
            threshold *= 2.0;
        }
        let my_share =
            game.player(self.player).num_tiles_owned() as f64 / num_tiles_without_fallout;
        if first_place_share - my_share > threshold {
            return Some(first_place);
        }
        None
    }
}
