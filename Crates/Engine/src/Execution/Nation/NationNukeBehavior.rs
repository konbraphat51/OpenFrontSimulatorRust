// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts: whom, where and when a nation launches nukes.
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
        let is_hydro_nation = !game.config().rules().has(Change::NationsLikeHumans5899)
            && random.borrow_mut().chance(3.0);
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
        let legacy = !config.rules().has(Change::NationNukeRework31);
        if !legacy
            && (silos.is_empty()
                || config.is_unit_disabled(UnitType::MissileSilo)
                || (config.is_unit_disabled(UnitType::AtomBomb)
                    && config.is_unit_disabled(UnitType::HydrogenBomb)))
        {
            return;
        }

        if game.config().rules().has(Change::NationsLikeHumans5899) {
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
        if let Some(tile) = best_tile
            && (best_value > 0.0 || difficulty != Difficulty::Impossible)
        {
            self.send_nuke(game, tile, nuke_type, nuke_target, 0.0);
        } else if difficulty == Difficulty::Impossible {
            self.maybe_destroy_enemy_sam(game, nuke_target);
        }
    }

    /// `findBestNukeTarget()`.
    pub fn find_best_nuke_target(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            return self.find_best_nuke_target_v2(game);
        }
        // On Hard & Impossible with only 2 players left, target the only other one.
        let diff = Self::difficulty(game);
        if (diff == Difficulty::Hard || diff == Difficulty::Impossible)
            && game.players().len() == 2
            && let Some(other) = game.players().into_iter().find(|&p| p != self.player)
        {
            return Some(other);
        }

        // Retaliate against incoming attacks (most important).
        let incoming_attack_player = self
            .attack_behavior
            .borrow_mut()
            .find_incoming_attack_player(game);
        if incoming_attack_player.is_some() {
            return incoming_attack_player;
        }

        // On Impossible, the richest nation hunts very high structure density targets.
        if diff == Difficulty::Impossible
            && game.config().rules().has(Change::NationNukeRework31)
            && self.is_richest_nation(game)
            && self.rnd().chance(2.0)
            && let Some(dense_target) = self.find_high_density_target(game)
        {
            return Some(dense_target);
        }

        // On Impossible, prioritize nuking the crown if they have more than 50% of the map.
        let game_mode = Self::game_mode(game);
        if diff == Difficulty::Impossible && game_mode == GameMode::FFA {
            let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
            if num_tiles_without_fallout > 0.0 {
                let sorted_by_tiles = Self::sorted_by_tiles(game);
                let crown = sorted_by_tiles[0];
                if crown != self.player && !game.player(self.player).is_friendly(crown, false) {
                    let crown_share =
                        game.player(crown).num_tiles_owned() as f64 / num_tiles_without_fallout;
                    if crown_share > 0.5 {
                        return Some(crown);
                    }
                }
            }
        }

        // Assist allies: check their targets (as in assistAllies, without emojis).
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if game.player(self.player).relation(ally) < Relation::Friendly {
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    continue;
                }
                if game.player(self.player).is_friendly(target, false) {
                    continue;
                }
                return Some(target);
            }
        }

        // The most hated player; much weaker players are ignored (no nukes needed).
        let my_max_troops = game.player(self.player).max_troops();
        for (other, relation) in game.player(self.player).all_relations_sorted() {
            if relation != Relation::Hostile {
                continue;
            }
            if game.player(self.player).is_friendly(other, false) {
                continue;
            }
            let other_max_troops = game.player(other).max_troops();
            if my_max_troops >= other_max_troops * 2.0 {
                continue;
            }
            return Some(other);
        }

        // In FFAs, nuke the crown if they are far enough ahead.
        if let Some(crown_target) = self.find_ffa_crown_target(game) {
            return Some(crown_target);
        }

        // In Teams, nuke the strongest team.
        if let Some(team_target) = self.find_strongest_team_target(game) {
            return Some(team_target);
        }

        None
    }

    /// Alive players by descending tile count (stable).
    fn sorted_by_tiles(game: &GameImpl) -> Vec<SmallId> {
        let mut sorted = game.players();
        sorted.sort_by(|&a, &b| {
            game.player(b)
                .num_tiles_owned()
                .cmp(&game.player(a).num_tiles_owned())
        });
        sorted
    }

    fn is_richest_nation(&self, game: &GameImpl) -> bool {
        let my_gold = game.player(self.player).gold();
        for other in game.players() {
            if other == self.player {
                continue;
            }
            if game.player(other).player_type() != PlayerType::Nation {
                continue;
            }
            if game.player(other).gold() > my_gold {
                return false;
            }
        }
        true
    }

    fn find_high_density_target(&self, game: &GameImpl) -> Option<SmallId> {
        let mut best_target = None;
        let mut best_density = HIGH_DENSITY_NUKE_THRESHOLD;
        for other in game.players() {
            if other == self.player {
                continue;
            }
            if game.player(other).player_type() == PlayerType::Bot {
                continue;
            }
            if game.player(self.player).is_friendly(other, false) {
                continue;
            }
            let tiles_owned = game.player(other).num_tiles_owned() as f64;
            if tiles_owned == 0.0 {
                continue;
            }
            let mut level_sum = 0.0;
            for s in game.player(other).units_of_types(STRUCTURES) {
                level_sum += game.unit_any(s).level as f64;
            }
            // Skip players with too few structures regardless of density.
            if level_sum < MIN_LEVEL_SUM_FOR_HIGH_DENSITY_NUKE {
                continue;
            }
            let density = level_sum / tiles_owned;
            if density > best_density {
                best_density = density;
                best_target = Some(other);
            }
        }
        best_target
    }

    fn find_ffa_crown_target(&self, game: &GameImpl) -> Option<SmallId> {
        let difficulty = Self::difficulty(game);
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            return self.find_ffa_crown_target_v2(game);
        }
        if Self::game_mode(game) != GameMode::FFA {
            return None;
        }

        if game.players().len() <= 1 {
            return None;
        }

        let sorted_by_tiles = Self::sorted_by_tiles(game);
        let first_place = sorted_by_tiles[0];

        // If we are the crown on Impossible, target 2nd place.
        if difficulty == Difficulty::Impossible
            && first_place == self.player
            && sorted_by_tiles.len() >= 2
        {
            let second_place = sorted_by_tiles[1];
            if !game.player(self.player).is_friendly(second_place, false) {
                return Some(second_place);
            }
        }

        // Don't target ourselves or allies.
        if first_place == self.player || game.player(self.player).is_friendly(first_place, false) {
            return None;
        }

        let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
        if num_tiles_without_fallout <= 0.0 {
            return None;
        }

        let first_place_share =
            game.player(first_place).num_tiles_owned() as f64 / num_tiles_without_fallout;
        let my_share =
            game.player(self.player).num_tiles_owned() as f64 / num_tiles_without_fallout;

        let threshold = match difficulty {
            Difficulty::Easy => 0.4,
            Difficulty::Medium => 0.3,
            Difficulty::Hard => 0.2,
            Difficulty::Impossible => 0.1,
        };

        // Does first place have `threshold` more tile percentage of the map than us?
        if first_place_share - my_share > threshold {
            return Some(first_place);
        }

        None
    }

    fn find_strongest_team_target(&self, game: &GameImpl) -> Option<SmallId> {
        if Self::game_mode(game) != GameMode::Team {
            return None;
        }

        if game.players().len() <= 1 {
            return None;
        }

        let mut team_tiles: OrderedMap<String, f64> = OrderedMap::new();
        let mut team_players: OrderedMap<String, Vec<SmallId>> = OrderedMap::new();

        for p in game.players() {
            let Some(team) = game.player(p).team().cloned() else {
                continue;
            };
            let tiles = team_tiles.get(&team).copied().unwrap_or(0.0);
            team_tiles.insert(
                team.clone(),
                tiles + game.player(p).num_tiles_owned() as f64,
            );
            if let Some(players) = team_players.get_mut(&team) {
                players.push(p);
            } else {
                team_players.insert(team, vec![p]);
            }
        }

        let mut sorted_teams: Vec<(String, f64)> =
            team_tiles.iter().map(|(k, v)| (k.clone(), *v)).collect();
        sorted_teams.sort_by(|a, b| b.1.total_cmp(&a.1));

        if sorted_teams.is_empty() {
            return None;
        }

        let mut strongest_team = sorted_teams[0].0.clone();
        if game.player(self.player).team() == Some(&strongest_team) {
            if sorted_teams.len() > 1 {
                strongest_team = sorted_teams[1].0.clone();
            } else {
                return None;
            }
        }

        let target_team_players = team_players.get(&strongest_team).unwrap();
        let v2 = game.config().rules().has(Change::NationsLikeHumans5899);

        // Filter out friendly players.
        let valid_targets: Vec<SmallId> = target_team_players
            .iter()
            .copied()
            .filter(|&p| !game.player(self.player).is_friendly(p, false))
            .collect();

        if valid_targets.is_empty() {
            return None;
        }

        // Hard and Impossible: one of the three players with the most structure levels.
        let difficulty = Self::difficulty(game);
        if v2 && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible) {
            let mut most_structures: Vec<(SmallId, f64)> = valid_targets
                .iter()
                .map(|&p| (p, Self::structure_levels(game, p)))
                .filter(|&(_, levels)| levels > 0.0)
                .collect();
            most_structures.sort_by(|a, b| b.1.total_cmp(&a.1));
            most_structures.truncate(3);
            if !most_structures.is_empty() {
                return Some(self.rnd().rand_element(&most_structures).0);
            }
        }

        if self.rnd().chance(2.0) {
            // Strongest player (the later one wins ties, as upstream's reduce does).
            let mut best = valid_targets[0];
            for &current in &valid_targets[1..] {
                if game.player(best).max_troops() <= game.player(current).max_troops() {
                    best = current;
                }
            }
            Some(best)
        } else {
            // Random player.
            Some(*self.rnd().rand_element(&valid_targets))
        }
    }

    /// Simulates saving up for a MIRV: the cost the nation perceives for a nuke.
    fn get_perceived_nuke_cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            return self.get_perceived_nuke_cost_v2(game, t);
        }
        // With only 2 players left, use the actual cost (no point saving for a MIRV).
        if game.players().len() == 2 {
            return self.cost(game, t);
        }

        if game.config().is_unit_disabled(UnitType::MIRV) {
            return self.cost(game, t);
        }

        // A limited amount in team games, synced with NationStructureBehavior.
        let gold = game.player(self.player).gold();
        if Self::game_mode(game) == GameMode::Team
            && (gold > self.cost(game, UnitType::HydrogenBomb)
                || !game.config().rules().has(Change::NationNukeRework31))
        {
            return self.cost(game, t);
        }

        // The actual cost if there is already enough gold for both a MIRV and a hydro.
        if gold > self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb) {
            return self.cost(game, t);
        }

        // On Hard & Impossible, ignore the perceived cost when under heavy attack (go all-in on nukes).
        let difficulty = Self::difficulty(game);
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && self.is_under_heavy_attack(game)
        {
            return self.cost(game, t);
        }

        if t == UnitType::AtomBomb {
            self.atom_bomb_perceived_cost
        } else {
            self.hydrogen_bomb_perceived_cost
        }
    }

    fn is_under_heavy_attack(&self, game: &GameImpl) -> bool {
        let player = game.player(self.player);
        let mut total_incoming_troops = 0.0;
        for attack in player.incoming_attacks() {
            total_incoming_troops += game.attack(attack).troops();
        }
        total_incoming_troops >= player.troops()
    }

    fn remove_old_nuke_events(&mut self, game: &GameImpl) {
        let max_age = 600.0; // 600 ticks = 1 minute
        let tick = game.ticks() as f64;
        while !self.recently_sent_nukes.is_empty()
            && (self.recently_sent_nukes[0].0 as f64) + max_age < tick
        {
            self.recently_sent_nukes.remove(0);
        }
    }

    fn is_teammate_already_nuking_this_spot(
        &self,
        game: &GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
    ) -> bool {
        let our_inner_radius = game.config().nuke_magnitudes(nuke_type).inner;

        // Check whether any teammate's nuke blast radius overlaps with ours.
        for nuke in game.units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb]) {
            let unit = game.unit_any(nuke);
            let nuke_owner = unit.owner;

            // Skip our own nukes and non-teammate nukes.
            if nuke_owner == self.player || !game.player(self.player).is_friendly(nuke_owner, false)
            {
                continue;
            }

            let Some(target_tile) = unit.target_tile() else {
                continue;
            };

            let teammate_inner_radius = game.config().nuke_magnitudes(unit.unit_type).inner;

            // The blast zones overlap if the distance between targets < the sum of the two radii.
            let dist_squared = game.map.euclidean_dist_squared(tile, target_tile) as f64;
            let sum_radius = our_inner_radius + teammate_inner_radius;
            if dist_squared <= sum_radius * sum_radius {
                return true;
            }
        }

        false
    }

    /// Flight path of an AI nuke from `from` to `to` (distance-based height, always "up").
    fn nuke_trajectory(game: &GameImpl, from: TileRef, to: TileRef) -> Vec<TileRef> {
        let speed = game.config().nuke_speed(UnitType::AtomBomb);
        let path_finder = ParabolaUniversalPathFinder::new(ParabolaOptions {
            increment: Some(speed),
            distance_based_height: Some(true),
            direction_up: Some(true),
            ignore_map_bounds: None,
            legacy_curve: !game.config().rules().has(Change::MirvNormalize5188),
            legacy_reset: !game.config().rules().has(Change::SamTargetingFixes4770),
            ..Default::default()
        });
        path_finder.find_path(&game.map, from, to)
    }

    /// Mirrors NukeTrajectoryPreviewLayer.ts: can an enemy SAM intercept the flight from `spawn_tile` to `target_tile`?
    fn is_trajectory_interceptable_by_sam(
        &self,
        game: &GameImpl,
        spawn_tile: TileRef,
        target_tile: TileRef,
        excluded_sam_ids: Option<&HashSet<UnitId>>,
    ) -> bool {
        let trajectory = Self::nuke_trajectory(game, spawn_tile, target_tile);
        if trajectory.is_empty() {
            return false;
        }

        let target_range = game.config().default_nuke_targetable_range();
        let target_range_squared = target_range * target_range;
        let dist2 = |a: TileRef, b: TileRef| game.map.euclidean_dist_squared(a, b) as f64;

        let mut untargetable_start: i64 = -1;
        let mut untargetable_end: i64 = -1;
        for (i, &tile) in trajectory.iter().enumerate() {
            if untargetable_start == -1 {
                if dist2(tile, spawn_tile) > target_range_squared {
                    if dist2(tile, target_tile) < target_range_squared {
                        // Overlapping spawn and target range: no untargetable segment.
                        break;
                    } else {
                        untargetable_start = i as i64;
                    }
                }
            } else if dist2(tile, target_tile) < target_range_squared {
                untargetable_end = i as i64;
                break;
            }
        }

        let max_sam_range = game.config().max_sam_range();
        let mut i: i64 = 0;
        while i < trajectory.len() as i64 {
            // Skip the mid-air untargetable portion.
            if untargetable_start != -1 && untargetable_end != -1 && i == untargetable_start {
                i = untargetable_end - 1;
                i += 1;
                continue;
            }

            let tile = trajectory[i as usize];
            let nearby_sams =
                game.nearby_units(tile, max_sam_range, &[UnitType::SAMLauncher], None, false);

            for (sam, sam_dist_squared) in nearby_sams {
                let unit = game.unit_any(sam);
                let owner = unit.owner;
                if owner == self.player || game.player(self.player).is_friendly(owner, false) {
                    continue;
                }
                // Skip SAMs we are intentionally overwhelming.
                if excluded_sam_ids.is_some_and(|ids| ids.contains(&sam)) {
                    continue;
                }
                let range = game.config().sam_range(unit.level as f64);
                if sam_dist_squared <= range * range {
                    return true;
                }
            }
            i += 1;
        }

        false
    }

    fn is_valid_nuke_tile(&self, game: &GameImpl, t: TileRef, nuke_target: SmallId) -> bool {
        let difficulty = Self::difficulty(game);
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            let owner = game.owner_id(t);
            if owner == nuke_target {
                return true;
            }
            // Hard and Impossible: terra nullius (small islands) and other players unless teammates or allies (even AFK ones).
            return (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
                && (owner == 0 || !game.player(self.player).is_friendly(owner, true));
        }

        let owner = game.owner_id(t);
        if owner == nuke_target {
            return true;
        }
        // On Hard & Impossible, allow terra nullius (small islands) and, in team games, other non-friendly players.
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && (owner == 0
                || (Self::game_mode(game) == GameMode::Team
                    && owner != 0
                    && !game.player(self.player).is_friendly(owner, false)))
        {
            return true;
        }
        // On Easy & Medium, only tiles of the target player (nuke away from the border) to reduce nuke usage.
        false
    }

    fn nuke_tile_score(
        &self,
        game: &GameImpl,
        tile: TileRef,
        silos: &[UnitId],
        targets: &[UnitId],
        nuke_type: UnitType,
    ) -> f64 {
        let magnitude = game.config().nuke_magnitudes(nuke_type);
        let dist = eucl_dist_fn(tile, magnitude.outer, false);
        let mut tile_value = 0.0;
        for &unit_id in targets {
            let unit = game.unit_any(unit_id);
            if !dist(&game.map, unit.tile) {
                continue;
            }
            let level = unit.level as f64;
            tile_value += match unit.unit_type {
                UnitType::City => 25_000.0 * level,
                UnitType::DefensePost => 5_000.0 * level,
                UnitType::MissileSilo => 50_000.0 * level,
                UnitType::Port => 15_000.0 * level,
                UnitType::Factory => 15_000.0 * level,
                _ => 0.0,
            };
        }

        let difficulty = Self::difficulty(game);
        // Easy ignores SAMs; Medium applies a simple local SAM penalty;
        // Hard & Impossible rely on the trajectory-based interception checks of maybeSendNuke().
        if difficulty == Difficulty::Medium {
            let dist50 = eucl_dist_fn(tile, 50.0, false);
            let has_sam = targets.iter().any(|&u| {
                let unit = game.unit_any(u);
                unit.unit_type == UnitType::SAMLauncher && dist50(&game.map, unit.tile)
            });
            if has_sam {
                return -1.0;
            }
        }

        // On Impossible with a hydrogen bomb, add value for SAMs that can be outranged.
        if difficulty == Difficulty::Impossible && nuke_type == UnitType::HydrogenBomb {
            let hydro_magnitude = game.config().nuke_magnitudes(UnitType::HydrogenBomb);
            let nearby_sams = game.nearby_units(
                tile,
                hydro_magnitude.outer,
                &[UnitType::SAMLauncher],
                None,
                false,
            );

            for (sam, _) in nearby_sams {
                let sam_unit = game.unit_any(sam);
                let sam_level = sam_unit.level as f64;
                if sam_level >= 5.0 {
                    continue; // Can't outrange level 5+ SAMs
                }

                let sam_range = game.config().sam_range(sam_level);
                let dist_to_sam =
                    (game.map.euclidean_dist_squared(tile, sam_unit.tile) as f64).sqrt();

                // Significant value for destroying a SAM that can be outranged.
                if dist_to_sam > sam_range {
                    tile_value += 100_000.0 * sam_level;
                }
            }
        }

        // Prefer tiles closer to a silo (but preserve structure value).
        let silo_tiles: Vec<TileRef> = silos.iter().map(|&u| game.unit_any(u).tile).collect();
        let Some((closest_silo, _)) = closest_two_tiles(&game.map, silo_tiles, [tile]) else {
            panic!("Missing result");
        };
        let distance_squared = game.map.euclidean_dist_squared(tile, closest_silo) as f64;
        let distance_to_closest_silo = distance_squared.sqrt();
        let distance_penalty = distance_to_closest_silo * 30.0;
        let base_tile_value = tile_value;
        // Keep at least 20% of the structure value.
        tile_value = (base_tile_value * 0.2).max(tile_value - distance_penalty);

        // Don't target near recent targets.
        for &(_, recent_tile, recent_nuke_type) in &self.recently_sent_nukes {
            let recent_inner_radius = game.config().nuke_magnitudes(recent_nuke_type).inner;
            let dist_squared = game.map.euclidean_dist_squared(tile, recent_tile) as f64;
            if dist_squared <= recent_inner_radius * recent_inner_radius {
                tile_value -= 1_000_000.0;
            }
        }

        tile_value
    }

    fn send_nuke(
        &mut self,
        game: &mut GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
        target_player: SmallId,
        wait_ticks: f64,
    ) {
        let tick = game.ticks();
        self.recently_sent_nukes.push((tick, tile, nuke_type));
        let v2 = game.config().rules().has(Change::NationsLikeHumans5899);
        let perceived = if v2 {
            self.inflated_nuke_cost(game, nuke_type)
        } else if nuke_type == UnitType::AtomBomb {
            self.atom_bomb_perceived_cost
        } else {
            self.hydrogen_bomb_perceived_cost
        };
        if nuke_type == UnitType::AtomBomb {
            self.atom_bombs_launched += 1;
            // +50% each time to simulate saving up for a MIRV (more than hydro, so atom bombs lose appeal late).
            self.atom_bomb_perceived_cost = (perceived * 150) / 100;
            self.atom_bomb_last_launch = Some(tick);
        } else if nuke_type == UnitType::HydrogenBomb {
            self.hydrogen_bombs_launched += 1;
            // +25% each time to simulate saving up for a MIRV.
            self.hydrogen_bomb_perceived_cost = (perceived * 125) / 100;
            self.hydrogen_bomb_last_launch = Some(tick);
        }
        self.log.queue(
            game,
            self.player,
            Decision::Nuke {
                nuke_type,
                tile,
                wait_ticks,
            },
        );
        self.emoji_behavior
            .borrow_mut()
            .maybe_send_nuke_emoji(game, target_player);
    }

    /// On Impossible, when no good nuke target exists (score <= 0), tries to destroy enemy SAMs by
    /// overwhelming them with atom bombs: a SAM of level N intercepts N nukes before its cooldown.
    fn maybe_destroy_enemy_sam(&mut self, game: &mut GameImpl, nuke_target: SmallId) {
        if game.config().is_unit_disabled(UnitType::AtomBomb) {
            return;
        }

        // Don't launch another salvo while atom bombs are in flight.
        if !game
            .player(self.player)
            .units_of(UnitType::AtomBomb)
            .is_empty()
        {
            return;
        }

        let atom_cost = self.cost(game, UnitType::AtomBomb);
        let enemy_sams = game.player(nuke_target).units_of(UnitType::SAMLauncher);
        if enemy_sams.is_empty() {
            return;
        }

        let our_silos: Vec<UnitId> = game
            .player(self.player)
            .units_of(UnitType::MissileSilo)
            .into_iter()
            .filter(|&silo| !game.unit_any(silo).is_under_construction())
            .collect();
        if our_silos.is_empty() {
            return;
        }

        // Try each enemy SAM as a target, easiest (lowest level) first.
        let mut sorted_sams = enemy_sams.clone();
        sorted_sams.sort_by_key(|&s| game.unit_any(s).level);
        let mut needs_more_silos = false;
        // The first failed attempt, to upgrade a silo that would have helped that plan.
        let mut failed_target: Option<FailedTarget> = None;

        for target_sam in sorted_sams {
            let target_tile = game.unit_any(target_sam).tile;
            // Hitting an ally's land would break the alliance.
            if game.config().rules().has(Change::NationsLikeHumans5899)
                && self.blast_hits_friendly_land(game, target_tile, UnitType::AtomBomb, 0.0)
            {
                continue;
            }

            // All enemy SAMs whose range covers the target tile will try to intercept.
            let covering_sams = self.find_enemy_sams_covering_tile(game, target_tile);
            let covering_sam_ids: HashSet<UnitId> = covering_sams.iter().copied().collect();

            // Total interception capacity = sum of covering SAM levels.
            let mut total_interceptions = 0.0;
            for &sam in &covering_sams {
                total_interceptions += game.unit_any(sam).level as f64;
            }
            let bombs_needed = total_interceptions + 1.0;

            // NukeExecution picks the closest non-cooldown silo by Manhattan distance (via nukeSpawn);
            // the plan mirrors that order.
            let mut all_available_silos: Vec<AvailableSilo> = Vec::new();
            for &silo in &our_silos {
                let silo_unit = game.unit_any(silo);
                let available_slots =
                    silo_unit.level as i64 - silo_unit.missile_timer_queue().len() as i64;
                if available_slots <= 0 {
                    continue;
                }
                let interceptable = self.is_trajectory_interceptable_by_sam(
                    game,
                    silo_unit.tile,
                    target_tile,
                    Some(&covering_sam_ids),
                );
                // Actual parabolic flight time in ticks.
                let trajectory = Self::nuke_trajectory(game, silo_unit.tile, target_tile);
                if trajectory.is_empty() {
                    continue;
                }
                all_available_silos.push(AvailableSilo {
                    silo,
                    slots: available_slots,
                    flight_ticks: trajectory.len() as f64,
                    interceptable,
                });
            }

            // Sort by Manhattan distance to the target (matching nukeSpawn's pick order).
            all_available_silos.sort_by_key(|e| {
                game.map
                    .manhattan_dist(game.unit_any(e.silo).tile, target_tile)
            });

            // Per-bomb launch sequence matching NukeExecution's order: each silo contributes its slots in turn.
            let mut launch_sequence: Vec<(f64, bool)> = Vec::new();
            for entry in &all_available_silos {
                for _ in 0..entry.slots {
                    launch_sequence.push((entry.flight_ticks, entry.interceptable));
                }
            }

            // Half the SAM cooldown as the maximum total arrival spread, to be safe.
            let sam_cooldown = game.config().sam_cooldown() as f64;
            let max_total_arrival_spread = (sam_cooldown / 2.0).floor();

            // 1 extra bomb for every 5, for SAMs the enemy builds while the bombs fly.
            let extra_bombs = (bombs_needed / 5.0).floor();
            let total_bombs = bombs_needed + extra_bombs;

            // Bombs of silos whose trajectory is not blocked by SAMs other than the covering ones.
            let mut unblocked_bombs: Vec<(usize, f64)> = Vec::new();
            for (i, &(flight_ticks, interceptable)) in launch_sequence.iter().enumerate() {
                if !interceptable {
                    unblocked_bombs.push((i, flight_ticks));
                }
            }

            if (unblocked_bombs.len() as f64) < total_bombs {
                failed_target.get_or_insert(FailedTarget {
                    target_tile,
                    covering_sam_ids: covering_sam_ids.clone(),
                    total_bombs,
                });
                needs_more_silos = true;
                continue;
            }

            // Sort unblocked bombs by flight time to find the window of maxTotalArrivalSpread with the most bombs.
            let mut sorted_by_flight = unblocked_bombs.clone();
            sorted_by_flight.sort_by(|a, b| a.1.total_cmp(&b.1));

            let mut best_window_start = 0;
            let mut best_window_count = 0;
            for start in 0..sorted_by_flight.len() {
                let mut end = start;
                while end < sorted_by_flight.len()
                    && sorted_by_flight[end].1 - sorted_by_flight[start].1
                        <= max_total_arrival_spread
                {
                    end += 1;
                }
                if end - start > best_window_count {
                    best_window_count = end - start;
                    best_window_start = start;
                }
            }

            if (best_window_count as f64) < total_bombs {
                failed_target.get_or_insert(FailedTarget {
                    target_tile,
                    covering_sam_ids: covering_sam_ids.clone(),
                    total_bombs,
                });
                needs_more_silos = true;
                continue;
            }

            // From the window, the totalBombs bombs with the lowest launch-sequence indices (minimises the gold cost).
            let window_bombs =
                &sorted_by_flight[best_window_start..best_window_start + best_window_count];
            let mut window_by_index: Vec<(usize, f64)> = window_bombs.to_vec();
            window_by_index.sort_by_key(|b| b.0);
            let selected: Vec<(usize, f64)> = window_by_index
                .into_iter()
                .take(total_bombs as usize)
                .collect();
            let selected_set: HashSet<usize> = selected.iter().map(|b| b.0).collect();
            let last_selected_index = selected[selected.len() - 1].0;
            let bombs_to_fire = last_selected_index + 1;

            // Per-bomb waitTicks so all selected bombs arrive in the window, spread evenly from the earliest flight time.
            let mut selected_flight_min = f64::INFINITY;
            for b in &selected {
                selected_flight_min = selected_flight_min.min(b.1);
            }
            let stagger_interval = ((max_total_arrival_spread / total_bombs).floor()).max(1.0);
            let mut selected_idx = 0.0;
            let mut wait_ticks_per_bomb: Vec<f64> = Vec::new();
            for (i, launch) in launch_sequence.iter().enumerate().take(bombs_to_fire) {
                if selected_set.contains(&i) {
                    let target_arrival = selected_flight_min + selected_idx * stagger_interval;
                    wait_ticks_per_bomb.push((target_arrival - launch.0).max(0.0));
                    selected_idx += 1.0;
                } else {
                    // Wasted bomb (interceptable or out of the window): launch immediately.
                    wait_ticks_per_bomb.push(0.0);
                }
            }

            // Gold for all fired bombs (including the wasted ones).
            let total_cost = atom_cost * bombs_to_fire as Gold;
            if game.player(self.player).gold() < total_cost {
                continue;
            }

            // Fire the salvo: NukeExecution picks silos in the same Manhattan order as planned.
            for &wait_ticks in wait_ticks_per_bomb.iter().take(bombs_to_fire) {
                self.send_nuke(
                    game,
                    target_tile,
                    UnitType::AtomBomb,
                    nuke_target,
                    wait_ticks,
                );
            }
            return;
        }

        // Couldn't destroy any SAM: upgrade silos only if capacity was the bottleneck
        // (if only gold is lacking, don't waste it on upgrades).
        if game.config().rules().has(Change::NationNukeRework31) {
            if needs_more_silos && let Some(failed) = failed_target {
                self.maybe_upgrade_helpful_silo(game, &failed);
            }
        } else if needs_more_silos {
            self.maybe_upgrade_best_protected_silo(game);
        }
    }

    /// All enemy SAMs whose range covers a given tile.
    fn find_enemy_sams_covering_tile(&self, game: &GameImpl, tile: TileRef) -> Vec<UnitId> {
        let nearby_sams = game.nearby_units(
            tile,
            game.config().max_sam_range(),
            &[UnitType::SAMLauncher],
            None,
            false,
        );

        let mut result = Vec::new();
        for (sam, dist_squared) in nearby_sams {
            let unit = game.unit_any(sam);
            let owner = unit.owner;
            if owner == self.player || game.player(self.player).is_friendly(owner, false) {
                continue;
            }
            let range = game.config().sam_range(unit.level as f64);
            if dist_squared <= range * range {
                result.push(sam);
            }
        }
        result
    }

    /// Before v0.31: upgrades the silo best protected by our own SAMs.
    fn maybe_upgrade_best_protected_silo(&mut self, game: &mut GameImpl) {
        let silos = game.player(self.player).units_of(UnitType::MissileSilo);
        if silos.is_empty() {
            return;
        }
        let our_sams = game.player(self.player).units_of(UnitType::SAMLauncher);
        let mut best_silo: Option<UnitId> = None;
        let mut best_protection = -1.0;
        for &silo in &silos {
            let silo_unit = game.unit_any(silo);
            if !game.player(self.player).can_upgrade_unit(silo) {
                continue;
            }
            let mut protection = 0.0;
            for &sam in &our_sams {
                let sam_unit = game.unit_any(sam);
                let range = game.config().sam_range(sam_unit.level as f64);
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(silo_unit.tile, sam_unit.tile)
                    as f64;
                if dist_squared <= range * range {
                    protection += sam_unit.level as f64;
                }
            }
            if protection > best_protection {
                best_protection = protection;
                best_silo = Some(silo);
            }
        }
        if let Some(silo) = best_silo {
            self.log.queue(
                game,
                self.player,
                Decision::Upgrade {
                    unit: silo,
                    amount: 1,
                },
            );
        }
    }

    /// Upgrades a silo that would have helped the failed overwhelm attempt: its trajectory to the failed
    /// target is not blocked by non-covering enemy SAMs and it is below the upgrade cap; among those,
    /// the one best protected by our own SAMs.
    fn maybe_upgrade_helpful_silo(&mut self, game: &mut GameImpl, failed_target: &FailedTarget) {
        let silos = game.player(self.player).units_of(UnitType::MissileSilo);
        if silos.is_empty() {
            return;
        }

        // Silos with an unblocked trajectory to the failed target (not interceptable by non-covering SAMs).
        let mut unblocked_silos: Vec<UnitId> = Vec::new();
        for &silo in &silos {
            if !self.is_trajectory_interceptable_by_sam(
                game,
                game.unit_any(silo).tile,
                failed_target.target_tile,
                Some(&failed_target.covering_sam_ids),
            ) {
                unblocked_silos.push(silo);
            }
        }
        if unblocked_silos.is_empty() {
            return;
        }

        // Bail out if the target is unreachable even at max silo level.
        let max_achievable_slots = unblocked_silos.len() as f64 * MAX_NATION_SILO_UPGRADE_LEVEL;
        if max_achievable_slots < failed_target.total_bombs {
            return;
        }

        let our_sams = game.player(self.player).units_of(UnitType::SAMLauncher);
        let mut best_silo: Option<UnitId> = None;
        let mut best_protection = -1.0;

        for &silo in &unblocked_silos {
            let silo_unit = game.unit_any(silo);
            if silo_unit.level as f64 >= MAX_NATION_SILO_UPGRADE_LEVEL {
                continue;
            }
            if !game.player(self.player).can_upgrade_unit(silo) {
                continue;
            }

            let mut protection = 0.0;
            for &sam in &our_sams {
                let sam_unit = game.unit_any(sam);
                let range = game.config().sam_range(sam_unit.level as f64);
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(silo_unit.tile, sam_unit.tile)
                    as f64;
                if dist_squared <= range * range {
                    protection += sam_unit.level as f64;
                }
            }

            if protection > best_protection {
                best_protection = protection;
                best_silo = Some(silo);
            }
        }

        if let Some(silo) = best_silo {
            self.log.queue(
                game,
                self.player,
                Decision::Upgrade {
                    unit: silo,
                    amount: 1,
                },
            );
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

    /// `maybeSendNuke()` of v0.35.0 on.
    fn maybe_send_nuke_v2(&mut self, game: &mut GameImpl, silos: Vec<UnitId>) {
        let Some((nuke_target, strike)) = self.choose_nuke_target(game) else {
            return;
        };
        if game.player(nuke_target).player_type() == PlayerType::Bot
            || game.player(self.player).is_on_same_team(nuke_target)
            || !self
                .attack_behavior
                .borrow_mut()
                .should_attack(game, nuke_target)
        {
            return;
        }

        // While saving up for a MIRV, Hard and Impossible only nuke back at players hitting them, with atom bombs.
        let saving = self.is_saving_for_mirv(game);
        if saving && !self.is_hitting_us(game, nuke_target) {
            return;
        }

        let nuke_types = self.nuke_types_to_try(game, saving);
        if nuke_types.is_empty() {
            return;
        }

        let structures = game.player(nuke_target).units_of_types(STRUCTURES);
        let difficulty = Self::difficulty(game);
        let num_random_tiles = if difficulty == Difficulty::Impossible {
            30
        } else {
            10
        };
        let random_tiles =
            rand_territory_tile_array(&mut self.rnd(), game, nuke_target, num_random_tiles);
        let mut candidates: Vec<TileRef> = Vec::new();
        let mut seen: HashSet<TileRef> = HashSet::new();
        for t in random_tiles
            .into_iter()
            .chain(structures.iter().map(|&u| game.unit_any(u).tile))
        {
            if seen.insert(t) {
                candidates.push(t);
            }
        }
        self.remove_old_nuke_events(game);

        for nuke_type in nuke_types {
            let tile = self.find_best_nuke_tile(
                game,
                nuke_type,
                nuke_target,
                &candidates,
                &silos,
                &structures,
                strike,
            );
            if let Some(tile) = tile {
                self.send_nuke(game, tile, nuke_type, nuke_target, 0.0);
                if strike && Self::follows_up_strikes(game) {
                    self.strikes.push(Strike {
                        target: nuke_target,
                        tile,
                        tick: game.ticks(),
                        nuke: None,
                    });
                }
                return;
            }
        }
        if difficulty == Difficulty::Impossible && !saving {
            self.maybe_destroy_enemy_sam(game, nuke_target);
        }
    }

    /// Hydrogen bomb first; on Hard and Impossible, an atom bomb if no spot is worth a hydrogen bomb.
    fn nuke_types_to_try(&self, game: &GameImpl, atom_only: bool) -> Vec<UnitType> {
        let config = game.config();
        let difficulty = config.game_config().difficulty;
        let gold = game.player(self.player).gold();
        let mut types: Vec<UnitType> = Vec::new();
        if !atom_only
            && !config.is_unit_disabled(UnitType::HydrogenBomb)
            && gold >= self.get_perceived_nuke_cost(game, UnitType::HydrogenBomb)
        {
            types.push(UnitType::HydrogenBomb);
        }
        if (types.is_empty()
            || difficulty == Difficulty::Hard
            || difficulty == Difficulty::Impossible)
            && !config.is_unit_disabled(UnitType::AtomBomb)
            && gold >= self.get_perceived_nuke_cost(game, UnitType::AtomBomb)
        {
            types.push(UnitType::AtomBomb);
        }
        types
    }

    #[allow(clippy::too_many_arguments)]
    fn find_best_nuke_tile(
        &self,
        game: &GameImpl,
        nuke_type: UnitType,
        nuke_target: SmallId,
        candidates: &[TileRef],
        silos: &[UnitId],
        structures: &[UnitId],
        strike: bool,
    ) -> Option<TileRef> {
        let difficulty = Self::difficulty(game);
        let range = game.config().nuke_magnitudes(nuke_type).outer;
        let mut scored: Vec<(TileRef, f64)> = Vec::new();

        'outer: for &tile in candidates {
            // Cheap pre-filter, blast_hits_friendly_land() checks the whole blast of the winner.
            let mut bounding_box = bounding_box_tiles(&game.map, tile, range as i32);
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

            // Above Easy, skip spots that are already being nuked or where the blast would be mostly wasted.
            let mut coverage = 0.0;
            if difficulty != Difficulty::Easy {
                if self.is_nuke_already_landing_near(game, tile, nuke_type) {
                    continue;
                }
                coverage = self.enemy_land_blast_coverage(game, tile, nuke_type);
                if coverage < min_blast_coverage(nuke_type) {
                    continue;
                }
            }

            // On Hard and Impossible, avoid trajectories that enemy SAMs can intercept.
            if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
                && self.is_trajectory_interceptable_by_sam(game, spawn_tile, tile, None)
            {
                continue;
            }

            let value = self.nuke_tile_score(game, tile, silos, structures, nuke_type);
            // Impossible only nukes spots with structure value, unless it is after the troops.
            let skip = if difficulty == Difficulty::Impossible && !strike {
                value <= 0.0
            } else {
                value < 0.0
            };
            if skip {
                continue;
            }
            scored.push((tile, value + coverage * FULL_BLAST_COVERAGE_VALUE));
        }
        // Stable sort: on ties the earlier candidate wins.
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let margin = if strike { STRIKE_BORDER_MARGIN } else { 0.0 };
        scored
            .into_iter()
            .find(|&(tile, _)| !self.blast_hits_friendly_land(game, tile, nuke_type, margin))
            .map(|(tile, _)| tile)
    }

    /// Who to nuke, and whether it is a strike: a nuke that softens a player up for our attack.
    fn choose_nuke_target(&mut self, game: &mut GameImpl) -> Option<(SmallId, bool)> {
        let target = self.find_best_nuke_target(game)?;
        let strike = match Self::difficulty(game) {
            Difficulty::Easy => false,
            Difficulty::Medium => self.troops_attacking(game, target) > 0.0,
            Difficulty::Hard | Difficulty::Impossible => self.can_strike(game, target),
        };
        Some((target, strike))
    }

    fn find_best_nuke_target_v2(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        // On Hard and Impossible with only 2 players left, target the only other one.
        let diff = Self::difficulty(game);
        if (diff == Difficulty::Hard || diff == Difficulty::Impossible)
            && game.players().len() == 2
            && let Some(other) = game.players().into_iter().find(|&p| p != self.player)
        {
            return Some(other);
        }

        // Retaliate against incoming attacks (most important).
        let incoming_attack_player = self
            .attack_behavior
            .borrow_mut()
            .find_incoming_attack_player(game);
        if incoming_attack_player.is_some() {
            return incoming_attack_player;
        }

        if let Some(p) = self.find_incoming_nuke_player(game) {
            return Some(p);
        }

        // In Teams, back up teammates in their fights before starting our own.
        if let Some(p) = self.find_teammate_fight_target(game) {
            return Some(p);
        }

        // On Impossible, prioritize nuking the crown if they have more than 50% of the map.
        let game_mode = Self::game_mode(game);
        if diff == Difficulty::Impossible && game_mode == GameMode::FFA {
            let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
            if num_tiles_without_fallout > 0.0 {
                let sorted_by_tiles = Self::sorted_by_tiles(game);
                let crown = sorted_by_tiles[0];
                if crown != self.player && !game.player(self.player).is_friendly(crown, false) {
                    let crown_share =
                        game.player(crown).num_tiles_owned() as f64 / num_tiles_without_fallout;
                    if crown_share > CROWN_NEAR_WIN_SHARE {
                        return Some(crown);
                    }
                }
            }
        }

        // Soften up a neighbour about as strong as us before attacking it.
        if let Some(p) = self.find_strike_target(game) {
            return Some(p);
        }

        // On Impossible, the richest nation hunts very high structure density targets.
        if diff == Difficulty::Impossible
            && self.is_richest_nation(game)
            && self.rnd().chance(2.0)
            && let Some(dense_target) = self.find_high_density_target(game)
        {
            return Some(dense_target);
        }

        // Assist allies: check their targets (as in assistAllies, without emojis).
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if game.player(self.player).relation(ally) < Relation::Friendly {
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    continue;
                }
                if game.player(self.player).is_friendly(target, false) {
                    continue;
                }
                return Some(target);
            }
        }

        // The most hated player; much weaker players are ignored (no nukes needed).
        let my_max_troops = game.player(self.player).max_troops();
        for (other, relation) in game.player(self.player).all_relations_sorted() {
            if relation != Relation::Hostile {
                continue;
            }
            if game.player(self.player).is_friendly(other, false) {
                continue;
            }
            let other_max_troops = game.player(other).max_troops();
            if my_max_troops >= other_max_troops * 2.0 {
                continue;
            }
            return Some(other);
        }

        // In FFAs, nuke the crown if they are far enough ahead.
        if let Some(crown_target) = self.find_ffa_crown_target(game) {
            return Some(crown_target);
        }

        // In Teams, nuke the strongest team.
        self.find_strongest_team_target(game)
    }

    fn find_ffa_crown_target_v2(&self, game: &GameImpl) -> Option<SmallId> {
        let difficulty = Self::difficulty(game);
        if Self::game_mode(game) != GameMode::FFA {
            return None;
        }
        if game.players().len() <= 1 {
            return None;
        }
        let sorted_by_tiles = Self::sorted_by_tiles(game);
        let first_place = sorted_by_tiles[0];

        // Don't target ourselves or allies.
        if first_place == self.player || game.player(self.player).is_friendly(first_place, false) {
            return None;
        }
        let num_tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
        if num_tiles_without_fallout <= 0.0 {
            return None;
        }
        let first_place_share =
            game.player(first_place).num_tiles_owned() as f64 / num_tiles_without_fallout;
        if first_place_share > CROWN_NEAR_WIN_SHARE {
            return Some(first_place);
        }

        // Below that, only a runaway leader that is far enough ahead of us.
        if find_runaway_leader(game) != Some(first_place) {
            return None;
        }
        let mut threshold = match difficulty {
            Difficulty::Easy => return None, // Never notices a runaway leader
            Difficulty::Medium => 0.3,
            Difficulty::Hard => 0.2,
            Difficulty::Impossible => 0.1,
        };
        if !game.player(self.player).shares_border_with(first_place) {
            threshold *= 2.0;
        }
        let my_share =
            game.player(self.player).num_tiles_owned() as f64 / num_tiles_without_fallout;
        if first_place_share - my_share > threshold {
            return Some(first_place);
        }
        None
    }

    /// A player not much weaker than us: Medium one it is attacking, Hard and Impossible the strongest neighbour they can follow up on.
    fn find_strike_target(&self, game: &GameImpl) -> Option<SmallId> {
        let my_troops = game.player(self.player).troops();
        let is_candidate = |other: SmallId| {
            other != 0
                && game.player(other).player_type() != PlayerType::Bot
                && !game.player(self.player).is_friendly(other, false)
                && game.player(other).troops() >= my_troops * STRIKE_MIN_TROOP_RATIO
        };
        match Self::difficulty(game) {
            Difficulty::Easy => None,
            Difficulty::Medium => {
                for &attack in game.player(self.player).outgoing_attacks() {
                    let target = game.attack(attack).target();
                    if is_candidate(target) {
                        return Some(target);
                    }
                }
                None
            }
            Difficulty::Hard | Difficulty::Impossible => {
                let mut best: Option<SmallId> = None;
                for other in game.player(self.player).nearby() {
                    if !is_candidate(other) {
                        continue;
                    }
                    if best.is_some_and(|b| game.player(other).troops() <= game.player(b).troops())
                    {
                        continue;
                    }
                    if self.can_strike(game, other) {
                        best = Some(other);
                    }
                }
                best
            }
        }
    }

    /// Whether the attack right after our nuke could send enough troops into `target` by land.
    fn can_strike(&self, game: &GameImpl, target: SmallId) -> bool {
        game.player(target).player_type() != PlayerType::Bot
            && !game.player(self.player).is_friendly(target, false)
            && self.attack_behavior.borrow().strike_troops(game, target)
                >= game.player(target).troops() * STRIKE_MIN_STACK_RATIO
            && game.player(self.player).shares_border_with(target)
    }

    fn follows_up_strikes(game: &GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible
    }

    /// `followUpStrikes()`, called every tick: attacks a strike's target the moment its bomb, or its MIRV's last warhead, lands.
    pub fn follow_up_strikes(&mut self, game: &mut GameImpl) {
        if !game.config().rules().has(Change::NationsLikeHumans5899) {
            return;
        }
        if Self::follows_up_strikes(game) && game.player(self.player).unit_count(UnitType::MIRV) > 0
        {
            self.track_mirv_strikes(game);
        }
        if self.strikes.is_empty() {
            return;
        }
        let mut pending: Vec<Strike> = Vec::new();
        let count = self.strikes.len();
        for i in 0..count {
            if self.strikes[i].nuke.is_none() {
                let found = self.find_strike_nuke(game, self.strikes[i].tile);
                self.strikes[i].nuke = found;
            }
            let (target, tick, nuke) = (
                self.strikes[i].target,
                self.strikes[i].tick,
                self.strikes[i].nuke,
            );
            let Some(nuke) = nuke else {
                if (game.ticks() as f64) - (tick as f64) <= STRIKE_SPAWN_TICKS {
                    pending.push(Strike {
                        target,
                        tile: self.strikes[i].tile,
                        tick,
                        nuke: None,
                    });
                }
                continue;
            };
            if game.unit_any(nuke).is_active() {
                pending.push(Strike {
                    target,
                    tile: self.strikes[i].tile,
                    tick,
                    nuke: Some(nuke),
                });
                continue;
            }
            if game.unit_any(nuke).unit_type() == UnitType::MIRV {
                // A MIRV deletes itself when it splits into warheads.
                if game.player(self.player).unit_count(UnitType::MIRVWarhead) > 0 {
                    pending.push(Strike {
                        target,
                        tile: self.strikes[i].tile,
                        tick,
                        nuke: Some(nuke),
                    });
                    continue;
                }
            } else if !game.unit_any(nuke).reached_target() {
                // Intercepted bombs leave the target's troops alone.
                continue;
            }
            if game.player(target).is_alive() {
                self.attack_behavior.borrow_mut().strike(game, target);
            }
        }
        self.strikes = pending;
    }

    /// Our MIRVs at bordering players are strikes too: a MIRV leaves next to no troops to defend with.
    fn track_mirv_strikes(&mut self, game: &GameImpl) {
        for mirv in game.player(self.player).units_of(UnitType::MIRV) {
            if self.strikes.iter().any(|s| s.nuke == Some(mirv)) {
                continue;
            }
            let unit = game.unit_any(mirv);
            let (Some(target), Some(tile)) = (unit.target_player(), unit.target_tile()) else {
                continue;
            };
            if target == 0 {
                continue;
            }
            if game.player(self.player).is_friendly(target, false) {
                continue;
            }
            if !game.player(self.player).shares_border_with(target) {
                continue;
            }
            self.strikes.push(Strike {
                target,
                tile,
                tick: game.ticks(),
                nuke: Some(mirv),
            });
        }
    }

    fn find_strike_nuke(&self, game: &GameImpl, tile: TileRef) -> Option<UnitId> {
        game.player(self.player)
            .units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb])
            .into_iter()
            .find(|&nuke| {
                game.unit_any(nuke).target_tile() == Some(tile)
                    && !self.strikes.iter().any(|s| s.nuke == Some(nuke))
            })
    }

    fn troops_attacking(&self, game: &GameImpl, target: SmallId) -> f64 {
        let mut troops = 0.0;
        for &attack in game.player(self.player).outgoing_attacks() {
            if game.attack(attack).target() == target {
                troops += game.attack(attack).troops();
            }
        }
        troops
    }

    fn find_incoming_nuke_player(&self, game: &GameImpl) -> Option<SmallId> {
        for nuke in game.units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb]) {
            let unit = game.unit_any(nuke);
            let sender = unit.owner;
            if game.player(self.player).is_friendly(sender, false) {
                continue;
            }
            if let Some(target_tile) = unit.target_tile()
                && game.owner_id(target_tile) == self.player
            {
                return Some(sender);
            }
        }
        None
    }

    /// Medium and up: the enemy attacking our teammates hardest, else the one they attack hardest.
    fn find_teammate_fight_target(&self, game: &GameImpl) -> Option<SmallId> {
        if Self::game_mode(game) != GameMode::Team || Self::difficulty(game) == Difficulty::Easy {
            return None;
        }
        let teammates: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != self.player && game.player(self.player).is_on_same_team(p))
            .collect();
        let is_enemy = |p: SmallId| {
            p != 0
                && game.player(p).player_type() != PlayerType::Bot
                && !game.player(self.player).is_friendly(p, false)
        };
        let mut best: Option<SmallId> = None;
        let mut best_troops = 0.0;
        for &mate in &teammates {
            for attack in game.player(mate).incoming_attacks() {
                let attacker = game.attack(attack).attacker();
                if is_enemy(attacker) && game.attack(attack).troops() > best_troops {
                    best = Some(attacker);
                    best_troops = game.attack(attack).troops();
                }
            }
        }
        if best.is_some() {
            return best;
        }
        for &mate in &teammates {
            for &attack in game.player(mate).outgoing_attacks() {
                let target = game.attack(attack).target();
                if is_enemy(target) && game.attack(attack).troops() > best_troops {
                    best = Some(target);
                    best_troops = game.attack(attack).troops();
                }
            }
        }
        best
    }

    fn get_perceived_nuke_cost_v2(&self, game: &GameImpl, t: UnitType) -> Gold {
        if self.pays_real_nuke_price(game) {
            return self.cost(game, t);
        }
        // Easy and Medium keep the price up for good, so only while a MIRV is worth saving for.
        if !Self::forgets_nuke_prices(game) && !is_mirv_worth_saving_for(game, self.player) {
            return self.cost(game, t);
        }
        self.inflated_nuke_cost(game, t)
    }

    fn pays_real_nuke_price(&self, game: &GameImpl) -> bool {
        // With only 2 players left, use the actual cost (no point saving for a MIRV).
        if game.players().len() == 2 {
            return true;
        }
        if game.config().is_unit_disabled(UnitType::MIRV) {
            return true;
        }
        // A limited amount in team games, synced with NationStructureBehavior.
        let gold = game.player(self.player).gold();
        if Self::game_mode(game) == GameMode::Team && gold > self.cost(game, UnitType::HydrogenBomb)
        {
            return true;
        }
        // The actual cost if there is already enough gold for both a MIRV and a hydro.
        if gold > self.mirv_save_up_target(game) {
            return true;
        }
        // On Hard and Impossible, ignore the perceived cost when under heavy attack (go all-in on nukes).
        let difficulty = Self::difficulty(game);
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && self.is_under_heavy_attack(game)
    }

    /// Hard and Impossible in FFA: the perceived price falls back to the real one within minutes of the last launch.
    fn forgets_nuke_prices(game: &GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && Self::game_mode(game) != GameMode::Team
    }

    fn inflated_nuke_cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        let real = self.cost(game, t);
        let (perceived, last_launch) = if t == UnitType::AtomBomb {
            (self.atom_bomb_perceived_cost, self.atom_bomb_last_launch)
        } else {
            (
                self.hydrogen_bomb_perceived_cost,
                self.hydrogen_bomb_last_launch,
            )
        };
        if perceived <= real {
            return real;
        }
        if !Self::forgets_nuke_prices(game) {
            return perceived;
        }
        let left: i128 = match last_launch {
            None => 0,
            Some(l) => NUKE_PRICE_MEMORY_TICKS - (game.ticks() as i128 - l as i128),
        };
        if left <= 0 {
            return real;
        }
        (real as i128 + ((perceived - real) as i128 * left) / NUKE_PRICE_MEMORY_TICKS) as Gold
    }

    /// Hard and Impossible in FFA: whether they are holding gold back for a MIRV their income will soon reach.
    fn is_saving_for_mirv(&self, game: &GameImpl) -> bool {
        Self::forgets_nuke_prices(game)
            && !self.pays_real_nuke_price(game)
            && is_mirv_worth_saving_for(game, self.player)
            && is_save_up_within_reach(game, self.player, self.mirv_save_up_target(game))
    }

    fn mirv_save_up_target(&self, game: &GameImpl) -> Gold {
        self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb)
    }

    /// Whether the player is attacking us or has a bomb flying at our land.
    fn is_hitting_us(&self, game: &GameImpl, other: SmallId) -> bool {
        if game
            .player(self.player)
            .incoming_attacks()
            .into_iter()
            .any(|a| game.attack(a).attacker() == other)
        {
            return true;
        }
        game.player(other)
            .units_of_types(&[
                UnitType::AtomBomb,
                UnitType::HydrogenBomb,
                UnitType::MIRV,
                UnitType::MIRVWarhead,
            ])
            .into_iter()
            .any(|nuke| {
                game.unit_any(nuke)
                    .target_tile()
                    .is_some_and(|t| game.owner_id(t) == self.player)
            })
    }

    /// Whether our inner blast would overlap the inner blast of any player's nuke in flight.
    fn is_nuke_already_landing_near(
        &self,
        game: &GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
    ) -> bool {
        let config = game.config();
        let our_inner_radius = config.nuke_magnitudes(nuke_type).inner;
        for nuke in game.units_of_types(&[
            UnitType::AtomBomb,
            UnitType::HydrogenBomb,
            UnitType::MIRVWarhead,
        ]) {
            let unit = game.unit_any(nuke);
            let Some(target_tile) = unit.target_tile() else {
                continue;
            };
            let sum_radius = our_inner_radius + config.nuke_magnitudes(unit.unit_type).inner;
            if game.map.euclidean_dist_squared(tile, target_tile) as f64 <= sum_radius * sum_radius
            {
                return true;
            }
        }
        false
    }

    /// Grid-sampled share of the blast that hits enemy land; inner radius tiles count double.
    fn enemy_land_blast_coverage(
        &self,
        game: &GameImpl,
        center: TileRef,
        nuke_type: UnitType,
    ) -> f64 {
        let magnitude = game.config().nuke_magnitudes(nuke_type);
        let (inner, outer) = (magnitude.inner, magnitude.outer);
        let step = f64::max(1.0, (outer / 10.0).floor());
        let cx = game.x(center) as f64;
        let cy = game.y(center) as f64;
        let mut total = 0.0;
        let mut enemy_land = 0.0;
        let mut dy = -outer;
        while dy <= outer {
            let mut dx = -outer;
            while dx <= outer {
                let dist_squared = dx * dx + dy * dy;
                if dist_squared <= outer * outer {
                    let weight = if dist_squared <= inner * inner {
                        2.0
                    } else {
                        1.0
                    };
                    total += weight;
                    if game.is_valid_coord((cx + dx) as i32, (cy + dy) as i32) {
                        // Water, fallout and unowned land have no player owner.
                        let owner =
                            game.owner_id(game.tile_ref((cx + dx) as i32, (cy + dy) as i32));
                        if owner != 0 && !game.player(self.player).is_friendly(owner, true) {
                            enemy_land += weight;
                        }
                    }
                }
                dx += step;
            }
            dy += step;
        }
        enemy_land / total
    }

    /// Whether any tile in the blast, widened by `margin`, belongs to us, a teammate or an ally (even AFK ones).
    fn blast_hits_friendly_land(
        &self,
        game: &GameImpl,
        center: TileRef,
        nuke_type: UnitType,
        margin: f64,
    ) -> bool {
        let friendly_ids: HashSet<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(self.player).is_friendly(p, true))
            .collect();
        let outer = game.config().nuke_magnitudes(nuke_type).outer + margin;
        let cx = game.x(center) as f64;
        let cy = game.y(center) as f64;
        let mut dy = -outer;
        while dy <= outer {
            let mut dx = -outer;
            while dx <= outer {
                if dx * dx + dy * dy <= outer * outer
                    && game.is_valid_coord((cx + dx) as i32, (cy + dy) as i32)
                {
                    let owner_id = game.owner_id(game.tile_ref((cx + dx) as i32, (cy + dy) as i32));
                    if friendly_ids.contains(&owner_id) {
                        return true;
                    }
                }
                dx += 1.0;
            }
            dy += 1.0;
        }
        false
    }
}
