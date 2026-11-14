// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: bots): bot annexation and bot attacks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    pub(super) fn troops_attacking(&self, game: &GameImpl, target: SmallId) -> f64 {
        game.player(target)
            .incoming_attacks()
            .into_iter()
            .fold(0.0, |sum, a| sum + game.attack(a).troops())
    }

    pub(super) fn neighboring_bots(&self, game: &GameImpl) -> Vec<SmallId> {
        game.player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| {
                n != 0
                    && !self.is_friendly(game, n)
                    && game.player(n).player_type() == PlayerType::Bot
            })
            .collect()
    }

    /// The 3 nearest bots within boat range (not on Easy), skipping those our boats already sail to.
    pub(super) fn overseas_bots(&self, game: &GameImpl) -> Vec<SmallId> {
        if game.config().game_config().difficulty == Difficulty::Easy
            || game.config().is_unit_disabled(UnitType::TransportShip)
            || game.player(self.player).unit_count(UnitType::TransportShip)
                >= game.config().boat_max_number() as i64
        {
            return Vec::new();
        }
        let mut boarded: Vec<SmallId> = Vec::new();
        for boat in game.player(self.player).units_of(UnitType::TransportShip) {
            if let Some(dst) = game.unit_any(boat).target_tile() {
                boarded.push(game.owner_id(dst));
            }
        }
        let range = self.max_boat_route(game);
        let ours = game.player(self.player).largest_cluster_bounding_box;
        let mut candidates: Vec<(SmallId, f64)> = Vec::new();
        for p in game.players() {
            if game.player(p).player_type() != PlayerType::Bot
                || self.is_friendly(game, p)
                || boarded.contains(&p)
            {
                continue;
            }
            let gap = box_gap(ours, game.player(p).largest_cluster_bounding_box);
            if gap <= range {
                candidates.push((p, gap));
            }
        }
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        candidates.into_iter().take(3).map(|(b, _)| b).collect()
    }

    /// Hard and Impossible nations take bots in an order that walls others in, annexing them for free.
    pub(super) fn annexes_bots(&self, game: &GameImpl) -> bool {
        let difficulty = game.config().game_config().difficulty;
        game.player(self.player).player_type() == PlayerType::Nation
            && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
    }

    /// The engine only checks a bot for annexation when its land changes, so a 1% attack makes it
    /// hand over a bot only we border.
    pub(super) fn annex_walled_in_bots(&mut self, game: &mut GameImpl) {
        if !self.annexes_bots(game) {
            return;
        }
        let bots = self.neighboring_bots(game);
        if bots.is_empty() {
            return;
        }
        let troops = (self.home_troops(game) / 100.0).floor();
        if troops < 1.0 {
            return;
        }
        for bot in walled_in_bots(game, self.player, &bots) {
            let target_id = Self::target_id(game, bot);
            let exec = Box::new(AttackExecution::new(
                Some(troops),
                self.player,
                target_id,
                None,
                true,
            ));
            self.send(game, exec, troops);
        }
    }

    // Attacks many neighbouring bots in parallel, densest-last; `send_attack` does nothing when
    // the reserve is used up. Bots with structures come first.
    pub(super) fn attack_bots(&mut self, game: &mut GameImpl) -> bool {
        if Self::v2(game) {
            return self.attack_bots_v2(game);
        }
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

        let density = |p: SmallId| {
            let pl = game.player(p);
            pl.troops() / pl.num_tiles_owned() as f64
        };
        let mut sorted_bots = bots;
        let farming = game.has(Change::BotStructureFarming2895);
        sorted_bots.sort_by(|&a, &b| {
            let a_has = farming && Self::owns_structures(game, a);
            let b_has = farming && Self::owns_structures(game, b);
            if a_has != b_has {
                return if a_has {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (density(a) - density(b))
                .partial_cmp(&0.0)
                .unwrap_or(Ordering::Equal)
        });
        let parallelism = self.bot_attack_max_parallelism(game);
        for &bot in sorted_bots.iter().take(parallelism) {
            self.send_attack(game, bot, false);
        }
        // Short-circuit the pipeline only when troops were actually allocated to bot attacks.
        self.bot_attack_troops_sent > 0.0
    }

    pub(super) fn attack_bots_v2(&mut self, game: &mut GameImpl) -> bool {
        let overseas = game.has(Change::NationOverseasBots36);
        if overseas {
            self.bot_attack_troops_sent = 0.0;
        }
        let bots = self.neighboring_bots(game);
        if bots.is_empty() {
            if overseas {
                if self.has_land_front(game) {
                    return false;
                }
                for bot in self.overseas_bots(game) {
                    if self.can_reach(game, bot) && self.send_attack(game, bot, false) {
                        return true;
                    }
                }
            }
            return false;
        }
        self.bot_attack_troops_sent = 0.0;

        let mut ordered: Vec<SmallId> = if self.annexes_bots(game) {
            plan_bot_attacks(game, self.player, &bots)
        } else {
            let density = |p: SmallId| {
                let pl = game.player(p);
                pl.troops() / pl.num_tiles_owned() as f64
            };
            let mut sorted = bots.clone();
            sorted.sort_by(|&a, &b| {
                (density(a) - density(b))
                    .partial_cmp(&0.0)
                    .unwrap_or(Ordering::Equal)
            });
            sorted
        };
        ordered.sort_by_key(|&b| std::cmp::Reverse(Self::owns_structures(game, b)));
        let parallelism = self.bot_attack_max_parallelism(game);
        let reduced: Vec<SmallId> = ordered.into_iter().take(parallelism).collect();
        for &bot in &reduced {
            self.send_attack(game, bot, false);
        }
        // Hard and Impossible only top up running attacks, so those count as well.
        self.bot_attack_troops_sent > 0.0
            || (self.annexes_bots(game)
                && game
                    .player(self.player)
                    .outgoing_attacks()
                    .iter()
                    .any(|&a| reduced.contains(&game.attack(a).target())))
    }

    pub(super) fn bot_attack_max_parallelism(&self, game: &GameImpl) -> usize {
        match game.config().game_config().difficulty {
            Difficulty::Easy => 1,
            Difficulty::Medium => {
                if self.chance(2.0) {
                    1
                } else {
                    2
                }
            }
            Difficulty::Hard => 3,
            Difficulty::Impossible => 100,
        }
    }
}
