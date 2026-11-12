// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationMIRVBehavior.ts: when and at whom a nation launches a MIRV.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{Decision, DecisionLog, EmojiPort};
use super::NationUtils::rand_territory_tile_array;
use super::SharedRandom;
use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Util::calculate_territory_center;
use crate::Game::Game::{Difficulty, Gold, PlayerType, SmallId, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// 30 seconds at 10 ticks/second.
const MIRV_COOLDOWN_TICKS: f64 = 300.0;

/// Share of a MIRV's warheads that must get past SAMs for it to be worth saving for or launching.
fn min_mirv_leak_share(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy => 0.0, // Never checks
        Difficulty::Medium => 0.1,
        Difficulty::Hard => 0.2,
        Difficulty::Impossible => 0.25,
    }
}

/// `MirvExecution`'s warhead cap.
const MIRV_MAX_WARHEADS: f64 = 350.0;
/// Target land per warhead its spacing works out to.
const LAND_TILES_PER_MIRV_WARHEAD: f64 = 2000.0;
/// Hard and Impossible MIRV a bordering rival with at least this share of their land, ahead of a land attack.
const RIVAL_MIN_TILE_RATIO: f64 = 0.5;
/// Target tiles sampled to estimate SAM coverage.
const MIRV_COVERAGE_SAMPLES: usize = 40;

/// `mirvLeakShare(game, attacker, target)`: estimated share of a MIRV's warheads at `target` that SAMs would miss.
#[allow(clippy::manual_clamp)]
pub fn mirv_leak_share(game: &GameImpl, attacker: SmallId, target: SmallId) -> f64 {
    let warheads = f64::min(
        MIRV_MAX_WARHEADS,
        f64::max(
            1.0,
            game.player(target).num_tiles_owned() as f64 / LAND_TILES_PER_MIRV_WARHEAD,
        ),
    );
    // Own seed, so the estimate is a pure function of the game state.
    let mut random = PseudoRandom::for_rules(
        game.config().rules(),
        (game.ticks() as f64) * 1024.0 + target as f64,
    );
    let samples = rand_territory_tile_array(&mut random, game, target, MIRV_COVERAGE_SAMPLES);
    if samples.is_empty() {
        return 1.0;
    }
    let config = game.config();
    let warheads_per_sample = warheads / samples.len() as f64;
    let mut warheads_in_range: Vec<(UnitId, f64)> = Vec::new();
    let mut covered_samples = 0.0;
    for &tile in &samples {
        let mut covered = false;
        for (sam, dist_squared) in game.nearby_units(
            tile,
            config.max_sam_range(),
            &[UnitType::SAMLauncher],
            None,
            false,
        ) {
            let unit = game.unit_any(sam);
            let owner = unit.owner;
            if owner == attacker || game.player(owner).is_friendly(attacker, false) {
                continue;
            }
            let range = config.sam_range(unit.level as f64);
            if dist_squared > range * range {
                continue;
            }
            match warheads_in_range.iter_mut().find(|(s, _)| *s == sam) {
                Some(entry) => entry.1 += warheads_per_sample,
                None => warheads_in_range.push((sam, 0.0 + warheads_per_sample)),
            }
            covered = true;
        }
        if covered {
            covered_samples += 1.0;
        }
    }

    let mut shots = 0.0;
    for (sam, in_range) in &warheads_in_range {
        shots += f64::min(game.unit_any(*sam).level as f64, *in_range);
    }
    let intercepted = f64::min(covered_samples * warheads_per_sample, shots);
    1.0 - intercepted / warheads
}

/// `mirvGetsThrough(game, attacker, target)`: enough warheads would get past the SAMs at `target`; Easy does not check.
pub fn mirv_gets_through(game: &GameImpl, attacker: SmallId, target: SmallId) -> bool {
    let min_leak_share = min_mirv_leak_share(game.config().game_config().difficulty);
    min_leak_share == 0.0 || mirv_leak_share(game, attacker, target) >= min_leak_share
}

/// `isMirvWorthSavingFor(game, player)`: a MIRV would get through at the land or city leader.
pub fn is_mirv_worth_saving_for(game: &GameImpl, player: SmallId) -> bool {
    let mut land_leader: Option<SmallId> = None;
    let mut city_leader: Option<SmallId> = None;
    for p in valid_mirv_targets(game, player) {
        if land_leader
            .is_none_or(|l| game.player(p).num_tiles_owned() > game.player(l).num_tiles_owned())
        {
            land_leader = Some(p);
        }
        if city_leader.is_none_or(|c| {
            game.player(p).unit_count(UnitType::City) > game.player(c).unit_count(UnitType::City)
        }) {
            city_leader = Some(p);
        }
    }
    if let Some(l) = land_leader
        && mirv_gets_through(game, player, l)
    {
        return true;
    }
    city_leader.is_some_and(|c| Some(c) != land_leader && mirv_gets_through(game, player, c))
}

fn valid_mirv_targets(game: &GameImpl, player: SmallId) -> Vec<SmallId> {
    game.players()
        .into_iter()
        .filter(|&p| {
            p != player
                && p != 0
                && game.player(p).player_type() != PlayerType::Bot
                && !game.player(player).is_on_same_team(p)
        })
        .collect()
}

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

        let v2 = game.config().rules().has(Change::NationsLikeHumans5899);
        if let Some(sender) = self.select_counter_mirv_target(game)
            && self.is_worth_mirving(game, sender, v2)
        {
            self.maybe_send_mirv(game, sender);
            return true;
        }

        if let Some(target) = self.select_victory_denial_target(game)
            && self.is_worth_mirving(game, target, v2)
        {
            self.maybe_send_mirv(game, target);
            return true;
        }

        if let Some(target) = self.select_steamroll_stop_target(game)
            && self.is_worth_mirving(game, target, v2)
        {
            self.maybe_send_mirv(game, target);
            return true;
        }

        if v2
            && let Some(target) = self.select_rival_target(game)
            && self.is_worth_mirving(game, target, v2)
        {
            self.maybe_send_mirv(game, target);
            return true;
        }

        false
    }

    fn is_worth_mirving(&self, game: &GameImpl, target: SmallId, v2: bool) -> bool {
        !self.was_recently_mirved(game, target)
            && (!v2 || mirv_gets_through(game, self.player, target))
    }

    /// Hard and Impossible: the biggest bordering player about our size, wiped out by a MIRV and our attack when it lands.
    fn select_rival_target(&self, game: &GameImpl) -> Option<SmallId> {
        let difficulty = Self::difficulty(game);
        if difficulty != Difficulty::Hard && difficulty != Difficulty::Impossible {
            return None;
        }
        let min_tiles = game.player(self.player).num_tiles_owned() as f64 * RIVAL_MIN_TILE_RATIO;
        let mut best: Option<SmallId> = None;
        for p in self.valid_mirv_target_players(game) {
            if game.player(self.player).is_friendly(p, false)
                || (game.player(p).num_tiles_owned() as f64) < min_tiles
            {
                continue;
            }
            if best.is_some_and(|b| {
                game.player(p).num_tiles_owned() <= game.player(b).num_tiles_owned()
            }) {
                continue;
            }
            if game.player(self.player).shares_border_with(p) {
                best = Some(p);
            }
        }
        best
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
