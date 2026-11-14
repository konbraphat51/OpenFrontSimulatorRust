// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: core): whom, where and when a nation launches nukes.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{AttackPort, Decision, DecisionLog, EmojiPort};
use super::NationMIRVBehavior::is_mirv_worth_saving_for;
use super::NationUtils::{find_runaway_leader, is_save_up_within_reach, rand_territory_tile_array};
use super::SharedRandom;
use crate::Builds::Rules::Change;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::bounding_box_tiles;
use crate::Execution::Util::closest_two_tiles;
use crate::Game::Game::{
    Difficulty, GameMode, Gold, PlayerType, Relation, STRUCTURES, SmallId, Tick, UnitId, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{TileRef, eucl_dist_fn};
use crate::Pathfinding::PathFinderParabola::{ParabolaOptions, ParabolaUniversalPathFinder};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

mod Launch;
mod Pricing;
mod Safety;
mod Silos;
mod Strikes;
mod Targeting35;
mod Targets;
mod Trajectory;

/// Cap on silo levels reachable via `maybeDestroyEnemySam`'s upgrade fallback.
const MAX_NATION_SILO_UPGRADE_LEVEL: f64 = 5.0;

/// Level-weighted structure density above which the richest impossible nation pre-emptively nukes a player.
const HIGH_DENSITY_NUKE_THRESHOLD: f64 = 1.0 / 75.0;

/// Minimum sum of structure levels a player needs to qualify as a high-density nuke target.
const MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE: f64 = 5.0;

/// Share of the non-fallout land above which every FFA nation nukes the crown.
const CROWN_NEAR_WIN_SHARE: f64 = 0.5;
/// Minimum share of a blast that must hit enemy land (not water, fallout or unowned land).
fn min_blast_coverage(nuke_type: UnitType) -> f64 {
    if nuke_type == UnitType::AtomBomb {
        0.25
    } else {
        0.4
    }
}
/// Tile score bonus for a blast that hits nothing but enemy land.
const FULL_BLAST_COVERAGE_VALUE: f64 = 50_000.0;
/// A strike target has at least this share of our troops; weaker ones fall to plain attacks.
const STRIKE_MIN_TROOP_RATIO: f64 = 0.5;
/// Hard and Impossible strike only if the follow-up attack can send this share of the target's troops.
const STRIKE_MIN_STACK_RATIO: f64 = 0.5;
/// Tiles kept between a strike's blast and our land, so the front survives for the follow-up attack.
const STRIKE_BORDER_MARGIN: f64 = 10.0;
/// Ticks a strike waits for its bomb to spawn before it is dropped.
const STRIKE_SPAWN_TICKS: f64 = 10.0;
/// Hard and Impossible: ticks after a launch until that nuke's perceived price is back down to its real price.
const NUKE_PRICE_MEMORY_TICKS: i128 = 3000;

/// A nuke in flight that is followed up with an attack when it lands.
struct Strike {
    target: SmallId,
    tile: TileRef,
    tick: Tick,
    nuke: Option<UnitId>,
}

/// A nuke launched recently: (tick, target tile, type).
type SentNuke = (Tick, TileRef, UnitType);

/// The first failed attempt of `maybeDestroyEnemySam`, remembered to upgrade a silo that would have helped.
struct FailedTarget {
    target_tile: TileRef,
    covering_sam_ids: HashSet<UnitId>,
    total_bombs: f64,
}

/// One silo's contribution to the overwhelm plan.
struct AvailableSilo {
    slots: i64,
    flight_ticks: f64,
    interceptable: bool,
    silo: UnitId,
}

/// Nuke decisions of one nation.
pub struct NationNukeBehavior {
    random: SharedRandom,
    player: SmallId,
    attack_behavior: Rc<RefCell<dyn AttackPort>>,
    emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    recently_sent_nukes: Vec<SentNuke>,
    atom_bombs_launched: u32,
    atom_bomb_perceived_cost: Gold,
    hydrogen_bombs_launched: u32,
    hydrogen_bomb_perceived_cost: Gold,
    /// 1/3 of nations only throw hydrogen bombs (to reduce atom bomb spam); drawn before v0.35.0.
    is_hydro_nation: bool,
    atom_bomb_last_launch: Option<Tick>,
    hydrogen_bomb_last_launch: Option<Tick>,
    /// Hard and Impossible: nukes in flight that are followed up with an attack when they land.
    strikes: Vec<Strike>,
    /// Test hook: records the launches instead of queueing them.
    pub log: DecisionLog,
}

impl NationNukeBehavior {
    /// `new NationNukeBehavior(random, game, player, attackBehavior, emojiBehavior)`; draws the hydro-nation flag.
    pub fn new(
        random: SharedRandom,
        game: &GameImpl,
        player: SmallId,
        attack_behavior: Rc<RefCell<dyn AttackPort>>,
        emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    ) -> Self {
        let p = game.player(player);
        let atom_bomb_perceived_cost = p.unit_cost(UnitType::AtomBomb, 0);
        let hydrogen_bomb_perceived_cost = p.unit_cost(UnitType::HydrogenBomb, 0);
        let is_hydro_nation =
            !game.has(Change::NationsLikeHumans5899) && random.borrow_mut().chance(3.0);
        Self {
            random,
            player,
            attack_behavior,
            emoji_behavior,
            recently_sent_nukes: Vec::new(),
            atom_bombs_launched: 0,
            atom_bomb_perceived_cost,
            hydrogen_bombs_launched: 0,
            hydrogen_bomb_perceived_cost,
            is_hydro_nation,
            atom_bomb_last_launch: None,
            hydrogen_bomb_last_launch: None,
            strikes: Vec::new(),
            log: DecisionLog::default(),
        }
    }

    fn rnd(&self) -> std::cell::RefMut<'_, PseudoRandom> {
        self.random.borrow_mut()
    }

    fn difficulty(game: &GameImpl) -> Difficulty {
        game.config().game_config().difficulty
    }

    fn game_mode(game: &GameImpl) -> GameMode {
        game.config().game_config().game_mode
    }

    /// Atom bombs launched so far.
    pub fn atom_bombs_launched(&self) -> u32 {
        self.atom_bombs_launched
    }

    /// Hydrogen bombs launched so far.
    pub fn hydrogen_bombs_launched(&self) -> u32 {
        self.hydrogen_bombs_launched
    }

    /// `maybeSendNuke()`.
    pub fn maybe_send_nuke(&mut self, game: &mut GameImpl) {
        let silos = game.player(self.player).units_of(UnitType::MissileSilo);
        let config = game.config();
        let legacy = !config.has(Change::NationNukeRework31);
        if !legacy
            && (silos.is_empty()
                || config.is_unit_disabled(UnitType::MissileSilo)
                || (config.is_unit_disabled(UnitType::AtomBomb)
                    && config.is_unit_disabled(UnitType::HydrogenBomb)))
        {
            return;
        }

        if game.has(Change::NationsLikeHumans5899) {
            self.maybe_send_nuke_v2(game, silos);
            return;
        }
        let Some(nuke_target) = self.find_best_nuke_target(game) else {
            return;
        };

        // Don't nuke tribes (as opposed to nations and humans).
        if (legacy && silos.is_empty())
            || game.player(nuke_target).player_type() == PlayerType::Bot
            || game.player(self.player).is_on_same_team(nuke_target)
            || !self
                .attack_behavior
                .borrow_mut()
                .should_attack(game, nuke_target)
        {
            return;
        }

        let hydro_cost = self.get_perceived_nuke_cost(game, UnitType::HydrogenBomb);
        let atom_cost = self.get_perceived_nuke_cost(game, UnitType::AtomBomb);
        let gold = game.player(self.player).gold();
        let nuke_type =
            if !game.config().is_unit_disabled(UnitType::HydrogenBomb) && gold >= hydro_cost {
                UnitType::HydrogenBomb
            } else if !game.config().is_unit_disabled(UnitType::AtomBomb)
                && (!self.is_hydro_nation || self.is_under_heavy_attack(game))
                && gold >= atom_cost
            {
                UnitType::AtomBomb
            } else {
                return;
            };
        let range = game.config().nuke_magnitudes(nuke_type).outer;

        let structures = game.player(nuke_target).units_of_types(STRUCTURES);
        let structure_tiles: Vec<TileRef> =
            structures.iter().map(|&u| game.unit_any(u).tile).collect();
        let difficulty = Self::difficulty(game);
        // More random tiles on Impossible to improve the chance of finding a perfect SAM outranging spot.
        let num_random_tiles = if difficulty == Difficulty::Impossible {
            30
        } else {
            10
        };
        let mut all_tiles =
            rand_territory_tile_array(&mut self.rnd(), game, nuke_target, num_random_tiles);
        all_tiles.extend(structure_tiles);

        let mut best_tile: Option<TileRef> = None;
        let mut best_value = -1.0; // -1 is important, so that land without structures can be nuked too
        self.remove_old_nuke_events(game);

        let mut seen: HashSet<TileRef> = HashSet::new();
        'outer: for &tile in &all_tiles {
            if !seen.insert(tile) {
                continue;
            }
            let mut bounding_box = bounding_box_tiles(&game.map, tile, range as i32);
            // Radius / 2 as well, in case a piece of unwanted territory lies inside the outer radius.
            bounding_box.extend(bounding_box_tiles(
                &game.map,
                tile,
                (range / 2.0).floor() as i32,
            ));
            for t in bounding_box {
                if !self.is_valid_nuke_tile(game, t, nuke_target) {
                    continue 'outer;
                }
            }
            let Some(spawn_tile) = game.player(self.player).can_build(nuke_type, tile, None) else {
                continue;
            };

            // In team games, avoid nuking the same position as a teammate.
            if Self::game_mode(game) == GameMode::Team
                && difficulty != Difficulty::Easy
                && self.is_teammate_already_nuking_this_spot(game, tile, nuke_type)
            {
                continue;
            }

            // On Hard & Impossible, avoid trajectories that enemy SAMs can intercept.
            if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
                && self.is_trajectory_interceptable_by_sam(game, spawn_tile, tile, None)
            {
                continue;
            }

            let value = self.nuke_tile_score(game, tile, &silos, &structures, nuke_type);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        let overwhelms_sams = game.has(Change::NationBehaviorUpdate3246);
        if let Some(tile) = best_tile
            && (!overwhelms_sams || best_value > 0.0 || difficulty != Difficulty::Impossible)
        {
            self.send_nuke(game, tile, nuke_type, nuke_target, 0.0);
        } else if overwhelms_sams && difficulty == Difficulty::Impossible {
            self.maybe_destroy_enemy_sam(game, nuke_target);
        }
    }

    fn cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        game.player(self.player).unit_cost(t, 0)
    }
}

impl NationNukeBehavior {
    fn structure_levels(game: &GameImpl, player: SmallId) -> f64 {
        let mut levels = 0.0;
        for s in game.player(player).units_of_types(STRUCTURES) {
            levels += game.unit_any(s).level as f64;
        }
        levels
    }
}
