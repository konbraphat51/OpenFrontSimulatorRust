// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts: attack decisions shared by tribes and nations.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Players are small ids with `0` standing for terra nullius. Upstream shares one `PseudoRandom`
//! and the nation behaviours between the owner execution and this object, hence the `Rc`s.

use crate::Builds::Rules::Change;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Configuration::Config::{AttackLogicAttacker, AttackLogicDefender, AttackLogicInput};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{bounding_box_center, calculate_bounding_box_center};
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::DonateTroopExecution::DonateTroopsExecution;
use crate::Execution::Nation::BotAnnexation::{plan_bot_attacks, walled_in_bots};
use crate::Execution::Nation::NationAllianceBehavior::NationAllianceBehavior;
use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::NationUtils::{
    find_juiciest_target, find_runaway_leader, has_high_starting_gold,
};
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::EmojiIds::{
    emoji_assist_accept, emoji_assist_relation_too_low, emoji_assist_target_ally,
    emoji_assist_target_me,
};
use crate::Game::Game::{
    AttackId, Cell, Difficulty, EmojiRecipient, GameMode, GameType, PlayerType, Relation, SmallId,
    TeamCountConfig, TerrainType, UnitId, UnitType, WarshipMode,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Game::TransportShipUtils::{can_build_transport_ship, target_transport_tile};
use crate::Pathfinding::PathFinder::find_water_path;
use crate::Pathfinding::Types::Source;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

/// The attack strategies of `getAttackStrategies`.
#[derive(Clone, Copy)]
enum Strategy {
    Crown,
    Retaliate,
    Bots,
    Assist,
    Traitor,
    Afk,
    Betray,
    Nuked,
    Victim,
    Juicy,
    Hated,
    VeryWeak,
    Weakest,
    Island,
    Donate,
}

/// Attack decisions of one AI player.
pub struct AiAttackBehavior {
    random: Rc<RefCell<PseudoRandom>>,
    player: SmallId,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    alliance_behavior: Option<Rc<RefCell<NationAllianceBehavior>>>,
    emoji_behavior: Option<Rc<RefCell<NationEmojiBehavior>>>,
    bot_attack_troops_sent: f64,
    /// Our boat attacks (landed beachheads) already followed up by land, while they last.
    followed_landings: Vec<AttackId>,
    /// Only set during a `maybe_attack` call: our land neighbours.
    land_neighbors: Option<Vec<SmallId>>,
    /// Only set during a `maybe_attack` call: each boat route planned, in planning order.
    boat_routes: Option<Vec<(SmallId, Option<BoatRoute>)>>,
}

/// A planned boat attack: where it lands, how many tiles it sails, what would sink it.
#[derive(Clone, Copy)]
struct BoatRoute {
    landing: TileRef,
    length: f64,
    blocker: Option<UnitId>,
}

/// Which `find*` helper `pick` runs over the target groups.
#[derive(Clone, Copy)]
enum Finder {
    Traitor,
    Afk,
    Victim,
    Juicy,
    VeryWeak,
    Weakest,
    First,
}

/// Fewest tiles between two boxes (unknown boxes count as touching).
fn box_gap(a: Option<(Cell, Cell)>, b: Option<(Cell, Cell)>) -> f64 {
    let (Some((a_min, a_max)), Some((b_min, b_max))) = (a, b) else {
        return 0.0;
    };
    let dx = f64::max(
        0.0,
        f64::max((a_min.x - b_max.x) as f64, (b_min.x - a_max.x) as f64),
    );
    let dy = f64::max(
        0.0,
        f64::max((a_min.y - b_max.y) as f64, (b_min.y - a_max.y) as f64),
    );
    dx + dy
}

/// `CHEAPEST_TROOP_RATIO`.
const CHEAPEST_TROOP_RATIO: f64 = 0.6;

/// `canBuildTransportShip(...)` used as a JS condition: a landing tile 0 would be falsy.
fn can_build_truthy(game: &GameImpl, player: SmallId, tile: TileRef) -> bool {
    can_build_transport_ship(game, player, tile).is_some_and(|t| t != 0)
}

/// Stable ascending sort by a key, treating incomparable keys as equal (like a NaN comparator).
fn sort_by_key_f64<T>(items: &mut [T], key: impl Fn(&T) -> f64) {
    items.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal));
}

impl AiAttackBehavior {
    /// `new AiAttackBehavior(random, game, player, triggerRatio, reserveRatio, expandRatio, allianceBehavior?, emojiBehavior?)`.
    pub fn new(
        random: Rc<RefCell<PseudoRandom>>,
        player: SmallId,
        trigger_ratio: f64,
        reserve_ratio: f64,
        expand_ratio: f64,
        alliance_behavior: Option<Rc<RefCell<NationAllianceBehavior>>>,
        emoji_behavior: Option<Rc<RefCell<NationEmojiBehavior>>>,
    ) -> Self {
        Self {
            random,
            player,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            alliance_behavior,
            emoji_behavior,
            bot_attack_troops_sent: 0.0,
            followed_landings: Vec::new(),
            land_neighbors: None,
            boat_routes: None,
        }
    }

    /// True for builds with the PR 5899 nation behaviour.
    fn v2(game: &GameImpl) -> bool {
        game.config().rules().has(Change::NationsLikeHumans5899)
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn troops(&self, game: &GameImpl) -> f64 {
        game.player(self.player).troops()
    }

    fn is_friendly(&self, game: &GameImpl, other: SmallId) -> bool {
        game.player(self.player).is_friendly(other, false)
    }

    /// `maybeAttack()`: the nation entry point; returns the blocking warship the owner should clear
    /// a sea lane to (`clearBlockedLane`, Hard and Impossible).
    pub fn maybe_attack(&mut self, game: &mut GameImpl) -> Option<UnitId> {
        if !Self::v2(game) {
            self.choose_attack(game);
            return None;
        }
        self.boat_routes = Some(Vec::new());
        self.choose_attack(game);
        let mut land_front = false;
        for &neighbor in self.land_neighbors.iter().flatten() {
            if !self.is_friendly(game, neighbor) {
                land_front = true;
            }
        }
        self.land_neighbors = None;
        let routes = self.boat_routes.take().unwrap_or_default();
        self.clear_blocked_lane(game, &routes, land_front)
    }

    /// `followUpLandings()`: every tick, Hard and Impossible boats are beachheads: once one lands,
    /// attack its target by land from there, once.
    pub fn follow_up_landings(&mut self, game: &mut GameImpl) {
        if !Self::v2(game) || !self.lands_beachheads(game) {
            return;
        }
        let attacks: Vec<AttackId> = game.player(self.player).outgoing_attacks().to_vec();
        self.followed_landings.retain(|a| attacks.contains(a));
        for attack in attacks {
            let landing = game.attack(attack).source_tile();
            let Some(landing) = landing else { continue };
            if self.followed_landings.contains(&attack) {
                continue;
            }
            self.followed_landings.push(attack);
            let target = game.attack(attack).target();
            if !self.holds_land_next_to(game, target, landing) {
                continue;
            }
            self.send_land_attack(game, target, None, (target != 0).then_some(target));
        }
    }

    fn holds_land_next_to(&self, game: &GameImpl, owner: SmallId, tile: TileRef) -> bool {
        let mut nbuf = [0; 4];
        let n = game.map.neighbors4(tile, &mut nbuf);
        for &t in &nbuf[..n] {
            if game.map.is_land(t) && !game.map.is_impassable(t) && game.map.owner_id(t) == owner {
                return true;
            }
        }
        false
    }

    fn choose_attack(&mut self, game: &mut GameImpl) {
        assert!(self.alliance_behavior.is_some(), "not initialized");

        // The set's insertion order feeds the stable troop sort below.
        let mut bordering_player_set: OrderedSet<SmallId> = OrderedSet::new();
        let mut border_has_non_nuked_terra_nullius = false;
        let small_id = self.player;
        let player_neighbors;
        {
            let g: &GameImpl = game;
            for t in g.player(self.player).border_tiles().iter() {
                g.map.for_each_neighbor(t, |n| {
                    if !g.map.is_land(n) || g.map.is_impassable(n) {
                        return;
                    }
                    let owner = g.map.owner_id(n);
                    if owner == small_id {
                        return;
                    }
                    if owner != 0 {
                        bordering_player_set.insert(owner);
                    }
                    if !g.map.has_owner(n) && !g.map.has_fallout(n) {
                        border_has_non_nuked_terra_nullius = true;
                    }
                });
            }
            player_neighbors = g.player(self.player).nearby();
        }
        if Self::v2(game) {
            self.land_neighbors = Some(bordering_player_set.iter().copied().collect());
        }
        for &n in &player_neighbors {
            if n != 0 {
                bordering_player_set.insert(n);
            }
        }
        let mut bordering_players: Vec<SmallId> = bordering_player_set.iter().copied().collect();
        sort_by_key_f64(&mut bordering_players, |&p| game.player(p).troops());
        let bordering_friends: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| self.is_friendly(game, o))
            .collect();
        let bordering_enemies: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| !self.is_friendly(game, o))
            .collect();

