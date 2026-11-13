// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/BotBehavior.ts of v0.26.14: attack and alliance decisions shared by tribes and fake humans.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! This build predates the `AllianceBehavior` split: alliance answers live here, and `sendAttack` always
//! sends a plain land attack. Players are small ids with `0` standing for terra nullius.

use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::Utils::EmojiIds::emoji_ids;
use crate::Game::Game::{Difficulty, PlayerType, Relation, SmallId, Tick};
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

/// `EMOJI_HECKLE`.
pub fn emoji_heckle() -> Vec<f64> {
    emoji_ids(&["\u{1f921}", "\u{1f621}"])
}

/// Attack and alliance decisions of one tribe or fake human.
pub struct BotBehaviorV26 {
    random: SharedRandom,
    player: SmallId,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    enemy: Option<SmallId>,
    enemy_updated: Option<Tick>,
}

impl BotBehaviorV26 {
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
            enemy: None,
            enemy_updated: None,
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

    /// `handleAllianceRequests()`: accept or reject every pending request.
    pub fn handle_alliance_requests(&mut self, game: &mut GameImpl) {
        for req in game.player(self.player).incoming_alliance_requests() {
            let requestor = game.alliance_request(req).requestor();
            if self.should_accept_alliance_request(game, requestor) {
                game.accept_request(req);
            } else {
                game.reject_alliance_request(req);
            }
        }
    }

    /// `shouldAcceptAllianceRequest(player, request)`.
    fn should_accept_alliance_request(&self, game: &GameImpl, requestor: SmallId) -> bool {
        let me = game.player(self.player);
        let them = game.player(requestor);
        if me.relation(requestor) < Relation::Neutral {
            return false; // Reject if hasMalice
        }
        if them.is_traitor() {
            return false; // Reject if isTraitor
        }
        if them.num_tiles_owned() as f64 > me.num_tiles_owned() as f64 * 3.0 {
            return true; // Accept if requestorIsMuchLarger
        }
        if them.alliances().len() >= 3 {
            return false; // Reject if tooManyAlliances
        }
        true
    }

    /// `handleAllianceExtensionRequests()`.
    pub fn handle_alliance_extension_requests(&mut self, game: &mut GameImpl) {
        for alliance in game.player(self.player).alliances().to_vec() {
            // Skip if no expiration yet, the ally did not ask, or we already agreed
            if !game.alliance(alliance).only_one_agreed_to_extend() {
                continue;
            }
            // A nation that is only neutral toward the human lets chance decide
            let human = game.alliance(alliance).other(self.player);
            if game.player(self.player).player_type() == PlayerType::Nation
                && game.player(self.player).relation(human) == Relation::Neutral
                && !self.chance(1.5)
            {
                continue;
            }
            let human_id = game.player(human).id().to_string();
            game.add_execution(Box::new(AllianceExtensionExecution::new(
                self.player,
                human_id,
            )));
        }
    }

