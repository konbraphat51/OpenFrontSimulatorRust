// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/BotBehavior.ts of v0.28.4: attack decisions shared by tribes and fake humans.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Players are small ids with `0` standing for terra nullius. The `PseudoRandom` is shared with the
//! owning execution (upstream passes one instance), hence the `Rc`.

use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::Util::{bounding_box_center, calculate_bounding_box_center};
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::EmojiIds::emoji_ids;
use crate::Game::Game::{Cell, Difficulty, PlayerType, Relation, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

fn emoji_assist_accept() -> Vec<f64> {
    emoji_ids(&["\u{1f44d}", "\u{26f5}", "\u{1f91d}", "\u{1f3af}"])
}

fn emoji_relation_too_low() -> Vec<f64> {
    emoji_ids(&["\u{1f971}", "\u{1f926}\u{200d}\u{2642}\u{fe0f}"])
}

fn emoji_target_me() -> Vec<f64> {
    emoji_ids(&["\u{1f97a}", "\u{1f480}"])
}

fn emoji_target_ally() -> Vec<f64> {
    emoji_ids(&["\u{1f54a}\u{fe0f}", "\u{1f44e}"])
}

fn emoji_heckle() -> Vec<f64> {
    emoji_ids(&["\u{1f921}", "\u{1f621}"])
}

/// Attack decisions of one tribe or fake human.
pub struct BotBehaviorV28 {
    random: SharedRandom,
    player: SmallId,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    bot_attack_troops_sent: f64,
    last_emoji_sent: OrderedMap<SmallId, Tick>,
}

impl BotBehaviorV28 {
    /// `new BotBehavior(random, game, player, triggerRatio, reserveRatio, expandRatio)`.
    pub fn new(
        random: SharedRandom,
        player: SmallId,
        trigger_ratio: f64,
        reserve_ratio: f64,
        expand_ratio: f64,
    ) -> Self {
        Self {
            random,
            player,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            bot_attack_troops_sent: 0.0,
            last_emoji_sent: OrderedMap::new(),
        }
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn rand_element(&self, list: &[f64]) -> f64 {
        *self.random.borrow_mut().rand_element(list)
    }

    fn is_friendly(&self, game: &GameImpl, other: SmallId) -> bool {
        game.player(self.player).is_friendly(other, false)
    }

    fn emoji(&self, game: &mut GameImpl, player: SmallId, emoji: f64) {
        if game.player(player).player_type() != PlayerType::Human {
            return;
        }
        let id = game.player(player).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.player, id, emoji)));
    }

    /// Prevents attacking humans on lower difficulties (or when we are a bot).
    fn should_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        // Always attack terra nullius, non-humans and traitors
        if other == 0
            || game.player(other).player_type() != PlayerType::Human
            || game.player(other).is_traitor()
            || game.player(self.player).player_type() == PlayerType::Bot
        {
            return true;
        }
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Easy && self.chance(4.0) {
            return false;
        }
        if difficulty == Difficulty::Medium && self.chance(2.0) {
            return false;
        }
        true
    }

    fn betray(&self, game: &mut GameImpl, target: SmallId) {
        let Some(alliance) = game.player(self.player).alliance_with(target) else {
            return;
        };
        game.player_mut(self.player).break_alliance(alliance);
    }

    fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.reserve_ratio
    }

    fn has_trigger_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.trigger_ratio
    }

    fn find_incoming_attack_player(&self, game: &GameImpl) -> Option<SmallId> {
        // Ignore bot attacks if we are not a bot.
        let ignore_bots = game.player(self.player).player_type() != PlayerType::Bot;
        let mut largest_attack = 0.0;
        let mut largest_attacker: Option<SmallId> = None;
        for attack in game.player(self.player).incoming_attacks() {
            let attacker = game.attack(attack).attacker();
            if ignore_bots && game.player(attacker).player_type() == PlayerType::Bot {
                continue;
            }
            let troops = game.attack(attack).troops();
            if troops <= largest_attack {
                continue;
            }
            largest_attack = troops;
            largest_attacker = Some(attacker);
        }
        largest_attacker
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(&self, game: &GameImpl) -> Option<SmallId> {
        let traitors: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| n != 0 && !self.is_friendly(game, n) && game.player(n).is_traitor())
            .collect();
        if traitors.is_empty() {
            None
        } else {
            Some(*self.random.borrow_mut().rand_element(&traitors))
        }
    }

    /// `assistAllies()`.
    pub fn assist_allies(&mut self, game: &mut GameImpl) {
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if game.player(self.player).relation(ally) < Relation::Friendly {
                let e = self.rand_element(&emoji_relation_too_low());
                self.emoji(game, ally, e);
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    let e = self.rand_element(&emoji_target_me());
                    self.emoji(game, ally, e);
                    continue;
                }
                if self.is_friendly(game, target) {
                    let e = self.rand_element(&emoji_target_ally());
                    self.emoji(game, ally, e);
                    continue;
                }
                // All checks passed, assist them
                game.player_mut(self.player).update_relation(ally, -20.0);
                self.send_attack(game, target, false);
                let e = self.rand_element(&emoji_assist_accept());
                self.emoji(game, ally, e);
                return;
            }
        }
    }

    fn most_hated_hostile(&self, game: &GameImpl) -> Option<SmallId> {
        let sorted = game.player(self.player).all_relations_sorted();
        let &(player, relation) = sorted.first()?;
        if relation == Relation::Hostile && !self.is_friendly(game, player) {
            Some(player)
        } else {
            None
        }
    }

    /// `attackBestTarget(borderingFriends, borderingEnemies)`.
    pub fn attack_best_target(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        // Save up troops until we reach the reserve ratio
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Maybe save up troops until we reach the trigger ratio
        if !self.has_trigger_ratio_troops(game) && !self.chance(10.0) {
            return;
        }
        // Retaliate against incoming attacks (most important)
        if let Some(incoming) = self.find_incoming_attack_player(game) {
            self.send_attack(game, incoming, true);
            return;
        }
        // Attack bots
        if self.attack_bots(game) {
            return;
        }
        // Maybe betray and attack
        if self.maybe_betray_and_attack(game, bordering_friends) {
            return;
        }
        // Attack nuked territory
        if self.is_bordering_nuked_territory(game) {
            self.send_attack(game, 0, false);
            return;
        }
        // Attack the most hated player with hostile relation
        if let Some(most_hated) = self.most_hated_hostile(game) {
            self.send_attack(game, most_hated, false);
            return;
        }
        // Attack the weakest player
        if let Some(&weakest) = bordering_enemies.first() {
            self.send_attack(game, weakest, false);
            return;
        }
        // Without bordering enemies, attack someone on an island next to us
        if let Some(island) = self.find_nearest_island_enemy(game) {
            self.send_attack(game, island, false);
        }
    }

    /// `attackBots()`: parallel attacks on neighbouring bots, sparsest first.
    pub fn attack_bots(&mut self, game: &mut GameImpl) -> bool {
        let bots: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| {
                n != 0
                    && !self.is_friendly(game, n)
                    && game.player(n).player_type() == PlayerType::Bot
            })
            .collect();
        if bots.is_empty() {
            return false;
        }
        self.bot_attack_troops_sent = 0.0;
        let density = |game: &GameImpl, p: SmallId| {
            game.player(p).troops() / game.player(p).num_tiles_owned() as f64
        };
        let mut sorted = bots;
        sorted.sort_by(|&a, &b| {
            (density(game, a) - density(game, b))
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let max = self.get_bot_attack_max_parallelism(game);
        for bot in sorted.into_iter().take(max) {
            self.send_attack(game, bot, false);
        }
        // Only short-circuit the pipeline when troops were allocated to bot attacks
        self.bot_attack_troops_sent > 0.0
    }

    fn get_bot_attack_max_parallelism(&self, game: &GameImpl) -> usize {
        match game.config().game_config().difficulty {
            Difficulty::Easy => 1,
            Difficulty::Medium => 2,
            Difficulty::Hard => 4,
            // On impossible, attack as many bots in parallel as possible
            Difficulty::Impossible => 100,
        }
    }

    /// Betray friends when we have ten times their troops.
    pub fn maybe_betray_and_attack(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
    ) -> bool {
        for &friend in bordering_friends {
            if game.player(self.player).is_allied_with(friend)
                && game.player(self.player).troops() >= game.player(friend).troops() * 10.0
            {
                self.betray(game, friend);
                self.send_attack(game, friend, true);
                return true;
            }
        }
        false
    }

    /// `findBestNukeTarget(borderingEnemies)`.
    pub fn find_best_nuke_target(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        // Retaliate against incoming attacks (most important)
        if let Some(incoming) = self.find_incoming_attack_player(game) {
            return Some(incoming);
        }
        // The most hated player with hostile relation
        if let Some(most_hated) = self.most_hated_hostile(game) {
            return Some(most_hated);
        }
        // The weakest player
        if let Some(&weakest) = bordering_enemies.first() {
            return Some(weakest);
        }
        // Without bordering enemies, someone on an island next to us
        self.find_nearest_island_enemy(game)
    }

    fn get_player_center(&self, game: &GameImpl, player: SmallId) -> Cell {
        let p = game.player(player);
        if let Some((min, max)) = p.largest_cluster_bounding_box {
            return bounding_box_center(min, max);
        }
        calculate_bounding_box_center(&game.map, p.border_tiles().iter())
            .expect("a player with a border has a center")
    }

    /// `findNearestIslandEnemy()`: the nearest (or second nearest) player we do not border.
    pub fn find_nearest_island_enemy(&self, game: &GameImpl) -> Option<SmallId> {
        if game.player(self.player).border_tiles().is_empty() {
            return None;
        }
        let our_troops = game.player(self.player).troops();
        let filtered: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                if p == self.player {
                    return false;
                }
                if !game.player(p).is_alive() {
                    return false;
                }
                if game.player(p).border_tiles().is_empty() {
                    return false;
                }
                if self.is_friendly(game, p) {
                    return false;
                }
                // Don't spam boats into players more than 2x our troops
                game.player(p).troops() <= our_troops * 2.0
            })
            .collect();
        if filtered.is_empty() {
            return None;
        }
        let player_center = self.get_player_center(game, self.player);
        let center_tile = game.tile_ref(player_center.x, player_center.y);
        let mut sorted: Vec<(SmallId, u32)> = filtered
            .into_iter()
            .map(|p| {
                let c = self.get_player_center(game, p);
                let tile = game.tile_ref(c.x, c.y);
                (p, game.manhattan_dist(center_tile, tile))
            })
            .collect();
        sorted.sort_by_key(|&(_, d)| d);
        // The nearest or second-nearest enemy, so a boat does not always meet the same warship
        if sorted.len() > 1 && self.chance(2.0) {
            Some(sorted[1].0)
        } else {
            Some(sorted[0].0)
        }
    }

    /// `attackRandomTarget()`.
    pub fn attack_random_target(&mut self, game: &mut GameImpl) {
        // Save up troops until we reach the trigger ratio
        if !self.has_trigger_ratio_troops(game) {
            return;
        }
        // Retaliate against incoming attacks
        if let Some(incoming) = self.find_incoming_attack_player(game) {
            self.send_attack(game, incoming, true);
            return;
        }
        // Select a traitor as an enemy
        if let Some(to_attack) = self.get_neighbor_traitor_to_attack(game)
            && self.chance(3.0)
        {
            self.send_attack(game, to_attack, false);
            return;
        }
        // Choose a new enemy randomly
        let neighbors = game.player(self.player).nearby();
        let shuffled = self.random.borrow_mut().shuffle_array(&neighbors);
        for neighbor in shuffled {
            if neighbor == 0 {
                continue;
            }
            if self.is_friendly(game, neighbor) {
                continue;
            }
            let neighbor_type = game.player(neighbor).player_type();
            if (neighbor_type == PlayerType::Nation || neighbor_type == PlayerType::Human)
                && self.chance(2.0)
            {
                continue;
            }
            self.send_attack(game, neighbor, false);
            return;
        }
    }

    /// `isBorderingNukedTerritory()`.
    pub fn is_bordering_nuked_territory(&self, game: &GameImpl) -> bool {
        let map = &game.map;
        for tile in game.player(self.player).border_tiles().iter() {
            for neighbor in map.neighbors(tile) {
                if map.is_land(neighbor) && !map.has_owner(neighbor) && map.has_fallout(neighbor) {
                    return true;
                }
            }
        }
        false
    }

    /// `forceSendAttack(target)`: half our troops, whatever the reserve.
    pub fn force_send_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        let troops = game.player(self.player).troops() / 2.0;
        self.add_attack(game, troops, target);
    }

    fn add_attack(&self, game: &mut GameImpl, troops: f64, target: SmallId) {
        let target_id = (target != 0).then(|| game.player(target).id().to_string());
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        )));
    }

    /// `sendAttack(target, force)`: a land attack when we share a border, else a boat attack on a player.
    pub fn send_attack(&mut self, game: &mut GameImpl, target: SmallId, force: bool) {
        if !force && !self.should_attack(game, target) {
            return;
        }
        if game.player(self.player).shares_border_with(target) {
            self.send_land_attack(game, target);
        } else if target != 0 {
            self.send_boat_attack(game, target);
        }
    }

    /// `sendLandAttack(target)`.
    pub fn send_land_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        let max_troops = game.player(self.player).max_troops();
        let reserve_ratio = if target != 0 {
            self.reserve_ratio
        } else {
            self.expand_ratio
        };
        let target_troops = max_troops * reserve_ratio;
        let our_troops = game.player(self.player).troops();
        let troops = if target != 0
            && game.player(target).player_type() == PlayerType::Bot
            && game.player(self.player).player_type() != PlayerType::Bot
        {
            self.calculate_bot_attack_troops(
                game,
                target,
                our_troops - target_troops - self.bot_attack_troops_sent,
            )
        } else {
            our_troops - target_troops
        };
        if troops < 1.0 {
            return;
        }
        self.add_attack(game, troops, target);
        if target != 0 {
            self.maybe_send_emoji(game, target);
        }
    }

    /// `sendBoatAttack(other)`.
    pub fn send_boat_attack(&mut self, game: &mut GameImpl, other: SmallId) {
        let map = &game.map;
        let ours: Vec<u32> = game
            .player(self.player)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let theirs: Vec<u32> = game
            .player(other)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let Some((_, landing)) = closest_two_tiles(map, ours, theirs) else {
            return;
        };
        let our_troops = game.player(self.player).troops();
        let troops = if game.player(other).player_type() == PlayerType::Bot {
            self.calculate_bot_attack_troops(game, other, our_troops / 5.0)
        } else {
            our_troops / 5.0
        };
        if troops < 1.0 {
            return;
        }
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            landing,
            Some(troops),
        )));
        self.maybe_send_emoji(game, other);
    }

    /// `calculateBotAttackTroops(target, maxTroops)`.
    pub fn calculate_bot_attack_troops(
        &mut self,
        game: &GameImpl,
        target: SmallId,
        max_troops: f64,
    ) -> f64 {
        if game.config().game_config().difficulty == Difficulty::Easy {
            self.bot_attack_troops_sent += max_troops;
            return max_troops;
        }
        let target_troops = game.player(target).troops();
        let mut troops = target_troops * 4.0;
        // Don't send more troops than maxTroops (keep a reserve)
        if troops > max_troops {
            // Without enough troops for a big enough bot attack, skip it
            if max_troops < target_troops * 2.0 {
                troops = 0.0;
            } else {
                troops = max_troops;
            }
        }
        self.bot_attack_troops_sent += troops;
        troops
    }

    /// `maybeSendEmoji(enemy)`: a heckle to a human, at most every 300 ticks per human.
    pub fn maybe_send_emoji(&mut self, game: &mut GameImpl, enemy: SmallId) {
        if game.player(self.player).player_type() == PlayerType::Bot {
            return;
        }
        if game.player(enemy).player_type() != PlayerType::Human {
            return;
        }
        let ticks = game.ticks();
        let last_sent = self
            .last_emoji_sent
            .get(&enemy)
            .map_or(-300.0, |&t| t as f64);
        if ticks as f64 - last_sent <= 300.0 {
            return;
        }
        self.last_emoji_sent.insert(enemy, ticks);
        let emoji = self.rand_element(&emoji_heckle());
        let id = game.player(enemy).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.player, id, emoji)));
    }
}
