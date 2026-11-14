// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: targets): nuke target finders.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// `findBestNukeTarget()`.
    pub fn find_best_nuke_target(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if game.has(Change::NationsLikeHumans5899) {
            return self.find_best_nuke_target_v2(game);
        }
        // On Hard & Impossible with only 2 players left, target the only other one.
        let diff = Self::difficulty(game);
        if game.has(Change::NationBehaviorUpdate3246)
            && (diff == Difficulty::Hard || diff == Difficulty::Impossible)
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

        // On Impossible, the richest nation hunts very high structure density targets.
        if diff == Difficulty::Impossible
            && game.has(Change::NationNukeRework31)
            && self.is_richest_nation(game)
            && self.rnd().chance(2.0)
            && let Some(dense_target) = self.find_high_density_target(game)
        {
            return Some(dense_target);
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
                    if crown_share > 0.5 {
                        return Some(crown);
                    }
                }
            }
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
        if let Some(team_target) = self.find_strongest_team_target(game) {
            return Some(team_target);
        }

        None
    }

    /// Alive players by descending tile count (stable).
    pub(super) fn sorted_by_tiles(game: &GameImpl) -> Vec<SmallId> {
        let mut sorted = game.players();
        sorted.sort_by(|&a, &b| {
            game.player(b)
                .num_tiles_owned()
                .cmp(&game.player(a).num_tiles_owned())
        });
        sorted
    }

    pub(super) fn is_richest_nation(&self, game: &GameImpl) -> bool {
        let my_gold = game.player(self.player).gold();
        for other in game.players() {
            if other == self.player {
                continue;
            }
            if game.player(other).player_type() != PlayerType::Nation {
                continue;
            }
            if game.player(other).gold() > my_gold {
                return false;
            }
        }
        true
    }

    pub(super) fn find_high_density_target(&self, game: &GameImpl) -> Option<SmallId> {
        let mut best_target = None;
        let mut best_density = HIGH_DENSITY_NUKE_THRESHOLD;
        for other in game.players() {
            if other == self.player {
                continue;
            }
            if game.player(other).player_type() == PlayerType::Bot {
                continue;
            }
            if game.player(self.player).is_friendly(other, false) {
                continue;
            }
            let tiles_owned = game.player(other).num_tiles_owned() as f64;
            if tiles_owned == 0.0 {
                continue;
            }
            let mut level_sum = 0.0;
            for s in game.player(other).units_of_types(STRUCTURES) {
                level_sum += game.unit_any(s).level as f64;
            }
            // Skip players with too few structures regardless of density.
            if level_sum < MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE {
                continue;
            }
            let density = level_sum / tiles_owned;
            if density > best_density {
                best_density = density;
                best_target = Some(other);
            }
        }
        best_target
    }

    pub(super) fn find_ffa_crown_target(&self, game: &GameImpl) -> Option<SmallId> {
        let difficulty = Self::difficulty(game);
        if game.has(Change::NationsLikeHumans5899) {
            return self.find_ffa_crown_target_v2(game);
        }
        if Self::game_mode(game) != GameMode::FFA {
            return None;
        }

        if game.players().len() <= 1 {
            return None;
        }

        let sorted_by_tiles = Self::sorted_by_tiles(game);
        let first_place = sorted_by_tiles[0];

        // If we are the crown on Impossible, target 2nd place.
        if difficulty == Difficulty::Impossible
            && first_place == self.player
            && sorted_by_tiles.len() >= 2
        {
            let second_place = sorted_by_tiles[1];
            if !game.player(self.player).is_friendly(second_place, false) {
                return Some(second_place);
            }
        }

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
        let my_share =
            game.player(self.player).num_tiles_owned() as f64 / num_tiles_without_fallout;

        let threshold = match difficulty {
            Difficulty::Easy => 0.4,
            Difficulty::Medium => 0.3,
            Difficulty::Hard => 0.2,
            Difficulty::Impossible => 0.1,
        };

        // Does first place have `threshold` more tile percentage of the map than us?
        if first_place_share - my_share > threshold {
            return Some(first_place);
        }

        None
    }

    pub(super) fn find_strongest_team_target(&self, game: &GameImpl) -> Option<SmallId> {
        if Self::game_mode(game) != GameMode::Team {
            return None;
        }

        if game.players().len() <= 1 {
            return None;
        }

        let mut team_tiles: OrderedMap<String, f64> = OrderedMap::new();
        let mut team_players: OrderedMap<String, Vec<SmallId>> = OrderedMap::new();

        for p in game.players() {
            let Some(team) = game.player(p).team().cloned() else {
                continue;
            };
            let tiles = team_tiles.get(&team).copied().unwrap_or(0.0);
            team_tiles.insert(
                team.clone(),
                tiles + game.player(p).num_tiles_owned() as f64,
            );
            if let Some(players) = team_players.get_mut(&team) {
                players.push(p);
            } else {
                team_players.insert(team, vec![p]);
            }
        }

        let mut sorted_teams: Vec<(String, f64)> =
            team_tiles.iter().map(|(k, v)| (k.clone(), *v)).collect();
        sorted_teams.sort_by(|a, b| b.1.total_cmp(&a.1));

        if sorted_teams.is_empty() {
            return None;
        }

        let mut strongest_team = sorted_teams[0].0.clone();
        if game.player(self.player).team() == Some(&strongest_team) {
            if sorted_teams.len() > 1 {
                strongest_team = sorted_teams[1].0.clone();
            } else {
                return None;
            }
        }

        let target_team_players = team_players.get(&strongest_team).unwrap();
        let v2 = game.has(Change::NationsLikeHumans5899);

        // Filter out friendly players.
        let valid_targets: Vec<SmallId> = target_team_players
            .iter()
            .copied()
            .filter(|&p| !game.player(self.player).is_friendly(p, false))
            .collect();

        if valid_targets.is_empty() {
            return None;
        }

        // Hard and Impossible: one of the three players with the most structure levels.
        let difficulty = Self::difficulty(game);
        if v2 && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible) {
            let mut most_structures: Vec<(SmallId, f64)> = valid_targets
                .iter()
                .map(|&p| (p, Self::structure_levels(game, p)))
                .filter(|&(_, levels)| levels > 0.0)
                .collect();
            most_structures.sort_by(|a, b| b.1.total_cmp(&a.1));
            most_structures.truncate(3);
            if !most_structures.is_empty() {
                return Some(self.rnd().rand_element(&most_structures).0);
            }
        }

        if self.rnd().chance(2.0) {
            // Strongest player (the later one wins ties, as upstream's reduce does).
            let mut best = valid_targets[0];
            for &current in &valid_targets[1..] {
                if game.player(best).max_troops() <= game.player(current).max_troops() {
                    best = current;
                }
            }
            Some(best)
        } else {
            // Random player.
            Some(*self.rnd().rand_element(&valid_targets))
        }
    }
}