        // Terra nullius, but not nuked territory (direct border or across a river).
        let has_non_nuked_terra_nullius =
            border_has_non_nuked_terra_nullius || player_neighbors.contains(&0);
        if has_non_nuked_terra_nullius && self.send_attack(game, 0, false) {
            return;
        }

        let hold_boats = Self::v2(game) && self.holds_boats_under_attack(game);
        if bordering_enemies.is_empty() {
            if !hold_boats && self.chance(5.0) {
                self.attack_with_random_boat(game, &[]);
            }
        } else {
            if !hold_boats && self.chance(10.0) {
                self.attack_with_random_boat(game, &bordering_enemies);
                return;
            }
            let behavior = self.alliance_behavior.clone().expect("checked above");
            behavior
                .borrow_mut()
                .maybe_send_alliance_requests(game, &bordering_enemies);
        }

        self.attack_best_target(game, &bordering_friends, &bordering_enemies);
    }

    fn attack_with_random_boat(&mut self, game: &mut GameImpl, bordering_enemies: &[SmallId]) {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return;
        }
        // The maximum number of transport ships may already be out.
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return;
        }
        let shore = self.shore_tiles(game, self.player);
        if shore.is_empty() {
            return;
        }
        let src = *self.random.borrow_mut().rand_element(&shore);

        // High-interest targets first (unowned or bot-owned), mainly relevant early on.
        let mut found = self.find_random_boat_target(game, src, bordering_enemies, true);
        if found.is_none() {
            found = self.find_random_boat_target(game, src, bordering_enemies, false);
            if found.is_none() {
                return;
            }
        }
        let (dst, launch) = found.expect("checked");
        let v2 = Self::v2(game);
        if v2 && self.sail(game, launch, dst, None).1.is_some() {
            return;
        }

        let owner = game.owner_id(dst);
        if v2 && self.lands_beachheads(game) {
            self.send_beachhead(game, owner, dst);
            return;
        }
        let cap = if owner != 0 {
            self.troop_send_cap(game, None)
        } else if game.config().rules().has(Change::NationExpansionCap5290) {
            self.troop_send_cap_for_expansion(game)
        } else {
            f64::INFINITY
        };
        let troops = (self.troops(game) / 5.0).min(cap);
        if troops < 1.0 {
            return;
        }
        // Hard and Impossible: skip when sending under 20% of the target's troops.
        if owner != 0 && self.is_attack_too_weak(game, troops, owner) {
            return;
        }
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            dst,
            Some(troops),
        )));
    }

    fn find_random_boat_target(
        &mut self,
        game: &GameImpl,
        tile: TileRef,
        bordering_enemies: &[SmallId],
        high_interest_only: bool,
    ) -> Option<(TileRef, TileRef)> {
        let x = game.x(tile) as f64;
        let y = game.y(tile) as f64;
        let mut unreachable_players: Vec<SmallId> = Vec::new();
        for _ in 0..500 {
            let (rand_x, rand_y) = {
                let mut r = self.random.borrow_mut();
                (
                    r.next_int(x - 150.0, x + 150.0),
                    r.next_int(y - 150.0, y + 150.0),
                )
            };
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if !game.is_land(rand_tile) {
                continue;
            }
            if game.map.is_impassable(rand_tile) {
                continue;
            }
            let owner = game.owner_id(rand_tile);
            if owner == self.player {
                continue;
            }
            // Skip players already known to be unreachable.
            if owner != 0 && unreachable_players.contains(&owner) {
                continue;
            }
            // No boats to players we share a border with, that looks stupid.
            if owner != 0 && bordering_enemies.contains(&owner) {
                continue;
            }
            // In FFA, no boats into players stronger than us.
            if self.is_ffa(game) && owner != 0 && game.player(owner).troops() > self.troops(game) {
                continue;
            }
            let matches_criteria = if high_interest_only {
                owner == 0 || game.player(owner).player_type() == PlayerType::Bot
            } else {
                // The owner is the receiver upstream, which matters for disconnected owners.
                owner == 0 || !game.player(owner).is_friendly(self.player, false)
            };
            if !matches_criteria {
                continue;
            }
            // `canBuildTransportShip(...) === false` is a strict comparison upstream.
            let Some(launch) = can_build_transport_ship(game, self.player, rand_tile) else {
                if owner != 0 {
                    unreachable_players.push(owner);
                }
                continue;
            };
            return Some((rand_tile, launch));
        }
        None
    }

    // `attackBestTarget` is called with friends and enemies sorted by troops (ascending).
    fn attack_best_target(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        if Self::v2(game) {
            self.attack_best_target_v2(game, bordering_friends, bordering_enemies);
            return;
        }
        // With high starting gold nations expand slowly and bots steal their structures:
        // attack bots before the ratio checks.
        if self.has_neighboring_bot_with_structures(game) && self.attack_bots(game) {
            return;
        }
        // Save up troops until the reserve ratio is reached.
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Maybe save up until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) && !self.chance(10.0) {
            return;
        }
        for strategy in Self::attack_strategies(game) {
            if self.run_strategy(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                &mut None,
            ) {
                return;
            }
        }
    }

    fn attack_best_target_v2(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        let difficulty = game.config().game_config().difficulty;
        let hard_or_impossible =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;

        // Hard and Impossible: answer attacks right away, even below the reserve and trigger ratios.
        if hard_or_impossible && self.retaliate(game) {
            return;
        }
        // A walled-in bot costs next to nothing to annex, so do not save up for it.
        self.annex_walled_in_bots(game);

        // Attack bots before the ratio checks when they hold structures, which we need back,
        // and with high starting gold, as those checks count the troops of cities built in a rush.
        if (self.has_neighboring_bot_with_structures(game)
            || (self.expands_past_city_troops(game) && self.has_trigger_ratio_troops(game, true)))
            && self.attack_bots(game)
        {
            return;
        }
        // Save up troops until the reserve ratio is reached.
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Medium: answer attacks without saving up to the trigger ratio first.
        if difficulty == Difficulty::Medium && self.retaliate(game) {
            return;
        }
        // Maybe save up until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) && !self.chance(10.0) {
            return;
        }
        let mut groups: Option<Vec<Vec<SmallId>>> = None;
        for strategy in Self::attack_strategies(game) {
            if self.run_strategy(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                &mut groups,
            ) {
                return;
            }
        }
    }

    fn attack_strategies(game: &GameImpl) -> Vec<Strategy> {
        use Strategy::*;
        if Self::v2(game) {
            return match game.config().game_config().difficulty {
                Difficulty::Easy => vec![Nuked, Bots, Retaliate, Assist, Betray, Hated, Weakest],
                Difficulty::Medium => vec![
                    Bots, Nuked, Assist, Betray, Hated, Afk, Traitor, Crown, Weakest, Island,
                    Donate,
                ],
                Difficulty::Hard => vec![
                    Bots, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Juicy, Victim,
                    Crown, Weakest, Island, Donate,
                ],
                Difficulty::Impossible => vec![
                    Bots, VeryWeak, Betray, Assist, Victim, Crown, Traitor, Juicy, Afk, Nuked,
                    Hated, Weakest, Island, Donate,
                ],
            };
        }
        let juicy = game.config().rules().has(Change::NationJuicy5294);
        match game.config().game_config().difficulty {
            Difficulty::Easy => vec![Nuked, Bots, Retaliate, Assist, Betray, Hated, Weakest],
            Difficulty::Medium => vec![
                Bots, Nuked, Retaliate, Assist, Betray, Hated, Afk, Traitor, Weakest, Island,
                Donate,
            ],
            Difficulty::Hard if !juicy => vec![
                Bots, Retaliate, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Victim,
                Weakest, Island, Donate,
            ],
            Difficulty::Impossible if !juicy => vec![
                Retaliate, Bots, VeryWeak, Assist, Traitor, Afk, Betray, Victim, Nuked, Hated,
                Weakest, Island, Donate,
            ],
            Difficulty::Hard => vec![
                Bots, Retaliate, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Juicy,
                Victim, Weakest, Island, Donate,
            ],
            Difficulty::Impossible => vec![
                Retaliate, Bots, VeryWeak, Betray, Assist, Victim, Traitor, Juicy, Afk, Nuked,
                Hated, Weakest, Island, Donate,
            ],
        }
    }

    fn run_strategy(
        &mut self,
        game: &mut GameImpl,
        strategy: Strategy,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> bool {
        if Self::v2(game) {
            return self.run_strategy_v2(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                groups,
            );
        }
        match strategy {
            Strategy::Crown => false,
            Strategy::Retaliate => match self.find_incoming_attack_player(game) {
                Some(attacker) => self.send_attack(game, attacker, true),
                None => false,
            },
            Strategy::Bots => self.attack_bots(game),
            Strategy::Assist => self.assist_allies(game),
            Strategy::Traitor => match self.find_traitor(game, bordering_enemies) {
                Some(traitor) => self.send_attack(game, traitor, false),
                None => false,
            },
            Strategy::Afk => {
                // Enemies are sorted by troops, so the first match is the weakest AFK enemy.
                let afk = bordering_enemies.iter().copied().find(|&enemy| {
                    game.player(enemy).is_disconnected()
                        && (!self.is_ffa(game)
                            || game.player(enemy).troops() < self.troops(game) * 3.0)
                });
                match afk {
                    Some(afk) => self.send_attack(game, afk, false),
                    None => false,
                }
            }
            Strategy::Betray => {
                self.maybe_betray_and_attack(game, bordering_friends, bordering_enemies)
            }
            Strategy::Nuked => {
                if self.is_bordering_nuked_territory(game) {
                    self.send_attack(game, 0, false)
                } else {
                    false
                }
            }
            Strategy::Victim => match self.find_victim(game, bordering_enemies) {
                Some(victim) => self.send_attack(game, victim, false),
                None => false,
            },
            Strategy::Juicy => match self.find_juicy_target(game, bordering_enemies) {
                Some(target) => self.send_attack(game, target, false),
                None => false,
            },
            Strategy::Hated => {
                for (other, relation) in game.player(self.player).all_relations_sorted() {
                    if relation != Relation::Hostile {
                        continue;
                    }
                    if self.is_friendly(game, other) {
                        continue;
                    }
                    if self.is_ffa(game) && game.player(other).troops() > self.troops(game) * 3.0 {
                        continue;
                    }
                    return self.send_attack(game, other, false);
                }
                false
            }
            Strategy::VeryWeak => match self.find_very_weak_enemy(game, bordering_enemies) {
                Some(enemy) => self.send_attack(game, enemy, false),
                None => false,
            },
            Strategy::Weakest => {
                if let Some(&weakest) = bordering_enemies.first() {
                    // In FFA, no attack on someone with more troops than us.
                    if !self.is_ffa(game) || game.player(weakest).troops() < self.troops(game) {
                        return self.send_attack(game, weakest, false);
                    }
                }
                false
            }
            Strategy::Island => {
                if bordering_enemies.is_empty()
                    && let Some(enemy) = self.find_nearest_island_enemy(game)
                {
                    return self.send_attack(game, enemy, false);
                }
                false
            }
            Strategy::Donate => self.donate_troops(game),
        }
    }

    /// The target groups, tried in order, each sorted by troops (ascending).
    fn target_groups(
        &mut self,
        game: &mut GameImpl,
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> Vec<Vec<SmallId>> {
        if let Some(g) = groups {
            return g.clone();
        }
        let difficulty = game.config().game_config().difficulty;
        let by_troops = |game: &GameImpl, list: &mut Vec<SmallId>| {
            sort_by_key_f64(list, |&p| game.player(p).troops());
        };
        let land_front = bordering_enemies
            .iter()
            .any(|&enemy| self.borders_by_land(game, enemy));
        let hard_or_impossible =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;
        let result = if difficulty == Difficulty::Impossible && land_front {
            let mut overseas = self.overseas_targets(game, bordering_enemies);
            by_troops(game, &mut overseas);
            vec![bordering_enemies.to_vec(), overseas]
        } else if hard_or_impossible && !land_front {
            let mut all = bordering_enemies.to_vec();
            all.extend(self.overseas_targets(game, bordering_enemies));
            by_troops(game, &mut all);
            vec![all]
        } else {
            vec![bordering_enemies.to_vec()]
        };
        *groups = Some(result.clone());
        result
    }

    /// What `finder` picks among the targets we can reach right now, from the first group where it picks anything.
    fn pick(
        &mut self,
        game: &mut GameImpl,
        finder: Finder,
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> Option<SmallId> {
        let easy = game.config().game_config().difficulty == Difficulty::Easy;
        for group in self.target_groups(game, bordering_enemies, groups) {
            let mut pool = group;
            while let Some(target) = self.run_finder(game, finder, &pool) {
                if easy || self.can_reach(game, target) {
                    return Some(target);
                }
                pool.retain(|&enemy| enemy != target);
            }
        }
        None
    }

    fn run_finder(&self, game: &GameImpl, finder: Finder, pool: &[SmallId]) -> Option<SmallId> {
        match finder {
            Finder::Traitor => self.find_traitor(game, pool),
            Finder::Afk => pool.iter().copied().find(|&enemy| {
                game.player(enemy).is_disconnected()
                    && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 3.0)
            }),
            Finder::Victim => self.find_victim(game, pool),
            Finder::Juicy => self.find_juicy_target(game, pool),
            Finder::VeryWeak => self.find_very_weak_enemy(game, pool),
            Finder::Weakest => pool.iter().copied().find(|&enemy| {
                !self.is_ffa(game) || game.player(enemy).troops() < self.troops(game)
            }),
            Finder::First => pool.first().copied(),
        }
    }

    fn run_strategy_v2(
        &mut self,
        game: &mut GameImpl,
        strategy: Strategy,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> bool {
        let difficulty = game.config().game_config().difficulty;
        let picked = |this: &mut Self,
                      game: &mut GameImpl,
                      f: Finder,
                      groups: &mut Option<Vec<Vec<SmallId>>>| {
            this.pick(game, f, bordering_enemies, groups)
        };
        match strategy {
            Strategy::Retaliate => self.retaliate(game),
            Strategy::Bots => self.attack_bots(game),
            Strategy::Assist => self.assist_allies(game),
            Strategy::Traitor => match picked(self, game, Finder::Traitor, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Afk => match picked(self, game, Finder::Afk, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Betray => {
                self.maybe_betray_and_attack(game, bordering_friends, bordering_enemies)
            }
            Strategy::Nuked => {
                if self.is_bordering_nuked_territory(game) {
                    self.send_attack(game, 0, false)
                } else {
                    false
                }
            }
            Strategy::Victim => match picked(self, game, Finder::Victim, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Juicy => match picked(self, game, Finder::Juicy, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Hated => {
                let relations = game.player(self.player).all_relations_sorted();
                for (other, relation) in relations {
                    if relation != Relation::Hostile {
                        continue;
                    }
                    if self.is_friendly(game, other) {
                        continue;
                    }
                    if self.is_ffa(game) && game.player(other).troops() > self.troops(game) * 3.0 {
                        continue;
                    }
                    if difficulty != Difficulty::Easy && !self.can_reach(game, other) {
                        continue;
                    }
                    return self.send_attack(game, other, false);
                }
                false
            }
            Strategy::VeryWeak => match picked(self, game, Finder::VeryWeak, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Weakest => match picked(self, game, Finder::Weakest, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Island => {
                if picked(self, game, Finder::First, groups).is_none()
                    && let Some(enemy) = self.find_nearest_island_enemy(game)
                {
                    return self.send_attack(game, enemy, false);
                }
                false
            }
            Strategy::Donate => self.donate_troops(game),
            Strategy::Crown => {
                let leader = self.find_crown_target(game, bordering_enemies);
                match leader {
                    Some(l) => {
                        self.should_attack(game, l) && self.send_land_attack(game, l, None, Some(l))
                    }
                    None => false,
                }
            }
        }
    }

    fn has_neighboring_bot_with_structures(&self, game: &GameImpl) -> bool {
        game.player(self.player).nearby().into_iter().any(|n| {
            n != 0
                && game.player(n).player_type() == PlayerType::Bot
                && !self.is_friendly(game, n)
                && Self::owns_structures(game, n)
        })
    }

    fn owns_structures(game: &GameImpl, player: SmallId) -> bool {
        game.player(player)
            .units()
            .iter()
            .any(|&u| game.unit_any(u).unit_type().is_structure())
    }

    fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.reserve_ratio
    }

    fn has_trigger_ratio_troops(&self, game: &GameImpl, expansion: bool) -> bool {
        if Self::v2(game) {
            return game.player(self.player).troops() / self.reserve_basis(game, expansion)
                >= self.trigger_ratio;
        }
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.trigger_ratio
    }

    /// The max troops the troop ratios refer to. With high starting gold, cities do not hold back
    /// expanding into free land and bots.
    fn reserve_basis(&self, game: &GameImpl, expansion: bool) -> f64 {
        let p = game.player(self.player);
        if expansion && self.expands_past_city_troops(game) {
            p.max_troops_without_cities()
        } else {
            p.max_troops()
        }
    }

    fn expands_past_city_troops(&self, game: &GameImpl) -> bool {
        has_high_starting_gold(game, self.player)
    }

    /// `findIncomingAttackPlayer()`: the attacker of the largest unfriendly incoming attack.
    pub fn find_incoming_attack_player(&self, game: &GameImpl) -> Option<SmallId> {
        let me = game.player(self.player);
        let mut incoming: Vec<_> = me
            .incoming_attacks()
            .into_iter()
            .filter(|&a| !self.is_friendly(game, game.attack(a).attacker()))
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

    fn retaliate(&mut self, game: &mut GameImpl) -> bool {
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
    fn holds_boats_under_attack(&mut self, game: &GameImpl) -> bool {
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
    fn send_retaliation(&mut self, game: &mut GameImpl, attacker: SmallId) -> bool {
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
        let troops = self.troops(game);
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
            self.troops(game) - max_troops * self.reserve_ratio,
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
    fn find_crown_target(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
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

    fn troops_attacking(&self, game: &GameImpl, target: SmallId) -> f64 {
        game.player(target)
            .incoming_attacks()
            .into_iter()
            .fold(0.0, |sum, a| sum + game.attack(a).troops())
    }

    fn neighboring_bots(&self, game: &GameImpl) -> Vec<SmallId> {
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

    /// Hard and Impossible nations take bots in an order that walls others in, annexing them for free.
    fn annexes_bots(&self, game: &GameImpl) -> bool {
        let difficulty = game.config().game_config().difficulty;
        game.player(self.player).player_type() == PlayerType::Nation
            && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
    }

    /// The engine only checks a bot for annexation when its land changes, so a 1% attack makes it
    /// hand over a bot only we border.
    fn annex_walled_in_bots(&mut self, game: &mut GameImpl) {
        if !self.annexes_bots(game) {
            return;
        }
        let bots = self.neighboring_bots(game);
        if bots.is_empty() {
            return;
        }
        let troops = (self.troops(game) / 100.0).floor();
        if troops < 1.0 {
            return;
        }
        for bot in walled_in_bots(game, self.player, &bots) {
            let target_id = Self::target_id(game, bot);
            game.add_execution(Box::new(AttackExecution::new(
                Some(troops),
                self.player,
                target_id,
                None,
                true,
            )));
        }
    }

    // Attacks many neighbouring bots in parallel, densest-last; `send_attack` does nothing when
    // the reserve is used up. Bots with structures come first.
    fn attack_bots(&mut self, game: &mut GameImpl) -> bool {
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
        sorted_bots.sort_by(|&a, &b| {
            let a_has = Self::owns_structures(game, a);
            let b_has = Self::owns_structures(game, b);
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

    fn attack_bots_v2(&mut self, game: &mut GameImpl) -> bool {
        let bots = self.neighboring_bots(game);
        if bots.is_empty() {
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

    fn bot_attack_max_parallelism(&self, game: &GameImpl) -> usize {
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

    fn assist_allies(&mut self, game: &mut GameImpl) -> bool {
        let emoji = self.emoji_behavior.clone().expect("not initialized");
        if game.config().disable_alliances() {
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
                if !self.send_attack(game, target, false) {
                    continue;
                }
                game.player_mut(self.player).update_relation(ally, -20.0);
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
    fn find_traitor(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
        if game.config().disable_alliances() {
            return None;
        }
        bordering_enemies.iter().copied().find(|&enemy| {
            game.player(enemy).is_traitor()
                && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 1.2)
        })
    }

    fn maybe_betray_and_attack(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        let behavior = self.alliance_behavior.clone().expect("not initialized");
        if game.config().disable_alliances() {
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
                    return self.send_attack(game, friend, true);
                }
            }
        }
        false
    }

    fn is_bordering_nuked_territory(&self, game: &GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::MissileSilo) {
            return false;
        }
        let mut nbuf = [0; 4];
        for tile in game.player(self.player).border_tiles().iter() {
            let n = game.map.neighbors4(tile, &mut nbuf);
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
    fn find_victim(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
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
    fn find_very_weak_enemy(
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
    fn find_juicy_target(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
        let candidates: Vec<SmallId> = bordering_enemies
            .iter()
            .copied()
            .filter(|&enemy| game.player(enemy).troops() <= self.troops(game) * 0.75)
            .collect();
        find_juiciest_target(game, &candidates)
    }

    fn find_nearest_island_enemy(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return None;
        }
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return None;
        }
        let has_shore = game
            .player(self.player)
            .border_tiles()
            .iter()
            .any(|t| game.map.is_shore(t));
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

    // In team games nations attack and boat into stronger enemies, relying on donations; in FFA
    // going after someone much stronger usually loses.
    fn is_ffa(&self, game: &GameImpl) -> bool {
        game.config().game_config().game_mode == GameMode::FFA
    }

    fn player_center(&self, game: &GameImpl, player: SmallId) -> crate::Game::Game::Cell {
        if let Some((min, max)) = game.player(player).largest_cluster_bounding_box {
            return bounding_box_center(min, max);
        }
        calculate_bounding_box_center(&game.map, game.player(player).border_tiles().iter())
            .expect("player has border tiles")
    }

    /// `attackRandomTarget()`.
    pub fn attack_random_target(&mut self, game: &mut GameImpl) {
        // Save up troops until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) {
            return;
        }
        // Retaliate against incoming attacks.
        if let Some(incoming) = self.find_incoming_attack_player(game)
            && self.send_attack(game, incoming, true)
        {
            return;
        }
        // Select a traitor as an enemy.
        if let Some(to_attack) = self.get_neighbor_traitor_to_attack(game, false)
            && self.chance(3.0)
            && self.send_attack(game, to_attack, false)
        {
            return;
        }
        // Choose a new enemy randomly.
        let neighbors = game.player(self.player).nearby();
        let shuffled = self.random.borrow_mut().shuffle_array(&neighbors);
        for neighbor in shuffled {
            if neighbor == 0 {
                continue;
            }
            if self.is_friendly(game, neighbor) {
                continue;
            }
            let t = game.player(neighbor).player_type();
            if (t == PlayerType::Nation || t == PlayerType::Human) && self.chance(2.0) {
                continue;
            }
            if self.send_attack(game, neighbor, false) {
                return;
            }
        }
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(
        &mut self,
        game: &GameImpl,
        include_allies: bool,
    ) -> Option<SmallId> {
        if game.config().disable_alliances() {
            return None;
        }
        let traitors: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| {
                n != 0
                    && (include_allies || !self.is_friendly(game, n))
                    && game.player(n).is_traitor()
            })
            .collect();
        if traitors.is_empty() {
            None
        } else {
            Some(*self.random.borrow_mut().rand_element(&traitors))
        }
    }

    /// `expandIntoFreeLand()`: grows into free land we border, never by boat and never into a player.
    pub fn expand_into_free_land(&mut self, game: &mut GameImpl) -> bool {
        if !self.has_land_border_with_terra_nullius(game) {
            return false;
        }
        self.send_land_attack(game, 0, None, None)
    }

    /// `forceSendAttack(target)`: attacks with half of the troops regardless of the checks.
    pub fn force_send_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        let troops = self.troops(game) / 2.0;
        let target_id = Self::target_id(game, target);
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        )));
    }

    fn target_id(game: &GameImpl, target: SmallId) -> Option<String> {
        (target != 0).then(|| game.player(target).id().to_string())
    }

    /// `sendAttack(target, force)`: true when an attack or boat was launched.
    pub fn send_attack(&mut self, game: &mut GameImpl, target: SmallId, force: bool) -> bool {
        if !force && !self.should_attack(game, target) {
            return false;
        }
        if target != 0 {
            if game.player(self.player).shares_border_with(target) {
                self.send_land_attack(game, target, None, None)
            } else {
                self.send_boat_attack(game, target)
            }
        } else if self.has_land_border_with_terra_nullius(game) {
            // `sharesBorderWith(TerraNullius)` counts water as terra nullius, so land is checked.
            self.send_land_attack(game, target, None, None)
        } else {
            self.send_boat_attack_to_nearby_terra_nullius(game)
        }
    }

    fn has_land_border_with_terra_nullius(&self, game: &GameImpl) -> bool {
        let map = &game.map;
        let mut nbuf = [0; 4];
        for border in game.player(self.player).border_tiles().iter() {
            let n = map.neighbors4(border, &mut nbuf);
            for &neighbor in &nbuf[..n] {
                if map.is_land(neighbor) && !map.is_impassable(neighbor) && !map.has_owner(neighbor)
                {
                    return true;
                }
            }
        }
        false
    }

    /// The player's shore border tiles, in border-set order.
    fn shore_tiles(&self, game: &GameImpl, player: SmallId) -> Vec<TileRef> {
        game.player(player)
            .border_tiles()
            .iter()
            .filter(|&t| game.map.is_shore(t))
            .collect()
    }

    // Scans shore border tiles (every 10th) for unowned land 5 tiles away across water in each
    // cardinal direction and sends a transport ship to the first match.
    fn send_boat_attack_to_nearby_terra_nullius(&mut self, game: &mut GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return false;
        }
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return false;
        }
        const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let shores = self.shore_tiles(game, self.player);
        let mut i = 0;
        while i < shores.len() {
            let border = shores[i];
            let bx = game.x(border) as i32;
            let by = game.y(border) as i32;
            for (dx, dy) in DIRECTIONS {
                let (x1, y1) = (bx + dx, by + dy);
                if !game.is_valid_coord(x1, y1) {
                    continue;
                }
                if !game.is_water(game.tile_ref(x1, y1)) {
                    continue;
                }
                let (nx, ny) = (bx + dx * 5, by + dy * 5);
                if !game.is_valid_coord(nx, ny) {
                    continue;
                }
                let tile = game.tile_ref(nx, ny);
                if !game.is_land(tile) {
                    continue;
                }
                if game.map.is_impassable(tile) {
                    continue;
                }
                if game.has_owner(tile) {
                    continue;
                }
                if game.map.has_fallout(tile) {
                    continue;
                }
                let launch = can_build_transport_ship(game, self.player, tile);
                if Self::v2(game) {
                    let Some(launch) = launch else { continue };
                    if self.sail(game, launch, tile, None).1.is_some() {
                        continue;
                    }
                    if self.lands_beachheads(game) {
                        return self.send_beachhead(game, 0, tile);
                    }
                } else if !launch.is_some_and(|t| t != 0) {
                    continue;
                }
                let troops = if game.config().rules().has(Change::NationExpansionCap5290) {
                    (self.troops(game) / 5.0).min(self.troop_send_cap_for_expansion(game))
                } else {
                    self.troops(game) / 5.0
                };
                if troops < 1.0 {
                    return false;
                }
                game.add_execution(Box::new(TransportShipExecution::new(
                    self.player,
                    tile,
                    Some(troops),
                )));
                return true;
            }
            i += 10;
        }
        false
    }

    /// `shouldAttack(other)`.
    pub fn should_attack(&mut self, game: &GameImpl, other: SmallId) -> bool {
        // Always attack terra nullius, non-humans and traitors; bots and HvN games always attack.
        if other == 0
            || game.player(other).player_type() != PlayerType::Human
            || game.player(other).is_traitor()
            || game.player(self.player).player_type() == PlayerType::Bot
            || game.config().player_teams() == TeamCountConfig::HumansVsNations
        {
            return true;
        }
        // Lower difficulties are prevented from attacking humans.
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Easy && self.random.borrow_mut().next_int(0.0, 4.0) != 0.0 {
            return false;
        }
        if difficulty == Difficulty::Medium && self.chance(4.0) {
            return false;
        }
        true
    }

    // Hard and Impossible nations in FFA: true when `troops` is under 20% of the target's troops.
    fn is_attack_too_weak(&self, game: &GameImpl, troops: f64, target: SmallId) -> bool {
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
    fn troop_send_cap(&self, game: &GameImpl, ignore: Option<SmallId>) -> f64 {
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
    fn neighbor_troop_cap(&self, game: &GameImpl, ignore: Option<SmallId>) -> f64 {
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
            f64::max(0.0, me.troops() - min_retained)
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
    fn troop_send_cap_for_expansion(&self, game: &GameImpl) -> f64 {
        let cap = self.troop_send_cap(game, None);
        if cap > 0.0 {
            return cap;
        }
        (self.troops(game) * 0.05).ceil()
    }

    fn calculate_attack_troops(
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
        let bot_with_structures = target_is_player
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
            self.calculate_bot_attack_troops(
                game,
                target,
                self.troops(game) - target_troops - self.bot_attack_troops_sent,
            )
        } else {
            non_bot_troops(self, game, target_troops)
        };

        // Hard and Impossible: do not drop below the neighbour troop threshold.
        if target_is_player {
            troops = troops.min(self.troop_send_cap(game, opponent));
        } else if game.config().rules().has(Change::NationExpansionCap5290) {
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
        if is_bot_attack && game.config().rules().has(Change::NationExpansionCap5290) {
            self.bot_attack_troops_sent += troops;
        }
        Some(troops)
    }

    fn send_land_attack(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
        fixed_troops: Option<f64>,
        opponent: Option<SmallId>,
    ) -> bool {
        let Some(troops) = self.calculate_attack_troops(
            game,
            target,
            |this, g, target_troops| fixed_troops.unwrap_or_else(|| this.troops(g) - target_troops),
            opponent,
        ) else {
            return false;
        };
        let target_id = Self::target_id(game, target);
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        )));
        true
    }

    fn send_boat_attack(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        if Self::v2(game) {
            return self.send_boat_attack_v2(game, target);
        }
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return false;
        }
        let my_shore = self.shore_tiles(game, self.player);
        let their_shore = self.shore_tiles(game, target);
        let Some((_, closest_y)) = closest_two_tiles(
            &game.map,
            my_shore.iter().copied(),
            their_shore.iter().copied(),
        ) else {
            return false;
        };
        if !can_build_truthy(game, self.player, closest_y) {
            return false;
        }
        let Some(troops) =
            self.calculate_attack_troops(game, target, |this, g, _| this.troops(g) / 5.0, None)
        else {
            return false;
        };
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            closest_y,
            Some(troops),
        )));
        true
    }

    fn send_boat_attack_v2(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        let Some(route) = self.boat_route(game, target) else {
            return false;
        };
        // A beachhead boat is cheap enough to risk past a warship, one at a time.
        let beachhead = self.lands_beachheads(game);
        if route.blocker.is_some()
            && (!beachhead || game.player(self.player).unit_count(UnitType::TransportShip) > 0)
        {
            return false;
        }
        if beachhead {
            return self.send_beachhead(game, target, route.landing);
        }
        let Some(troops) =
            self.calculate_attack_troops(game, target, |this, g, _| this.troops(g) / 5.0, None)
        else {
            return false;
        };
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            route.landing,
            Some(troops),
        )));
        true
    }

    /// Hard and Impossible nations boat a tiny force and attack by land as soon as it lands.
    fn lands_beachheads(&self, game: &GameImpl) -> bool {
        let difficulty = game.config().game_config().difficulty;
        game.player(self.player).player_type() == PlayerType::Nation
            && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
    }

    /// A 1% boat, if the land attack following its landing would be worth making now.
    fn send_beachhead(&mut self, game: &mut GameImpl, target: SmallId, landing: TileRef) -> bool {
        let Some(troops) = self.calculate_attack_troops(
            game,
            target,
            |this, g, target_troops| this.troops(g) - target_troops,
            (target != 0).then_some(target),
        ) else {
            return false;
        };
        let boat_troops = f64::max(1.0, (self.troops(game) / 100.0).floor());
        // Only the boat leaves now: the land attack is budgeted once it lands.
        if target != 0 && game.player(target).player_type() == PlayerType::Bot {
            self.bot_attack_troops_sent += boat_troops - troops;
        }
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            landing,
            Some(boat_troops),
        )));
        true
    }

    fn borders_by_land(&self, game: &GameImpl, target: SmallId) -> bool {
        match &self.land_neighbors {
            Some(set) => set.contains(&target),
            None => game.player(self.player).shares_border_with(target),
        }
    }

    /// By land, or by a boat that passes no hostile warship and is not sailing too far.
    fn can_reach(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        if self.borders_by_land(game, target) {
            return true;
        }
        match self.boat_route(game, target) {
            Some(route) => route.blocker.is_none() && route.length <= self.max_boat_route(game),
            None => false,
        }
    }

    fn boat_route(&mut self, game: &mut GameImpl, target: SmallId) -> Option<BoatRoute> {
        if let Some(routes) = &self.boat_routes
            && let Some((_, known)) = routes.iter().find(|(t, _)| *t == target)
        {
            return *known;
        }
        let route = self.plan_boat_route(game, target);
        if let Some(routes) = self.boat_routes.as_mut() {
            routes.push((target, route));
        }
        route
    }

    /// The direct crossing, or if a warship blocks it, the shortest safe one landing elsewhere.
    fn plan_boat_route(&mut self, game: &mut GameImpl, target: SmallId) -> Option<BoatRoute> {
        if game.config().is_unit_disabled(UnitType::TransportShip)
            || game.player(self.player).unit_count(UnitType::TransportShip)
                >= game.config().boat_max_number() as i64
        {
            return None;
        }
        let ours = self.shore_tiles(game, self.player);
        let theirs = self.shore_tiles(game, target);
        let (_, closest_y) =
            closest_two_tiles(&game.map, ours.iter().copied(), theirs.iter().copied())?;
        let warships = self.hostile_warships(game);
        let direct = self.route_to(game, closest_y, &warships);
        if let Some(d) = direct
            && d.blocker.is_none()
        {
            return Some(d);
        }
        let mut best: Option<BoatRoute> = None;
        for landing in self.other_landings(game, &ours, &theirs, closest_y, &warships) {
            let route = self.route_to(game, landing, &warships);
            if let Some(r) = route
                && r.blocker.is_none()
                && best.is_none_or(|b| r.length < b.length)
            {
                best = Some(r);
            }
        }
        best.or(direct)
    }

    /// The launch follows from the landing: our shore closest to it by water.
    fn route_to(
        &mut self,
        game: &mut GameImpl,
        landing: TileRef,
        warships: &[UnitId],
    ) -> Option<BoatRoute> {
        let launch = can_build_transport_ship(game, self.player, landing)?;
        let (length, blocker) = self.sail(game, launch, landing, Some(warships));
        Some(BoatRoute {
            landing,
            length,
            blocker,
        })
    }

    /// Landing spots on their coast apart from `tried` and clear of hostile warships, nearest to
    /// our coast first: Medium tries 1, Hard 2, Impossible 4.
    fn other_landings(
        &self,
        game: &GameImpl,
        ours: &[TileRef],
        theirs: &[TileRef],
        tried: TileRef,
        warships: &[UnitId],
    ) -> Vec<TileRef> {
        let count = match game.config().game_config().difficulty {
            Difficulty::Easy => return Vec::new(),
            Difficulty::Medium => 1,
            Difficulty::Hard => 2,
            Difficulty::Impossible => 4,
        };
        let range = self.danger_range(game);
        let range_squared = range * range;
        let our_step = usize::max(1, ours.len() / 32);
        let our_sample: Vec<TileRef> = ours
            .iter()
            .enumerate()
            .filter(|(i, _)| i % our_step == 0)
            .map(|(_, &t)| t)
            .collect();
        let mut candidates: Vec<(TileRef, f64)> = Vec::new();
        let their_step = usize::max(1, theirs.len() / 32);
        let mut i = 0;
        while i < theirs.len() {
            let tile = theirs[i];
            i += their_step;
            if warships.iter().any(|&w| {
                game.map
                    .euclidean_dist_squared(game.unit_any(w).tile(), tile) as f64
                    <= range_squared
            }) {
                continue;
            }
            let mut dist = f64::INFINITY;
            for &s in &our_sample {
                dist = dist.min(game.map.manhattan_dist(s, tile) as f64);
            }
            candidates.push((tile, dist));
        }
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        let mut picked = vec![tried];
        for (tile, _) in candidates {
            if picked.len() > count {
                break;
            }
            if picked
                .iter()
                .any(|&p| game.map.manhattan_dist(p, tile) < 50)
            {
                continue;
            }
            picked.push(tile);
        }
        picked.split_off(1)
    }

    fn hostile_warships(&self, game: &GameImpl) -> Vec<UnitId> {
        game.units_of(UnitType::Warship)
            .into_iter()
            .filter(|&w| {
                let unit = game.unit_any(w);
                unit.owner_id() != self.player
                    && !unit.is_under_construction()
                    && unit.warship_state().state != WarshipMode::Docked
                    && game
                        .player(unit.owner_id())
                        .can_attack_player(self.player, true)
            })
            .collect()
    }

    /// Warship targeting range, plus what it may move while our boat approaches.
    fn danger_range(&self, game: &GameImpl) -> f64 {
        game.config().warship_targetting_range() + 20.0
    }

    /// Tiles a boat sails from `launch` to the shore at `target`, and the hostile warship that sinks it.
    /// Easy does not look, Medium checks both shores, Hard and Impossible pathfind.
    fn sail(
        &self,
        game: &mut GameImpl,
        launch: TileRef,
        target: TileRef,
        warships: Option<&[UnitId]>,
    ) -> (f64, Option<UnitId>) {
        let landing = target_transport_tile(game, self.player, target).unwrap_or(target);
        let direct = game.map.manhattan_dist(launch, landing) as f64;
        let difficulty = game.config().game_config().difficulty;
        if game.player(self.player).player_type() == PlayerType::Bot
            || difficulty == Difficulty::Easy
        {
            return (direct, None);
        }
        let hostile;
        let warships: &[UnitId] = match warships {
            Some(w) => w,
            None => {
                hostile = self.hostile_warships(game);
                &hostile
            }
        };

        let path: Option<Vec<TileRef>> = if difficulty == Difficulty::Medium || direct <= 20.0 {
            None
        } else {
            find_water_path(
                &mut game.water_manager,
                &game.map,
                Source::Single(launch),
                landing,
            )
        };
        let route: Vec<TileRef> = path.clone().unwrap_or_else(|| vec![launch, landing]);
        let length = path.as_ref().map_or(direct, |p| p.len() as f64);
        // Without a path, assume the warship can fire as soon as we launch.
        let sailed = |i: usize| if path.is_none() { 0.0 } else { i as f64 };

        let range = self.danger_range(game);
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for &t in &route {
            let (x, y) = (game.map.x(t) as f64, game.map.y(t) as f64);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        for &w in warships {
            let w_tile = game.unit_any(w).tile();
            let (wx, wy) = (game.map.x(w_tile) as f64, game.map.y(w_tile) as f64);
            if wx < min_x - range || wx > max_x + range || wy < min_y - range || wy > max_y + range
            {
                continue;
            }
            for (i, &r) in route.iter().enumerate() {
                if game.map.euclidean_dist_squared(w_tile, r) as f64 > range * range {
                    continue;
                }
                // Shell and boat close in by at most 4 tiles a tick; 10 tiles spare for its reaction.
                let ticks_left = length - sailed(i);
                if 4.0 * ticks_left >= game.map.manhattan_dist(w_tile, r) as f64 - 10.0 {
                    return (length, Some(w));
                }
                break;
            }
        }
        (length, None)
    }

    /// Hard and Impossible do not sail further than this to attack someone.
    fn max_boat_route(&self, game: &GameImpl) -> f64 {
        match game.config().game_config().difficulty {
            Difficulty::Easy | Difficulty::Medium => f64::INFINITY,
            Difficulty::Hard => 300.0,
            Difficulty::Impossible => 500.0,
        }
    }

    /// Enemies across the water (not bordering us) whose main territory is within boat range of ours.
    fn overseas_targets(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Vec<SmallId> {
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return Vec::new();
        }
        let range = self.max_boat_route(game);
        let ours = game.player(self.player).largest_cluster_bounding_box;
        game.players()
            .into_iter()
            .filter(|&p| {
                p != self.player
                    && game.player(p).player_type() != PlayerType::Bot
                    && !self.is_friendly(game, p)
                    && !bordering_enemies.contains(&p)
                    && (!self.is_ffa(game) || game.player(p).troops() < self.troops(game))
                    && box_gap(ours, game.player(p).largest_cluster_bounding_box) <= range
            })
            .collect()
    }

    /// Hard and Impossible send warships at whatever blocks the juiciest target they could not reach.
    /// Without a land front any distance counts: the sea is the only way to attack.
    fn clear_blocked_lane(
        &self,
        game: &GameImpl,
        routes: &[(SmallId, Option<BoatRoute>)],
        land_front: bool,
    ) -> Option<UnitId> {
        let difficulty = game.config().game_config().difficulty;
        if difficulty != Difficulty::Hard && difficulty != Difficulty::Impossible {
            return None;
        }
        let max_route = self.max_boat_route(game);
        let mut blocked: Vec<(SmallId, UnitId)> = Vec::new();
        for (target, route) in routes {
            let Some(route) = route else { continue };
            let Some(blocker) = route.blocker else {
                continue;
            };
            if (!land_front || route.length <= max_route)
                && (!self.is_ffa(game) || game.player(*target).troops() < self.troops(game))
            {
                blocked.push((*target, blocker));
            }
        }
        let keys: Vec<SmallId> = blocked.iter().map(|(t, _)| *t).collect();
        let target = find_juiciest_target(game, &keys)?;
        blocked.iter().find(|(t, _)| *t == target).map(|(_, b)| *b)
    }

    fn calculate_bot_attack_troops(
        &mut self,
        game: &GameImpl,
        target: SmallId,
        max_troops: f64,
    ) -> f64 {
        let old_accounting = !game.config().rules().has(Change::NationExpansionCap5290);
        if game.config().game_config().difficulty == Difficulty::Easy {
            if old_accounting {
                self.bot_attack_troops_sent += max_troops;
            }
            return max_troops;
        }
        if Self::v2(game) && game.config().game_config().difficulty != Difficulty::Medium {
            // Top up what already attacks the bot, or wait until we can send it all.
            let troops = self.bot_attack_troops_needed(game, target) * 1.3
                - self.our_stack_against(game, target);
            return if troops <= max_troops { troops } else { 0.0 };
        }
        let target_troops = game.player(target).troops();
        let mut troops = target_troops * 4.0;
        // Keep the reserve: do not send more than `max_troops`.
        if troops > max_troops {
            // Too few troops left for a big enough bot attack: skip it.
            if max_troops < target_troops * 2.0 {
                troops = 0.0;
            } else {
                troops = max_troops;
            }
        }
        if old_accounting {
            self.bot_attack_troops_sent += troops;
        }
        troops
    }

    /// Keeps the attack at its cheapest loss rate until the bot collapses below 100 tiles.
    fn bot_attack_troops_needed(&self, game: &GameImpl, bot: SmallId) -> f64 {
        let stack = f64::max(1.0, game.player(bot).troops() / CHEAPEST_TROOP_RATIO);
        let tiles = f64::max(0.0, game.player(bot).num_tiles_owned() as f64 - 100.0);
        f64::max(stack, tiles * self.bot_tile_loss(game, bot, stack))
    }

    /// Troops lost per tile at that stack: Impossible reads the terrain on a 6x6 grid over the
    /// bot's land, Hard assumes highland.
    fn bot_tile_loss(&self, game: &GameImpl, bot: SmallId, stack: f64) -> f64 {
        let mut terrains: Vec<TerrainType> = Vec::new();
        let bx = game.player(bot).largest_cluster_bounding_box;
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Impossible
            && let Some((min, max)) = bx
        {
            let width = (max.x - min.x + 1) as f64;
            let height = (max.y - min.y + 1) as f64;
            for i in 0..6 {
                for j in 0..6 {
                    let x = min.x + (((2 * i + 1) as f64 * width) / 12.0).floor() as i32;
                    let y = min.y + (((2 * j + 1) as f64 * height) / 12.0).floor() as i32;
                    let tile = game.tile_ref(x, y);
                    if game.map.owner_id(tile) == bot {
                        terrains.push(game.map.terrain_type(tile));
                    }
                }
            }
        }
        if terrains.is_empty() {
            terrains.push(TerrainType::Highland);
        }
        let me = game.player(self.player);
        let b = game.player(bot);
        let mut loss = 0.0;
        for &terrain in &terrains {
            loss += game
                .config()
                .attack_logic(&AttackLogicInput {
                    terrain,
                    attack_troops: stack,
                    attacker: AttackLogicAttacker {
                        player_type: me.player_type(),
                        num_tiles: me.num_tiles_owned() as f64,
                    },
                    defender: Some(AttackLogicDefender {
                        player_type: b.player_type(),
                        num_tiles: b.num_tiles_owned() as f64,
                        troops: b.troops(),
                        is_traitor: b.is_traitor(),
                        is_disconnected_teammate: false,
                    }),
                    defender_has_defense_post: false,
                    fallout_ratio: None,
                    border_size: 1.0,
                })
                .attacker_troop_loss;
        }
        loss / terrains.len() as f64
    }

    /// Land attacks on the same target merge, so these troops join the next one we send, while the
    /// target's attacks on us cancel out against it.
    fn our_stack_against(&self, game: &GameImpl, target: SmallId) -> f64 {
        let mut troops = 0.0;
        for &attack in game.player(self.player).outgoing_attacks() {
            let a = game.attack(attack);
            if a.target() == target && a.source_tile().is_none() {
                troops += a.troops();
            }
        }
        for attack in game.player(self.player).incoming_attacks() {
            if game.attack(attack).attacker() == target {
                troops -= game.attack(attack).troops();
            }
        }
        troops
    }

    fn donate_troops(&mut self, game: &mut GameImpl) -> bool {
        // Only in team games.
        if game.config().game_config().game_mode != GameMode::Team {
            return false;
        }
        // Not in public games (to balance HvN).
        if game.config().game_config().game_type == GameType::Public {
            return false;
        }
        if !game.config().donate_troops() {
            return false;
        }
        if !Self::v2(game) && game.winner().is_some() {
            return false;
        }
        match game.config().game_config().difficulty {
            Difficulty::Easy => return false,
            Difficulty::Medium => {
                if !self.chance(4.0) {
                    return false;
                }
            }
            Difficulty::Hard => {
                if !self.chance(2.0) {
                    return false;
                }
            }
            Difficulty::Impossible => {}
        }

        // Teammates currently in combat.
        let teammates: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(self.player).is_on_same_team(p))
            .filter(|&p| {
                !game.player(p).incoming_attacks().is_empty()
                    || !game.player(p).outgoing_attacks().is_empty()
            })
            .collect();
        if teammates.is_empty() {
            return false;
        }

        // Lowest troop percentage first.
        let mut with_percentage: Vec<(SmallId, f64)> = teammates
            .into_iter()
            .map(|t| {
                let max_troops = game.player(t).max_troops();
                (t, game.player(t).troops() / f64::max(max_troops, 1.0))
            })
            .collect();
        sort_by_key_f64(&mut with_percentage, |&(_, p)| p);

        let selected = with_percentage
            .iter()
            .map(|&(t, _)| t)
            .find(|&t| game.player(self.player).can_donate_troops(t));
        let Some(selected) = selected else {
            return false;
        };

        // Donate a portion of our troops, keeping the reserve.
        let max_troops = game.player(self.player).max_troops();
        let troops_to_keep = max_troops * self.reserve_ratio;
        let available_troops = self.troops(game) - troops_to_keep;
        if available_troops < 1.0 {
            return false;
        }
        let selected_id = game.player(selected).id().to_string();
        game.add_execution(Box::new(DonateTroopsExecution::new(
            self.player,
            selected_id,
            Some(available_troops),
        )));
        true
    }
}