    fn emoji(&self, game: &mut GameImpl, player: SmallId, emoji: f64) {
        if game.player(player).player_type() != PlayerType::Human {
            return;
        }
        let id = game.player(player).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.player, id, emoji)));
    }

    fn set_new_enemy(&mut self, game: &mut GameImpl, new_enemy: Option<SmallId>, force: bool) {
        if let Some(e) = new_enemy
            && !force
            && !self.should_attack(game, e)
        {
            return;
        }
        self.enemy = new_enemy;
        self.enemy_updated = Some(game.ticks());
    }

    fn should_attack(&self, game: &mut GameImpl, other: SmallId) -> bool {
        if game.player(self.player).is_on_same_team(other) {
            return false;
        }
        let should_attack = self.attack_chance(game, other);
        if should_attack && game.player(self.player).is_allied_with(other) {
            self.betray(game, other);
            return true;
        }
        should_attack
    }

    fn betray(&self, game: &mut GameImpl, target: SmallId) {
        let Some(alliance) = game.player(self.player).alliance_with(target) else {
            return;
        };
        game.player_mut(self.player).break_alliance(alliance);
    }

    fn attack_chance(&self, game: &GameImpl, other: SmallId) -> bool {
        if game.player(self.player).is_allied_with(other) {
            if self.should_discourage_attack(game, other) {
                self.chance(200.0)
            } else {
                self.chance(50.0)
            }
        } else if self.should_discourage_attack(game, other) {
            self.chance(4.0)
        } else {
            true
        }
    }

    fn should_discourage_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        if game.player(other).is_traitor() {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible {
            return false;
        }
        // Only discourage attacks on humans who are not traitors on easy or medium difficulty
        game.player(other).player_type() == PlayerType::Human
    }

    fn clear_enemy(&mut self) {
        self.enemy = None;
    }

    /// `forgetOldEnemies()`.
    pub fn forget_old_enemies(&mut self, game: &GameImpl) {
        let updated = self.enemy_updated.unwrap_or(0);
        if game.ticks() as f64 - updated as f64 > 100.0 {
            self.clear_enemy();
        }
    }

    fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.reserve_ratio
    }

    fn has_trigger_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.trigger_ratio
    }

    /// `checkIncomingAttacks()`: switch enemies to the largest attacker.
    fn check_incoming_attacks(&mut self, game: &mut GameImpl) {
        let mut largest_attack = 0.0;
        let mut largest_attacker: Option<SmallId> = None;
        for attack in game.player(self.player).incoming_attacks() {
            let troops = game.attack(attack).troops();
            if troops <= largest_attack {
                continue;
            }
            largest_attack = troops;
            largest_attacker = Some(game.attack(attack).attacker());
        }
        if let Some(attacker) = largest_attacker {
            self.set_new_enemy(game, Some(attacker), true);
        }
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(&self, game: &GameImpl) -> Option<SmallId> {
        let traitors: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| n != 0 && game.player(n).is_traitor())
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
                if game.player(self.player).is_allied_with(target) {
                    let e = self.rand_element(&emoji_target_ally());
                    self.emoji(game, ally, e);
                    continue;
                }
                // All checks passed, assist them
                game.player_mut(self.player).update_relation(ally, -20.0);
                self.set_new_enemy(game, Some(target), false);
                let e = self.rand_element(&emoji_assist_accept());
                self.emoji(game, ally, e);
                return;
            }
        }
    }

    /// `selectEnemy(enemies)`.
    pub fn select_enemy(&mut self, game: &mut GameImpl, enemies: &[SmallId]) -> Option<SmallId> {
        if self.enemy.is_none() {
            // Save up troops until we reach the reserve ratio
            if !self.has_reserve_ratio_troops(game) {
                return None;
            }
            // Maybe save up troops until we reach the trigger ratio
            if !self.has_trigger_ratio_troops(game) && !self.chance(10.0) {
                return None;
            }

            // Prefer neighbouring bots
            let bots: Vec<SmallId> = game
                .player(self.player)
                .nearby()
                .into_iter()
                .filter(|&n| n != 0 && game.player(n).player_type() == PlayerType::Bot)
                .collect();
            if !bots.is_empty() {
                let density = |p: SmallId| {
                    let pl = game.player(p);
                    pl.troops() / pl.num_tiles_owned() as f64
                };
                let mut lowest_density_bot: Option<SmallId> = None;
                let mut lowest_density = f64::INFINITY;
                for &bot in &bots {
                    let current = density(bot);
                    if current < lowest_density {
                        lowest_density = current;
                        lowest_density_bot = Some(bot);
                    }
                }
                if let Some(bot) = lowest_density_bot {
                    self.set_new_enemy(game, Some(bot), false);
                }
            }

            // Retaliate against incoming attacks, only after clearing bots
            if self.enemy.is_none() {
                self.check_incoming_attacks(game);
            }

            // Select the most hated player (50% chance)
            if self.enemy.is_none() && self.chance(2.0) {
                let most_hated = game
                    .player(self.player)
                    .all_relations_sorted()
                    .first()
                    .copied();
                if let Some((player, relation)) = most_hated
                    && relation == Relation::Hostile
                {
                    self.set_new_enemy(game, Some(player), false);
                }
            }

            // Select the weakest player
            if self.enemy.is_none() && !enemies.is_empty() {
                self.set_new_enemy(game, Some(enemies[0]), false);
            }

            // Select a random player
            if self.enemy.is_none() && !enemies.is_empty() {
                let pick = *self.random.borrow_mut().rand_element(enemies);
                self.set_new_enemy(game, Some(pick), false);
            }
        }
        // Sanity check, don't attack our allies or teammates
        self.enemy_sanity_check(game)
    }

    /// `selectRandomEnemy()`: used by tribes.
    pub fn select_random_enemy(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if self.enemy.is_none() {
            // Save up troops until we reach the trigger ratio
            if !self.has_trigger_ratio_troops(game) {
                return None;
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
                if game.player(neighbor).player_type() == PlayerType::Nation && self.chance(2.0) {
                    continue;
                }
                self.set_new_enemy(game, Some(neighbor), false);
            }

            // Retaliate against incoming attacks
            if self.enemy.is_none() {
                self.check_incoming_attacks(game);
            }

            // Select a traitor as an enemy
            if self.enemy.is_none()
                && let Some(to_attack) = self.get_neighbor_traitor_to_attack(game)
                && !self.is_friendly(game, to_attack)
                && self.chance(3.0)
            {
                self.set_new_enemy(game, Some(to_attack), false);
            }
        }
        // Sanity check, don't attack our allies or teammates
        self.enemy_sanity_check(game)
    }

    fn enemy_sanity_check(&mut self, game: &GameImpl) -> Option<SmallId> {
        if let Some(e) = self.enemy
            && self.is_friendly(game, e)
        {
            self.clear_enemy();
        }
        self.enemy
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

    /// `sendAttack(target)`: a land attack that keeps the reserve (or expand) ratio.
    pub fn send_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        // Skip friendly targets; breaking alliances is the caller's decision
        if target != 0 && self.is_friendly(game, target) {
            return;
        }
        let max_troops = game.player(self.player).max_troops();
        let reserve_ratio = if target != 0 {
            self.reserve_ratio
        } else {
            self.expand_ratio
        };
        let target_troops = max_troops * reserve_ratio;
        let troops = game.player(self.player).troops() - target_troops;
        if troops < 1.0 {
            return;
        }
        self.add_attack(game, troops, target);
    }
}
