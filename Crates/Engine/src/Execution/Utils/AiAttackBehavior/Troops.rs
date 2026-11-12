// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: troops): troop caps and attack sizing.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    /// `shouldAttack(other)`.
    pub fn should_attack(&mut self, game: &GameImpl, other: SmallId) -> bool {
        // Always attack terra nullius, non-humans and traitors; bots and HvN games always attack.
        if other == 0
            || game.player(other).player_type() != PlayerType::Human
            || game.player(other).is_traitor()
            || game.player(self.player).player_type() == PlayerType::Bot
            || (game.has(Change::NationAiRework2984)
                && game.config().player_teams() == TeamCountConfig::HumansVsNations)
        {
            return true;
        }
        // Lower difficulties are prevented from attacking humans.
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Easy {
            let skip = if Self::legacy31(game) {
                self.chance(2.0)
            } else {
                self.random.borrow_mut().next_int(0.0, 4.0) != 0.0
            };
            if skip {
                return false;
            }
        }
        if difficulty == Difficulty::Medium && self.chance(4.0) {
            return false;
        }
        true
    }

    // Hard and Impossible nations in FFA: true when `troops` is under 20% of the target's troops.
    pub(super) fn is_attack_too_weak(&self, game: &GameImpl, troops: f64, target: SmallId) -> bool {
        if !game.has(Change::AiTroopCap4239) {
            return false;
        }
        if game.player(self.player).player_type() == PlayerType::Bot {
            return false;
        }
        if game.config().game_config().game_mode == GameMode::Team {
            return false;
        }
        // Nations under attack may retaliate freely.
        if !game.player(self.player).incoming_attacks().is_empty() {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && troops < game.player(target).troops() * 0.2
    }

    // Hard and Impossible nations in FFA: the most troops that can be sent without dropping below
    // a fraction of the strongest non-allied, non-bot neighbour (Hard 75%, Impossible 90%).
    pub(super) fn troop_send_cap(&self, game: &GameImpl, ignore: Option<SmallId>) -> f64 {
        if !game.has(Change::AiTroopCap4239) {
            return f64::INFINITY;
        }
        if Self::v2(game) {
            let mut cap = self.neighbor_troop_cap(game, ignore);
            // Nations under attack may retaliate with at least the incoming troops.
            let incoming = game.player(self.player).incoming_attacks();
            if !incoming.is_empty() {
                let total_incoming = incoming
                    .into_iter()
                    .fold(0.0, |sum, a| sum + game.attack(a).troops());
                cap = f64::max(cap, total_incoming);
            }
            return cap;
        }
        self.neighbor_troop_cap(game, ignore)
    }

    /// `troopSendCap()` without the allowance for nations under attack (v2) or with it (older builds).
    pub(super) fn neighbor_troop_cap(&self, game: &GameImpl, ignore: Option<SmallId>) -> f64 {
        let v2 = Self::v2(game);
        let me = game.player(self.player);
        if me.player_type() == PlayerType::Bot {
            return f64::INFINITY;
        }
        if game.config().game_config().game_mode == GameMode::Team {
            return f64::INFINITY;
        }
        let retain_fraction = match game.config().game_config().difficulty {
            Difficulty::Hard => 0.75,
            Difficulty::Impossible => 0.9,
            _ => return f64::INFINITY,
        };
        let mut max_neighbor_troops = 0.0;
        for n in me.nearby() {
            if n != 0
                && Some(n) != ignore
                && !self.is_friendly(game, n)
                && game.player(n).player_type() != PlayerType::Bot
                && game.player(n).troops() > max_neighbor_troops
            {
                max_neighbor_troops = game.player(n).troops();
            }
        }
        let mut cap = if max_neighbor_troops == 0.0 {
            f64::INFINITY
        } else {
            let min_retained = (max_neighbor_troops * retain_fraction).ceil();
            f64::max(0.0, self.home_troops(game) - min_retained)
        };
        if v2 {
            return cap;
        }
        // Nations under attack may retaliate with at least the incoming troops.
        let incoming = me.incoming_attacks();
        if !incoming.is_empty() {
            let total_incoming = incoming
                .into_iter()
                .fold(0.0, |sum, a| sum + game.attack(a).troops());
            cap = f64::max(cap, total_incoming);
        }
        cap
    }

    // Like `troop_send_cap`, but above 0: terra nullius cannot fight back, so it is throttled.
    pub(super) fn troop_send_cap_for_expansion(&self, game: &GameImpl) -> f64 {
        let cap = self.troop_send_cap(game, None);
        if cap > 0.0 {
            return cap;
        }
        (self.home_troops(game) * 0.05).ceil()
    }

    pub(super) fn calculate_attack_troops(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
        non_bot_troops: impl FnOnce(&Self, &GameImpl, f64) -> f64,
        opponent: Option<SmallId>,
    ) -> Option<f64> {
        let v2 = Self::v2(game);
        let max_troops = if v2 {
            let expansion = target == 0 || game.player(target).player_type() == PlayerType::Bot;
            self.reserve_basis(game, expansion)
        } else {
            game.player(self.player).max_troops()
        };
        let target_is_player = target != 0;
        let bot_with_structures = game.has(Change::BotStructureFarming2895)
            && target_is_player
            && game.player(target).player_type() == PlayerType::Bot
            && Self::owns_structures(game, target);
        // Attacking a bot with structures uses the expand ratio: they must be recaptured fast.
        let use_reserve = target_is_player && !bot_with_structures;
        let reserve_ratio = if use_reserve {
            self.reserve_ratio
        } else {
            self.expand_ratio
        };
        let target_troops = max_troops * reserve_ratio;

        let is_bot_attack = target_is_player
            && game.player(target).player_type() == PlayerType::Bot
            && game.player(self.player).player_type() != PlayerType::Bot;
        let mut troops = if is_bot_attack {
            let available = if Self::home_troops_rule(game) {
                f64::min(
                    self.troops(game) - self.bot_attack_troops_sent,
                    self.home_troops(game),
                ) - target_troops
            } else {
                self.troops(game) - target_troops - self.bot_attack_troops_sent
            };
            self.calculate_bot_attack_troops(game, target, available)
        } else {
            non_bot_troops(self, game, target_troops)
        };

        // Hard and Impossible: do not drop below the neighbour troop threshold.
        if target_is_player {
            troops = troops.min(self.troop_send_cap(game, opponent));
        } else if game.has(Change::NationExpansionCap5290) {
            troops = troops.min(self.troop_send_cap_for_expansion(game));
        }
        if troops < 1.0 {
            return None;
        }
        // Troops already attacking an opponent we take on on purpose count toward the too-weak check.
        let pile_on = if v2 && target_is_player && Some(target) == opponent {
            self.troops_attacking(game, target)
        } else {
            0.0
        };
        if target_is_player && self.is_attack_too_weak(game, troops + pile_on, target) {
            return None;
        }
        if target_is_player && game.player(self.player).player_type() == PlayerType::Nation {
            let emoji = self.emoji_behavior.clone().expect("not initialized");
            emoji.borrow_mut().maybe_send_attack_emoji(game, target);
        }
        // Only troops that will actually be sent, after the cap, are counted.
        if is_bot_attack && game.has(Change::NationExpansionCap5290) {
            self.bot_attack_troops_sent += troops;
        }
        Some(troops)
    }
}
