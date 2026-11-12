// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts: whom, where and when a nation launches nukes.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::NationBehaviorPorts::{AttackPort, Decision, DecisionLog, EmojiPort};
use super::NationUtils::rand_territory_tile_array;
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
    random: Rc<RefCell<PseudoRandom>>,
    player: SmallId,
    attack_behavior: Rc<RefCell<dyn AttackPort>>,
    emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    recently_sent_nukes: Vec<SentNuke>,
    atom_bombs_launched: u32,
    atom_bomb_perceived_cost: Gold,
    hydrogen_bombs_launched: u32,
    hydrogen_bomb_perceived_cost: Gold,
    /// 1/3 of nations only throw hydrogen bombs (to reduce atom bomb spam).
    is_hydro_nation: bool,
    /// Test hook: records the launches instead of queueing them.
    pub log: DecisionLog,
}

impl NationNukeBehavior {
    /// `new NationNukeBehavior(random, game, player, attackBehavior, emojiBehavior)`; draws the hydro-nation flag.
    pub fn new(
        game: &GameImpl,
        random: Rc<RefCell<PseudoRandom>>,
        player: SmallId,
        attack_behavior: Rc<RefCell<dyn AttackPort>>,
        emoji_behavior: Rc<RefCell<dyn EmojiPort>>,
    ) -> Self {
        let p = game.player(player);
        let atom_bomb_perceived_cost = p.unit_cost(UnitType::AtomBomb, 0);
        let hydrogen_bomb_perceived_cost = p.unit_cost(UnitType::HydrogenBomb, 0);
        let is_hydro_nation = random.borrow_mut().chance(3.0);
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
        if silos.is_empty()
            || config.is_unit_disabled(UnitType::MissileSilo)
            || (config.is_unit_disabled(UnitType::AtomBomb)
                && config.is_unit_disabled(UnitType::HydrogenBomb))
        {
            return;
        }

        let Some(nuke_target) = self.find_best_nuke_target(game) else {
            return;
        };

        // Don't nuke tribes (as opposed to nations and humans).
        if game.player(nuke_target).player_type() == PlayerType::Bot
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

        // Filter out friendly players.
        let valid_targets: Vec<SmallId> = target_team_players
            .iter()
            .copied()
            .filter(|&p| !game.player(self.player).is_friendly(p, false))
            .collect();

        if valid_targets.is_empty() {
            return None;
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
        // With only 2 players left, use the actual cost (no point saving for a MIRV).
        if game.players().len() == 2 {
            return self.cost(game, t);
        }

        if game.config().is_unit_disabled(UnitType::MIRV) {
            return self.cost(game, t);
        }

        // A limited amount in team games, synced with NationStructureBehavior.
        let gold = game.player(self.player).gold();
        if Self::game_mode(game) == GameMode::Team && gold > self.cost(game, UnitType::HydrogenBomb)
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
        if nuke_type == UnitType::AtomBomb {
            self.atom_bombs_launched += 1;
            // +50% each time to simulate saving up for a MIRV (more than hydro, so atom bombs lose appeal late).
            self.atom_bomb_perceived_cost = (self.atom_bomb_perceived_cost * 150) / 100;
        } else if nuke_type == UnitType::HydrogenBomb {
            self.hydrogen_bombs_launched += 1;
            // +25% each time to simulate saving up for a MIRV.
            self.hydrogen_bomb_perceived_cost = (self.hydrogen_bomb_perceived_cost * 125) / 100;
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
        if needs_more_silos && let Some(failed) = failed_target {
            self.maybe_upgrade_helpful_silo(game, &failed);
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
            self.log
                .queue(game, self.player, Decision::Upgrade { unit: silo });
        }
    }

    fn cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        game.player(self.player).unit_cost(t, 0)
    }
}
