// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: core): attack decisions shared by tribes and nations.
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
use crate::Execution::Execution::Execution;
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

mod Boats;
mod BotTroops;
mod Bots;
mod Expansion;
mod Retaliation;
mod Strategies;
mod Targets;
mod Troops;

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
    /// Troops sent this tick (tick, troops): they only leave home when the executions init.
    sent_this_tick: Option<(u32, f64)>,
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
            sent_this_tick: None,
        }
    }

    /// True for builds with the PR 5899 nation behaviour.
    fn v2(game: &GameImpl) -> bool {
        game.has(Change::NationsLikeHumans5899)
    }

    /// True before the v0.31 rework: a chosen attack ends the turn whether or not it was sent.
    pub fn legacy31(game: &GameImpl) -> bool {
        !game.has(Change::AiAttackRework31)
    }

    /// The result an attack strategy reports: before v0.31 always true once a target was found.
    pub fn sent(game: &GameImpl, attacked: bool) -> bool {
        Self::legacy31(game) || attacked
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn troops(&self, game: &GameImpl) -> f64 {
        game.player(self.player).troops()
    }

    fn home_troops_rule(game: &GameImpl) -> bool {
        game.has(Change::NationHomeTroops36)
    }

    /// `homeTroops()`: our troops once everything sent this tick has left (v0.36.0 on).
    fn home_troops(&self, game: &GameImpl) -> f64 {
        if !Self::home_troops_rule(game) {
            return self.troops(game);
        }
        let troops = self.troops_at_init(game);
        match self.sent_this_tick {
            Some((tick, sent)) if tick == game.ticks() => f64::max(0.0, troops - sent),
            _ => troops,
        }
    }

    /// `troopsAtInit()`: nations are cut back to max troops by `PlayerExecution` before the executions init.
    fn troops_at_init(&self, game: &GameImpl) -> f64 {
        let troops = self.troops(game);
        if game.player(self.player).player_type() == PlayerType::Nation {
            f64::min(troops, game.player(self.player).max_troops())
        } else {
            troops
        }
    }

    /// `send(exec, troops)`: adds the execution and books the troops it takes from home.
    fn send(&mut self, game: &mut GameImpl, exec: Box<dyn Execution>, troops: f64) {
        let tick = game.ticks();
        let sent = match self.sent_this_tick {
            Some((t, s)) if t == tick => s,
            _ => 0.0,
        };
        let booked = f64::min(troops, self.home_troops(game));
        self.sent_this_tick = Some((tick, sent + booked));
        game.add_execution(exec);
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
        let land_front = self.has_land_front(game);
        self.land_neighbors = None;
        let routes = self.boat_routes.take().unwrap_or_default();
        self.clear_blocked_lane(game, &routes, land_front)
    }

    /// `hasLandFront()`: a land neighbour that is not friendly (only known during `maybe_attack`).
    fn has_land_front(&self, game: &GameImpl) -> bool {
        self.land_neighbors
            .iter()
            .flatten()
            .any(|&neighbor| !self.is_friendly(game, neighbor))
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

    // In team games nations attack and boat into stronger enemies, relying on donations; in FFA
    // going after someone much stronger usually loses.
    fn is_ffa(&self, game: &GameImpl) -> bool {
        // Before v0.31 the FFA caution applied in every mode.
        Self::legacy31(game) || game.config().game_config().game_mode == GameMode::FFA
    }

    fn player_center(&self, game: &GameImpl, player: SmallId) -> crate::Game::Game::Cell {
        if let Some((min, max)) = game.player(player).largest_cluster_bounding_box {
            return bounding_box_center(min, max);
        }
        calculate_bounding_box_center(&game.map, game.player(player).border_tiles().iter())
            .expect("player has border tiles")
    }
}
