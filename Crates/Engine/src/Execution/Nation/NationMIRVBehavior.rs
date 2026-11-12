// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationMIRVBehavior.ts: when and at whom a nation launches a MIRV.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{Decision, DecisionLog, EmojiPort};
use super::SharedRandom;
use crate::Builds::Rules::Change;
use crate::Execution::Util::calculate_territory_center;
use crate::Game::Game::{Difficulty, Gold, PlayerType, SmallId, UnitType};
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// 30 seconds at 10 ticks/second.
const MIRV_COOLDOWN_TICKS: f64 = 300.0;

/// MIRV decisions of one nation.
pub struct NationMIRVBehavior {
    random: SharedRandom,
    player: SmallId,
    emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    /// Test hook: records the MIRV launches instead of queueing them.
    pub log: DecisionLog,
}

impl NationMIRVBehavior {
    /// `new NationMIRVBehavior(random, game, player, emojiBehavior)`.
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    ) -> Self {
        Self {
            random,
            player,
            emoji_behavior,
            log: DecisionLog::default(),
        }
    }

    fn difficulty(game: &GameImpl) -> Difficulty {
        game.config().game_config().difficulty
    }

    fn hesitation_odds(game: &GameImpl) -> f64 {
        match Self::difficulty(game) {
            Difficulty::Easy => 2.0,
            Difficulty::Medium => 4.0,
            Difficulty::Hard => 8.0,
            Difficulty::Impossible => 16.0,
        }
    }

    /// Whole percent of the land, for exact integer comparison.
    fn victory_denial_threshold_percent(game: &GameImpl) -> f64 {
        match Self::difficulty(game) {
            Difficulty::Easy => 75.0,
            Difficulty::Medium => 65.0,
            Difficulty::Hard => 55.0,
            Difficulty::Impossible => 40.0,
        }
    }

    fn steamroll_city_gap_multiplier(game: &GameImpl) -> f64 {
        match Self::difficulty(game) {
            Difficulty::Easy => 2.0,
            Difficulty::Medium => 1.5,
            Difficulty::Hard => 1.25,
            Difficulty::Impossible => 1.15,
        }
    }

    fn steamroll_min_leader_cities(game: &GameImpl) -> f64 {
        match Self::difficulty(game) {
            Difficulty::Easy => 20.0,
            Difficulty::Medium | Difficulty::Hard => 10.0,
            Difficulty::Impossible => 8.0,
        }
    }

    /// `considerMIRV()`.
    pub fn consider_mirv(&mut self, game: &mut GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::MIRV) {
            return false;
        }
        if game
            .player(self.player)
            .units_of(UnitType::MissileSilo)
            .is_empty()
        {
            return false;
        }
        if game.player(self.player).gold() < self.cost(game, UnitType::MIRV) {
            return false;
        }

        if self.random.borrow_mut().chance(Self::hesitation_odds(game)) {
            return false;
        }

        if let Some(sender) = self.select_counter_mirv_target(game)
            && !self.was_recently_mirved(game, sender)
        {
            self.maybe_send_mirv(game, sender);
            return true;
        }

        if let Some(target) = self.select_victory_denial_target(game)
            && !self.was_recently_mirved(game, target)
        {
            self.maybe_send_mirv(game, target);
            return true;
        }

        if let Some(target) = self.select_steamroll_stop_target(game)
            && !self.was_recently_mirved(game, target)
        {
            self.maybe_send_mirv(game, target);
            return true;
        }

        false
    }

    fn select_counter_mirv_target(&self, game: &GameImpl) -> Option<SmallId> {
        let mut attackers: Vec<SmallId> = self
            .valid_mirv_target_players(game)
            .into_iter()
            .filter(|&p| self.is_inbound_mirv_from(game, p))
            .collect();
        if attackers.is_empty() {
            return None;
        }
        attackers.sort_by(|&a, &b| {
            game.player(b)
                .num_tiles_owned()
                .cmp(&game.player(a).num_tiles_owned())
        });
        Some(attackers[0])
    }

    fn select_victory_denial_target(&self, game: &GameImpl) -> Option<SmallId> {
        let total_land = game.num_land_tiles();
        if total_land == 0.0 {
            return None;
        }
        // Compared against tiles * 100; severity ranks by tile count.
        let percent = game
            .config()
            .rules()
            .has(Change::NationVictoryDenialPercent5223);
        let scaled_threshold = total_land * Self::victory_denial_threshold_percent(game);
        let (team_share_limit, solo_share_limit) = match Self::difficulty(game) {
            Difficulty::Easy => (0.9, 0.75),
            Difficulty::Medium => (0.8, 0.65),
            Difficulty::Hard => (0.7, 0.55),
            Difficulty::Impossible => (0.6, 0.4),
        };
        let mut best: Option<(SmallId, f64)> = None;
        for p in self.valid_mirv_target_players(game) {
            let mut severity = 0.0;
            if let Some(team) = game.player(p).team() {
                let team_members: Vec<SmallId> = game
                    .players()
                    .into_iter()
                    .filter(|&x| game.player(x).team() == Some(team) && x != 0)
                    .collect();
                let mut team_territory = 0.0;
                for &x in &team_members {
                    team_territory += game.player(x).num_tiles_owned() as f64;
                }
                let team_share = team_territory / total_land;
                let over = if percent {
                    team_territory * 100.0 >= scaled_threshold
                } else {
                    team_share >= team_share_limit
                };
                if over {
                    // Only the largest team member is a target when the team exceeds the threshold.
                    let mut largest_member: Option<SmallId> = None;
                    let mut largest_tiles = -1.0;
                    for &member in &team_members {
                        let tiles = game.player(member).num_tiles_owned() as f64;
                        if tiles > largest_tiles {
                            largest_tiles = tiles;
                            largest_member = Some(member);
                        }
                    }
                    severity = if largest_member == Some(p) {
                        if percent { team_territory } else { team_share }
                    } else {
                        0.0
                    };
                }
            } else {
                let tiles = game.player(p).num_tiles_owned() as f64;
                let share = tiles / total_land;
                if percent {
                    if tiles * 100.0 >= scaled_threshold {
                        severity = tiles;
                    }
                } else if share >= solo_share_limit {
                    severity = share;
                }
            }
            if severity > 0.0 && best.is_none_or(|(_, s)| severity > s) {
                best = Some((p, severity));
            }
        }
        best.map(|(p, _)| p)
    }

    fn select_steamroll_stop_target(&self, game: &GameImpl) -> Option<SmallId> {
        let valid_targets = self.valid_mirv_target_players(game);
        if valid_targets.is_empty() {
            return None;
        }

        let mut all_players: Vec<(SmallId, f64)> = game
            .players()
            .into_iter()
            .filter(|&p| p != 0)
            .map(|p| (p, self.count_cities(game, p)))
            .collect();
        all_players.sort_by(|a, b| b.1.total_cmp(&a.1));

        if all_players.len() < 2 {
            return None;
        }

        let top_player = all_players[0];
        if top_player.1 <= Self::steamroll_min_leader_cities(game) {
            return None;
        }

        let second_highest = all_players[1].1;
        let threshold = second_highest * Self::steamroll_city_gap_multiplier(game);

        if top_player.1 >= threshold {
            return valid_targets
                .contains(&top_player.0)
                .then_some(top_player.0);
        }
        None
    }

    fn was_recently_mirved(&self, game: &GameImpl, target: SmallId) -> bool {
        match game.recent_mirv_targets.get(game.player(target).id()) {
            None => false,
            Some(&last_tick) => (game.ticks() as f64) - (last_tick as f64) < MIRV_COOLDOWN_TICKS,
        }
    }

    fn record_mirv_hit(&self, game: &mut GameImpl, target: SmallId) {
        let id = game.player(target).id().to_string();
        let tick = game.ticks();
        game.recent_mirv_targets.insert(id, tick);
    }

    fn valid_mirv_target_players(&self, game: &GameImpl) -> Vec<SmallId> {
        let me = game.player(self.player);
        game.players()
            .into_iter()
            .filter(|&p| {
                p != self.player
                    && p != 0
                    && game.player(p).player_type() != PlayerType::Bot
                    && !me.is_on_same_team(p)
            })
            .collect()
    }

    fn is_inbound_mirv_from(&self, game: &GameImpl, attacker: SmallId) -> bool {
        for mirv in game.player(attacker).units_of(UnitType::MIRV) {
            let Some(dst) = game.unit_any(mirv).target_tile() else {
                continue;
            };
            if !game.has_owner(dst) {
                continue;
            }
            if game.owner_id(dst) == self.player {
                return true;
            }
        }
        false
    }

    fn maybe_send_mirv(&mut self, game: &mut GameImpl, enemy: SmallId) {
        self.emoji_behavior
            .borrow_mut()
            .maybe_send_attack_emoji(game, enemy);

        let center_tile = calculate_territory_center(game, enemy);
        if let Some(center) = center_tile
            && game
                .player(self.player)
                .can_build(UnitType::MIRV, center, None)
                .is_some()
        {
            self.log
                .queue(game, self.player, Decision::Mirv { tile: center });
            self.record_mirv_hit(game, enemy);
            self.emoji_behavior
                .borrow_mut()
                .send_nuke_emoji_to_all(game);
            let random = Rc::clone(&self.random);
            self.emoji_behavior
                .borrow_mut()
                .respond_to_mirv(game, &mut random.borrow_mut(), enemy);
        }
    }

    fn count_cities(&self, game: &GameImpl, p: SmallId) -> f64 {
        game.player(p).unit_count(UnitType::City) as f64
    }

    fn cost(&self, game: &GameImpl, unit_type: UnitType) -> Gold {
        game.player(self.player).unit_cost(unit_type, 0)
    }
}
