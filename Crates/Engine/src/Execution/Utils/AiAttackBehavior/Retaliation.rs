// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: retaliation): retaliation, strikes and crown targets.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    /// `findIncomingAttackPlayer()`: the attacker of the largest unfriendly incoming attack.
    pub fn find_incoming_attack_player(&self, game: &GameImpl) -> Option<SmallId> {
        let me = game.player(self.player);
        let mut incoming: Vec<_> = me
            .incoming_attacks()
            .into_iter()
            .filter(|&a| Self::legacy31(game) || !self.is_friendly(game, game.attack(a).attacker()))
            .collect();
        // Bot attacks are ignored unless we are a bot.
        if me.player_type() != PlayerType::Bot {
            incoming.retain(|&a| {
                game.player(game.attack(a).attacker()).player_type() != PlayerType::Bot
            });
        }
        let mut largest_attack = 0.0;
        let mut largest_attacker = None;
        for a in incoming {
            let attack = game.attack(a);
            if attack.troops() <= largest_attack {
                continue;
            }
            largest_attack = attack.troops();
            largest_attacker = Some(attack.attacker());
        }
        largest_attacker
    }

    pub(super) fn retaliate(&mut self, game: &mut GameImpl) -> bool {
        let Some(attacker) = self.find_incoming_attack_player(game) else {
            return false;
        };
        let difficulty = game.config().game_config().difficulty;
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && self.is_ffa(game)
            && game.player(self.player).shares_border_with(attacker)
        {
            return self.send_retaliation(game, attacker);
        }
        self.send_attack(game, attacker, true)
    }

    /// Hard and Impossible keep their troops home while under attack, Medium only sometimes.
    pub(super) fn holds_boats_under_attack(&mut self, game: &GameImpl) -> bool {
        if self.find_incoming_attack_player(game).is_none() {
            return false;
        }
        match game.config().game_config().difficulty {
            Difficulty::Easy => false,
            Difficulty::Medium => self.chance(2.0),
            Difficulty::Hard | Difficulty::Impossible => true,
        }
    }

    /// Cancels the attacker's incoming troops, and pushes into its land when we can also match its
    /// home army while keeping back what `troop_send_cap` wants for every other neighbour.
    pub(super) fn send_retaliation(&mut self, game: &mut GameImpl, attacker: SmallId) -> bool {
        let mut incoming = 0.0;
        for attack in game.player(self.player).incoming_attacks() {
            if game.attack(attack).attacker() == attacker {
                incoming += game.attack(attack).troops();
            }
        }
        let mut pressing = 0.0;
        for &attack in game.player(self.player).outgoing_attacks() {
            if game.attack(attack).target() == attacker {
                pressing += game.attack(attack).troops();
            }
        }
        let troops = self.home_troops(game);
        let max_troops = game.player(self.player).max_troops();
        let spare = f64::min(
            troops - max_troops * self.expand_ratio,
            self.neighbor_troop_cap(game, Some(attacker)),
        );
        let above_reserve = troops - max_troops * self.reserve_ratio;
        let counter = incoming + f64::max(0.0, game.player(attacker).troops() - pressing);
        let send = if spare >= counter {
            f64::max(counter, f64::min(above_reserve, spare))
        } else {
            f64::min(incoming, f64::max(spare, above_reserve))
        };
        self.send_land_attack(game, attacker, Some(send), Some(attacker))
    }

    /// Troops a strike on `target` could send now: above the reserve, and within `troop_send_cap` without target.
    pub fn strike_troops(&self, game: &GameImpl, target: SmallId) -> f64 {
        let max_troops = game.player(self.player).max_troops();
        f64::min(
            self.home_troops(game) - max_troops * self.reserve_ratio,
            self.troop_send_cap(game, Some(target)),
        )
    }

    /// Attacks a player our nuke just hit, while its troops are down. Skips the trigger ratio.
    pub fn strike(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        if self.is_friendly(game, target) || !game.player(self.player).shares_border_with(target) {
            return false;
        }
        self.send_land_attack(game, target, None, Some(target))
    }

    /// A bordering runaway leader: Hard and Impossible always consider it, Medium only joins
    /// attacks already under way.
    pub(super) fn find_crown_target(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        let leader = find_runaway_leader(game)?;
        if !bordering_enemies.contains(&leader)
            || !game.player(self.player).shares_border_with(leader)
        {
            return None;
        }
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible {
            return Some(leader);
        }
        if self.troops_attacking(game, leader) >= game.player(leader).troops() * 0.1 {
            Some(leader)
        } else {
            None
        }
    }
}
