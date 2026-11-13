// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: targets): ally assistance and target finders.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    pub(super) fn assist_allies(&mut self, game: &mut GameImpl) -> bool {
        let emoji = self.emoji_behavior.clone().expect("not initialized");
        if game.config().nation_alliances_disabled() {
            return false;
        }
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if (game.player(self.player).relation(ally) as u8) < Relation::Friendly as u8 {
                emoji.borrow_mut().send_emoji(
                    game,
                    EmojiRecipient::Player(ally),
                    &emoji_assist_relation_too_low(),
                );
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    emoji.borrow_mut().send_emoji(
                        game,
                        EmojiRecipient::Player(ally),
                        &emoji_assist_target_me(),
                    );
                    continue;
                }
                if self.is_friendly(game, target) {
                    emoji.borrow_mut().send_emoji(
                        game,
                        EmojiRecipient::Player(ally),
                        &emoji_assist_target_ally(),
                    );
                    continue;
                }
                if Self::legacy31(game) {
                    game.player_mut(self.player).update_relation(ally, -20.0);
                    self.send_attack(game, target, false);
                } else {
                    if !self.send_attack(game, target, false) {
                        continue;
                    }
                    game.player_mut(self.player).update_relation(ally, -20.0);
                }
                emoji.borrow_mut().send_emoji(
                    game,
                    EmojiRecipient::Player(ally),
                    &emoji_assist_accept(),
                );
                return true;
            }
        }
        false
    }

    // A traitor who is not significantly stronger than us.
    pub(super) fn find_traitor(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        if game.config().nation_alliances_disabled() {
            return None;
        }
        bordering_enemies.iter().copied().find(|&enemy| {
            game.player(enemy).is_traitor()
                && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 1.2)
        })
    }

    pub(super) fn maybe_betray_and_attack(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        let behavior = self.alliance_behavior.clone().expect("not initialized");
        if game.config().nation_alliances_disabled() {
            return false;
        }
        if !bordering_friends.is_empty() {
            // Computed once, it does not depend on which friend is looked at.
            let juiciest_ally = behavior
                .borrow_mut()
                .find_juiciest_ally(game, bordering_friends);
            for &friend in bordering_friends {
                if behavior.borrow_mut().maybe_betray(
                    game,
                    friend,
                    juiciest_ally,
                    bordering_friends,
                    bordering_enemies,
                ) {
                    let r = self.send_attack(game, friend, true);
                    return Self::sent(game, r);
                }
            }
        }
        false
    }

    pub(super) fn is_bordering_nuked_territory(&self, game: &GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::MissileSilo) {
            return false;
        }
        let mut nbuf = [0; 4];
        for tile in game.player(self.player).border_tiles().iter() {
            let n = game.map.neighbors4_nswe(tile, &mut nbuf);
            for &neighbor in &nbuf[..n] {
                if game.map.is_land(neighbor)
                    && !game.map.has_owner(neighbor)
                    && game.map.has_fallout(neighbor)
                {
                    return true;
                }
            }
        }
        false
    }

    // Someone not much stronger than us who is under big attack (50%+ of their troops incoming).
    pub(super) fn find_victim(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        bordering_enemies.iter().copied().find(|&enemy| {
            if self.is_ffa(game) && game.player(enemy).troops() > self.troops(game) * 1.2 {
                return false;
            }
            let total_incoming_troops = game
                .player(enemy)
                .incoming_attacks()
                .into_iter()
                .fold(0.0, |sum, a| sum + game.attack(a).troops());
            total_incoming_troops > game.player(enemy).troops() * 0.5
        })
    }

    // Very weak (under 15% of their max troops) enemies without many more troops than us.
    pub(super) fn find_very_weak_enemy(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        bordering_enemies.iter().copied().find(|&enemy| {
            let enemy_max_troops = game.player(enemy).max_troops();
            game.player(enemy).troops() < enemy_max_troops * 0.15
                && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 1.2)
        })
    }

    // The juiciest enemy we could plausibly beat (troops at most 75% of ours).
    pub(super) fn find_juicy_target(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        let candidates: Vec<SmallId> = bordering_enemies
            .iter()
            .copied()
            .filter(|&enemy| game.player(enemy).troops() <= self.troops(game) * 0.75)
            .collect();
        find_juiciest_target(game, &candidates)
    }

    /// `findNearestIslandEnemy` before PR 2984: the nearest or second nearest weaker player, reachable or not.
    pub(super) fn find_nearest_island_enemy_v29(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if game.player(self.player).border_tiles().is_empty() {
            return None;
        }
        let filtered_players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                p != self.player
                    && game.player(p).is_alive()
                    && !game.player(p).border_tiles().is_empty()
                    && !self.is_friendly(game, p)
                    && game.player(p).troops() < self.troops(game)
            })
            .collect();
        if filtered_players.is_empty() {
            return None;
        }
        let player_center = self.player_center(game, self.player);
        let mut sorted_players: Vec<(SmallId, u32)> = filtered_players
            .into_iter()
            .map(|p| {
                let center = self.player_center(game, p);
                let a = game.tile_ref(player_center.x, player_center.y);
                let b = game.tile_ref(center.x, center.y);
                (p, game.manhattan_dist(a, b))
            })
            .collect();
        sorted_players.sort_by_key(|&(_, d)| d);
        // The second nearest enemy half of the time, so the boat does not always meet the same warship.
        if sorted_players.len() > 1 && self.chance(2.0) {
            return Some(sorted_players[1].0);
        }
        Some(sorted_players[0].0)
    }

    pub(super) fn find_nearest_island_enemy(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if !game.has(Change::NationAiRework2984) {
            return self.find_nearest_island_enemy_v29(game);
        }
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return None;
        }
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return None;
        }
        let has_shore = game.player(self.player).border_tiles().iter().any(|t| {
            if Self::legacy31(game) {
                game.map.is_ocean_shore(t)
            } else {
                game.map.is_shore(t)
            }
        });
        if !has_shore {
            return None;
        }

        let filtered_players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                if p == self.player {
                    return false;
                }
                if self.is_friendly(game, p) {
                    return false;
                }
                // In FFA, no boats to players with more troops.
                !self.is_ffa(game) || game.player(p).troops() < self.troops(game)
            })
            .collect();
        if filtered_players.is_empty() {
            return None;
        }

        let player_center = self.player_center(game, self.player);
        let mut sorted_players: Vec<(SmallId, u32)> = filtered_players
            .into_iter()
            .map(|p| {
                let center = self.player_center(game, p);
                let a = game.tile_ref(player_center.x, player_center.y);
                let b = game.tile_ref(center.x, center.y);
                (p, game.manhattan_dist(a, b))
            })
            .collect();
        sorted_players.sort_by_key(|&(_, d)| d);

        let v2 = Self::v2(game);
        let my_shore = self.shore_tiles(game, self.player);
        let mut reachable_players: Vec<SmallId> = Vec::new();
        for (p, _) in sorted_players {
            if v2 {
                // Beachhead boats sail any distance, even past a warship: nothing nearer is left to attack.
                let reachable = if self.lands_beachheads(game) {
                    self.borders_by_land(game, p) || self.boat_route(game, p).is_some()
                } else {
                    self.can_reach(game, p)
                };
                if reachable {
                    reachable_players.push(p);
                    if reachable_players.len() >= 2 {
                        break;
                    }
                }
                continue;
            }
            let their_shore = self.shore_tiles(game, p);
            let Some((_, y)) = closest_two_tiles(
                &game.map,
                my_shore.iter().copied(),
                their_shore.iter().copied(),
            ) else {
                continue;
            };
            if can_build_truthy(game, self.player, y) {
                if !game.has(Change::NationBehaviorUpdate3246) {
                    return Some(p);
                }
                reachable_players.push(p);
                // Up to two reachable candidates are enough.
                if reachable_players.len() >= 2 {
                    break;
                }
            }
        }
        if reachable_players.is_empty() {
            return None;
        }
        // 33% chance to pick the second-nearest player.
        if reachable_players.len() >= 2 && self.chance(3.0) {
            return Some(reachable_players[1]);
        }
        Some(reachable_players[0])
    }
}
