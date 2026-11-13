// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts of v0.28.4: the AI player of a map nation.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use crate::Builds::Rules::Rules;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{bounding_box_tiles, calculate_bounding_box, simple_hash};
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::MIRVExecution::MirvExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::{calculate_territory_center, closest_two_tiles};
use crate::Execution::Utils::AllianceBehaviorV28::AllianceBehaviorV28;
use crate::Execution::Utils::BotBehaviorV28::BotBehaviorV28;
use crate::Execution::Utils::StructureSpawnTileValueV28::structure_spawn_tile_value;
use crate::Game::Game::{
    Cell, Difficulty, Gold, Nation, PlayerType, Relation, SmallId, TerrainType, Tick, UnitId,
    UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{TileRef, eucl_dist_fn};
use std::cell::RefCell;
use std::rc::Rc;

/// Ticks until a MIRV can be attempted again.
const MIRV_COOLDOWN_TICKS: u32 = 20;
/// Odds of aborting a MIRV attempt.
const MIRV_HESITATION_ODDS: f64 = 7.0;
/// Threshold for team victory denial.
const VICTORY_DENIAL_TEAM_THRESHOLD: f64 = 0.8;
/// Threshold for individual victory denial.
const VICTORY_DENIAL_INDIVIDUAL_THRESHOLD: f64 = 0.65;
/// Multiplier for the steamroll city gap threshold.
const STEAMROLL_CITY_GAP_MULTIPLIER: f64 = 1.3;
/// Minimum city count for the leader to trigger steamroll detection.
const STEAMROLL_MIN_LEADER_CITIES: i64 = 10;
/// How long the valid MIRV target list is kept, in ticks.
const MIRV_TARGETS_CACHE_TICKS: u32 = 2 * 10;

/// `canBuild(...)` used as a JS condition: a spawn tile 0 would be falsy.
fn truthy(tile: Option<TileRef>) -> bool {
    tile.is_some_and(|t| t != 0)
}

/// Drives one fake human: spawning, then periodic decisions.
pub struct FakeHumanExecution {
    active: bool,
    random: SharedRandom,
    behavior: Option<BotBehaviorV28>,
    alliance_behavior: Option<AllianceBehaviorV28>,
    player: Option<SmallId>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    last_nuke_sent: Vec<(Tick, TileRef)>,
    last_mirv_sent: Vec<(Tick, TileRef)>,
    embargo_malus_applied: OrderedSet<SmallId>,
    tracked_transport_ships: OrderedSet<UnitId>,
    tracked_trade_ships: OrderedSet<UnitId>,
    mirv_targets_cache: Option<(Tick, Vec<SmallId>)>,
    game_id: String,
    nation: Nation,
    rules: Rules,
}

impl FakeHumanExecution {
    /// `new FakeHumanExecution(gameID, nation)`.
    pub fn new(game_id: &str, nation: Nation, rules: &Rules) -> Self {
        let seed = simple_hash(&nation.player_info.id) as f64 + simple_hash(game_id) as f64;
        let mut random = PseudoRandom::for_rules(rules, seed);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(50.0, 60.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 40.0) / 100.0;
        let expand_ratio = random.next_int(10.0, 20.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            behavior: None,
            alliance_behavior: None,
            player: None,
            attack_rate,
            attack_tick,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            last_nuke_sent: Vec::new(),
            last_mirv_sent: Vec::new(),
            embargo_malus_applied: OrderedSet::new(),
            tracked_transport_ships: OrderedSet::new(),
            tracked_trade_ships: OrderedSet::new(),
            mirv_targets_cache: None,
            game_id: game_id.to_string(),
            nation,
            rules: rules.clone(),
        }
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn next_int(&self, min: f64, max: f64) -> f64 {
        self.random.borrow_mut().next_int(min, max)
    }

    fn me(&self) -> SmallId {
        self.player.expect("not initialized")
    }

    fn cost(&self, game: &GameImpl, unit_type: UnitType) -> Gold {
        game.player(self.me()).unit_cost(unit_type, 0)
    }

    fn update_relations_from_embargos(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        for other in others {
            let embargo_malus = -20.0;
            let has = game.player(other).has_embargo_against(player);
            let applied = self.embargo_malus_applied.contains(&other);
            if has && !applied {
                game.player_mut(player)
                    .update_relation(other, embargo_malus);
                self.embargo_malus_applied.insert(other);
            } else if !has && applied {
                game.player_mut(player)
                    .update_relation(other, -embargo_malus);
                self.embargo_malus_applied.remove(&other);
            }
        }
    }

    fn handle_embargoes_to_hostile_nations(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        for other in others {
            // When hostile, start an embargo and keep it until neutral again
            let relation = game.player(player).relation(other);
            if relation <= Relation::Hostile
                && !game.player(player).has_embargo_against(other)
                && !game.player(player).is_on_same_team(other)
            {
                game.player_mut(player).add_embargo(other, false);
            } else if relation >= Relation::Neutral
                && game.player(player).has_embargo_against(other)
            {
                game.player_mut(player).stop_embargo(other);
            }
        }
    }

    fn random_spawn_land(&self, game: &GameImpl) -> Option<TileRef> {
        let delta = 25.0;
        let cell = self
            .nation
            .spawn_cell
            .expect("a fake human has a spawn cell");
        let mut tries = 0;
        while tries < 50 {
            tries += 1;
            let x = self.next_int(cell.x as f64 - delta, cell.x as f64 + delta);
            let y = self.next_int(cell.y as f64 - delta, cell.y as f64 + delta);
            if !game.is_valid_coord(x as i32, y as i32) {
                continue;
            }
            let tile = game.tile_ref(x as i32, y as i32);
            if game.is_land(tile) && !game.has_owner(tile) {
                if game.map.terrain_type(tile) == TerrainType::Mountain && self.chance(2.0) {
                    continue;
                }
                return Some(tile);
            }
        }
        None
    }

    fn maybe_attack(&mut self, game: &mut GameImpl) {
        let player = self.me();
        let map = &game.map;
        let border: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .flat_map(|t| map.neighbors(t))
            .filter(|&t| map.is_land(t) && map.owner_id(t) != player)
            .collect();
        let mut bordering_players: OrderedSet<SmallId> = OrderedSet::new();
        for &t in &border {
            let owner = map.owner_id(t);
            if owner != 0 {
                bordering_players.insert(owner);
            }
        }
        let mut bordering_players: Vec<SmallId> = bordering_players.iter().copied().collect();
        bordering_players.sort_by(|&a, &b| {
            (game.player(a).troops() - game.player(b).troops())
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let bordering_friends: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| game.player(player).is_friendly(o, false))
            .collect();
        let bordering_enemies: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| !game.player(player).is_friendly(o, false))
            .collect();

        // Attack terra nullius but not nuked territory
        let has_non_nuked_terra_nullius = border
            .iter()
            .any(|&t| !map.has_owner(t) && !map.has_fallout(t));
        let mut behavior = self.behavior.take().expect("not initialized");
        let mut alliance = self.alliance_behavior.take().expect("not initialized");
        self.run_attack(
            game,
            &mut behavior,
            &mut alliance,
            has_non_nuked_terra_nullius,
            &bordering_friends,
            &bordering_enemies,
        );
        self.behavior = Some(behavior);
        self.alliance_behavior = Some(alliance);
    }

    fn run_attack(
        &mut self,
        game: &mut GameImpl,
        behavior: &mut BotBehaviorV28,
        alliance: &mut AllianceBehaviorV28,
        has_non_nuked_terra_nullius: bool,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        if has_non_nuked_terra_nullius {
            behavior.send_attack(game, 0, false);
            return;
        }
        if bordering_enemies.is_empty() {
            if self.chance(5.0) {
                self.send_boat_randomly(game, &[]);
            }
        } else {
            if self.chance(10.0) {
                self.send_boat_randomly(game, bordering_enemies);
                return;
            }
            alliance.maybe_send_alliance_requests(game, bordering_enemies);
        }
        behavior.assist_allies(game);
        behavior.attack_best_target(game, bordering_friends, bordering_enemies);
        let nuke_target = behavior.find_best_nuke_target(game, bordering_enemies);
        self.maybe_send_nuke(game, behavior, nuke_target);
    }

    fn maybe_send_nuke(
        &mut self,
        game: &mut GameImpl,
        behavior: &mut BotBehaviorV28,
        other: Option<SmallId>,
    ) {
        let player = self.me();
        let silos = game.player(player).units_of(UnitType::MissileSilo);
        let Some(other) = other else { return };
        if silos.is_empty()
            || game.player(player).gold() < self.cost(game, UnitType::AtomBomb)
            // Don't nuke bots (as opposed to fake humans and humans)
            || game.player(other).player_type() == PlayerType::Bot
            || game.player(player).is_on_same_team(other)
        {
            return;
        }
        let nuke_type = if game.player(player).gold() > self.cost(game, UnitType::HydrogenBomb) {
            UnitType::HydrogenBomb
        } else {
            UnitType::AtomBomb
        };
        let range: i32 = if nuke_type == UnitType::HydrogenBomb {
            60
        } else {
            15
        };
        let structures = game.player(other).units_of_types(&[
            UnitType::City,
            UnitType::DefensePost,
            UnitType::MissileSilo,
            UnitType::Port,
            UnitType::SAMLauncher,
        ]);
        let structure_tiles: Vec<TileRef> = structures
            .iter()
            .map(|&u| game.unit_any(u).tile())
            .collect();
        let random_tiles = self.rand_territory_tile_array(game, 10);
        let mut all_tiles: OrderedSet<TileRef> = OrderedSet::new();
        for t in random_tiles.into_iter().chain(structure_tiles) {
            all_tiles.insert(t);
        }

        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        self.remove_old_nuke_events(game);
        'outer: for &tile in all_tiles.iter() {
            let mut bounding_box = bounding_box_tiles(&game.map, tile, range);
            // Add radius / 2 in case there is a piece of unwanted territory inside the outer radius
            bounding_box.extend(bounding_box_tiles(&game.map, tile, range / 2));
            for t in bounding_box {
                // Make sure we nuke away from the border
                if game.owner_id(t) != other {
                    continue 'outer;
                }
            }
            if !truthy(game.player(player).can_build(nuke_type, tile, None)) {
                continue;
            }
            let value = self.nuke_tile_score(game, tile, &silos, &structures);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        if let Some(tile) = best_tile {
            self.send_nuke(game, behavior, tile, nuke_type, other);
        }
    }

    fn remove_old_nuke_events(&mut self, game: &GameImpl) {
        let max_age = 500.0;
        let tick = game.ticks() as f64;
        while let Some(&(sent, _)) = self.last_nuke_sent.first() {
            if sent as f64 + max_age < tick {
                self.last_nuke_sent.remove(0);
            } else {
                break;
            }
        }
    }

    fn send_nuke(
        &mut self,
        game: &mut GameImpl,
        behavior: &mut BotBehaviorV28,
        tile: TileRef,
        nuke_type: UnitType,
        target_player: SmallId,
    ) {
        let tick = game.ticks();
        self.last_nuke_sent.push((tick, tile));
        game.add_execution(Box::new(NukeExecution::new(
            nuke_type,
            self.me(),
            tile,
            None,
            -1.0,
            0.0,
            None,
        )));
        behavior.maybe_send_emoji(game, target_player);
    }

    fn nuke_tile_score(
        &self,
        game: &GameImpl,
        tile: TileRef,
        silos: &[UnitId],
        targets: &[UnitId],
    ) -> f64 {
        // Potential damage in a 25-tile radius
        let dist = eucl_dist_fn(tile, 25.0, false);
        let mut tile_value: f64 = targets
            .iter()
            .filter(|&&u| dist(&game.map, game.unit_any(u).tile()))
            .map(|&u| match game.unit_any(u).unit_type() {
                UnitType::City => 25_000.0,
                UnitType::DefensePost => 5_000.0,
                UnitType::MissileSilo => 50_000.0,
                UnitType::Port => 10_000.0,
                _ => 0.0,
            })
            .fold(0.0, |prev, cur| prev + cur);

        // Avoid areas defended by SAM launchers
        let dist50 = eucl_dist_fn(tile, 50.0, false);
        let sams = targets
            .iter()
            .filter(|&&u| {
                game.unit_any(u).unit_type() == UnitType::SAMLauncher
                    && dist50(&game.map, game.unit_any(u).tile())
            })
            .count();
        tile_value -= 50_000.0 * sams as f64;

        // Prefer tiles that are closer to a silo
        let silo_tiles: Vec<TileRef> = silos.iter().map(|&u| game.unit_any(u).tile()).collect();
        let (closest_silo, _) =
            closest_two_tiles(&game.map, silo_tiles, [tile]).expect("Missing result");
        let distance_squared = game.map.euclidean_dist_squared(tile, closest_silo) as f64;
        let distance_to_closest_silo = distance_squared.sqrt();
        tile_value -= distance_to_closest_silo * 30.0;

        // Don't target near recent targets
        let recent = self
            .last_nuke_sent
            .iter()
            .filter(|&&(_, t)| dist(&game.map, t))
            .map(|_| 1_000_000.0)
            .fold(0.0, |prev, cur| prev + cur);
        tile_value -= recent;
        tile_value
    }

    fn handle_units(&mut self, game: &mut GameImpl) -> bool {
        self.maybe_spawn_structure(game, UnitType::City, |n| n)
            || self.maybe_spawn_structure(game, UnitType::Port, |n| n)
            || self.maybe_spawn_warship(game)
            || self.maybe_spawn_structure(game, UnitType::Factory, |n| n)
            || self.maybe_spawn_structure(game, UnitType::DefensePost, |n| (n + 2) * (n + 2))
            || self.maybe_spawn_structure(game, UnitType::SAMLauncher, |n| n * n)
            || self.maybe_spawn_structure(game, UnitType::MissileSilo, |n| n * n)
    }

    fn maybe_spawn_structure(
        &mut self,
        game: &mut GameImpl,
        unit_type: UnitType,
        multiplier: impl Fn(i64) -> i64,
    ) -> bool {
        let player = self.me();
        let owned = game.player(player).units_owned(unit_type);
        let perceived_cost_multiplier = multiplier(owned + 1);
        let real_cost = self.cost(game, unit_type);
        let perceived_cost = real_cost * perceived_cost_multiplier;
        if game.player(player).gold() < perceived_cost {
            return false;
        }
        let Some(tile) = self.structure_spawn_tile(game, unit_type) else {
            return false;
        };
        if game
            .player(player)
            .can_build(unit_type, tile, None)
            .is_none()
        {
            return false;
        }
        game.add_execution(Box::new(ConstructionExecution::new(
            player, unit_type, tile, None, None,
        )));
        true
    }

    fn structure_spawn_tile(&mut self, game: &GameImpl, unit_type: UnitType) -> Option<TileRef> {
        let player = self.me();
        let tiles = if unit_type == UnitType::Port {
            self.rand_coastal_tile_array(game, 25)
        } else {
            self.rand_territory_tile_array(game, 25)
        };
        if tiles.is_empty() {
            return None;
        }
        let value_function = structure_spawn_tile_value(game, player, unit_type);
        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        for t in tiles {
            let v = value_function(game, t);
            if v <= best_value && best_tile.is_some() {
                continue;
            }
            if !truthy(game.player(player).can_build(unit_type, t, None)) {
                continue;
            }
            // Found a better tile
            best_tile = Some(t);
            best_value = v;
        }
        best_tile
    }

    fn rand_coastal_tile_array(&self, game: &GameImpl, num_tiles: usize) -> Vec<TileRef> {
        let tiles: Vec<TileRef> = game
            .player(self.me())
            .border_tiles()
            .iter()
            .filter(|&t| game.map.is_ocean_shore(t))
            .collect();
        self.array_sampler(tiles, num_tiles)
    }

    /// `arraySampler`: all elements, or `sample_size` drawn without replacement.
    fn array_sampler(&self, a: Vec<TileRef>, sample_size: usize) -> Vec<TileRef> {
        if a.len() <= sample_size {
            return a;
        }
        let mut remaining: OrderedSet<TileRef> = OrderedSet::new();
        for t in a {
            remaining.insert(t);
        }
        let mut out = Vec::with_capacity(sample_size);
        for _ in 0..sample_size {
            let t = *self.random.borrow_mut().rand_from_set(&remaining);
            remaining.remove(&t);
            out.push(t);
        }
        out
    }

    fn maybe_spawn_warship(&mut self, game: &mut GameImpl) -> bool {
        let player = self.me();
        if !self.chance(50.0) {
            return false;
        }
        let ports = game.player(player).units_of(UnitType::Port);
        let ships = game.player(player).units_of(UnitType::Warship);
        if !ports.is_empty()
            && ships.is_empty()
            && game.player(player).gold() > self.cost(game, UnitType::Warship)
        {
            let port = *self.random.borrow_mut().rand_element(&ports);
            let port_tile = game.unit_any(port).tile();
            let Some(target_tile) = self.warship_spawn_tile(game, port_tile) else {
                return false;
            };
            if game
                .player(player)
                .can_build(UnitType::Warship, target_tile, None)
                .is_none()
            {
                return false;
            }
            game.add_execution(Box::new(ConstructionExecution::new(
                player,
                UnitType::Warship,
                target_tile,
                None,
                None,
            )));
            return true;
        }
        false
    }

    fn rand_territory_tile_array(&self, game: &GameImpl, num_tiles: usize) -> Vec<TileRef> {
        let player = self.me();
        let bounding_box =
            calculate_bounding_box(&game.map, game.player(player).border_tiles().iter());
        let mut tiles = Vec::new();
        for _ in 0..num_tiles {
            if let Some(tile) = self.rand_territory_tile(game, player, bounding_box) {
                tiles.push(tile);
            }
        }
        tiles
    }

    fn rand_territory_tile(
        &self,
        game: &GameImpl,
        p: SmallId,
        bounding_box: Option<(Cell, Cell)>,
    ) -> Option<TileRef> {
        let (min, max) = bounding_box?;
        for _ in 0..100 {
            let rand_x = self.next_int(min.x as f64, max.x as f64);
            let rand_y = self.next_int(min.y as f64, max.y as f64);
            if !game.is_on_map(Cell::new(rand_x as i32, rand_y as i32)) {
                // Sanity check, should never happen
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if game.owner_id(rand_tile) == p {
                return Some(rand_tile);
            }
        }
        None
    }

    fn warship_spawn_tile(&self, game: &GameImpl, port_tile: TileRef) -> Option<TileRef> {
        let radius = 250.0;
        let (px, py) = (game.x(port_tile) as f64, game.y(port_tile) as f64);
        for _ in 0..50 {
            let rand_x = self.next_int(px - radius, px + radius);
            let rand_y = self.next_int(py - radius, py + radius);
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let tile = game.tile_ref(rand_x as i32, rand_y as i32);
            // Sanity check
            if !game.map.is_ocean(tile) {
                continue;
            }
            return Some(tile);
        }
        None
    }

    fn send_boat_randomly(&mut self, game: &mut GameImpl, bordering_enemies: &[SmallId]) {
        let player = self.me();
        let ocean_shore: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .filter(|&t| game.map.is_ocean_shore(t))
            .collect();
        if ocean_shore.is_empty() {
            return;
        }
        let src = *self.random.borrow_mut().rand_element(&ocean_shore);

        // First look for high-interest targets (unowned or bot-owned), mainly relevant early on
        let mut dst = self.random_boat_target(game, src, bordering_enemies, true);
        if dst.is_none() {
            // None found? Then look for players
            dst = self.random_boat_target(game, src, bordering_enemies, false);
        }
        let Some(dst) = dst else { return };
        let troops = game.player(player).troops() / 5.0;
        let owner = game.owner_id(dst);
        let target_id = (owner != 0).then(|| game.player(owner).id().to_string());
        game.add_execution(Box::new(
            TransportShipExecution::new(player, dst, Some(troops))
                .with_intent_target(target_id, None),
        ));
    }

    fn random_boat_target(
        &self,
        game: &GameImpl,
        tile: TileRef,
        bordering_enemies: &[SmallId],
        high_interest_only: bool,
    ) -> Option<TileRef> {
        let player = self.me();
        let (x, y) = (game.x(tile) as f64, game.y(tile) as f64);
        for _ in 0..500 {
            let rand_x = self.next_int(x - 150.0, x + 150.0);
            let rand_y = self.next_int(y - 150.0, y + 150.0);
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if !game.is_land(rand_tile) {
                continue;
            }
            let owner = game.owner_id(rand_tile);
            if owner == player {
                continue;
            }
            // Don't send boats to players we share a border with, that usually looks stupid
            if owner != 0 && bordering_enemies.contains(&owner) {
                continue;
            }
            // Don't spam boats into players more than twice as large as us
            if owner != 0 && game.player(owner).troops() > game.player(player).troops() * 2.0 {
                continue;
            }
            if high_interest_only {
                // Prioritise unowned tiles or tiles owned by bots
                if owner == 0 || game.player(owner).player_type() == PlayerType::Bot {
                    return Some(rand_tile);
                }
            } else if owner == 0 || !game.player(owner).is_friendly(player, false) {
                // Unowned tiles or tiles of non-friendly players
                return Some(rand_tile);
            }
        }
        None
    }

    // ---- MIRV strategy ------------------------------------------------------------------------

    fn consider_mirv(&mut self, game: &mut GameImpl, behavior: &mut BotBehaviorV28) -> bool {
        let player = self.me();
        if game
            .player(player)
            .units_of(UnitType::MissileSilo)
            .is_empty()
        {
            return false;
        }
        if game.player(player).gold() < self.cost(game, UnitType::MIRV) {
            return false;
        }
        self.remove_old_mirv_events(game);
        if !self.last_mirv_sent.is_empty() {
            return false;
        }
        if self.chance(MIRV_HESITATION_ODDS) {
            self.trigger_mirv_cooldown(game, None);
            return false;
        }
        if let Some(sender) = self.select_counter_mirv_target(game) {
            self.maybe_send_mirv(game, behavior, sender);
            return true;
        }
        if let Some(target) = self.select_victory_denial_target(game) {
            self.maybe_send_mirv(game, behavior, target);
            return true;
        }
        if let Some(target) = self.select_steamroll_stop_target(game) {
            self.maybe_send_mirv(game, behavior, target);
            return true;
        }
        false
    }

    fn select_counter_mirv_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let mut attackers: Vec<SmallId> = self
            .get_valid_mirv_target_players(game)
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

    fn select_victory_denial_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let total_land = game.num_land_tiles();
        if total_land == 0.0 {
            return None;
        }
        let mut best: Option<(SmallId, f64)> = None;
        for p in self.get_valid_mirv_target_players(game) {
            let mut severity = 0.0;
            let team = game.player(p).team().cloned();
            if let Some(team) = team {
                let team_members: Vec<SmallId> = game
                    .players()
                    .into_iter()
                    .filter(|&x| game.player(x).team() == Some(&team))
                    .collect();
                let team_territory: f64 = team_members
                    .iter()
                    .map(|&x| game.player(x).num_tiles_owned() as f64)
                    .fold(0.0, |a, b| a + b);
                let team_share = team_territory / total_land;
                if team_share >= VICTORY_DENIAL_TEAM_THRESHOLD {
                    // Only the largest team member is a target once the team exceeds the threshold
                    let mut largest_member: Option<SmallId> = None;
                    let mut largest_tiles: i64 = -1;
                    for &member in &team_members {
                        let tiles = game.player(member).num_tiles_owned() as i64;
                        if tiles > largest_tiles {
                            largest_tiles = tiles;
                            largest_member = Some(member);
                        }
                    }
                    severity = if largest_member == Some(p) {
                        team_share
                    } else {
                        // Skip non-largest members
                        0.0
                    };
                }
            } else {
                let share = game.player(p).num_tiles_owned() as f64 / total_land;
                if share >= VICTORY_DENIAL_INDIVIDUAL_THRESHOLD {
                    severity = share;
                }
            }
            if severity > 0.0 && best.is_none_or(|(_, s)| severity > s) {
                best = Some((p, severity));
            }
        }
        best.map(|(p, _)| p)
    }

    fn select_steamroll_stop_target(&mut self, game: &GameImpl) -> Option<SmallId> {
        let valid_targets = self.get_valid_mirv_target_players(game);
        if valid_targets.is_empty() {
            return None;
        }
        let mut all_players: Vec<(SmallId, i64)> = game
            .players()
            .into_iter()
            .map(|p| (p, game.player(p).unit_count(UnitType::City)))
            .collect();
        all_players.sort_by_key(|b| std::cmp::Reverse(b.1));
        if all_players.len() < 2 {
            return None;
        }
        let top = all_players[0];
        if top.1 <= STEAMROLL_MIN_LEADER_CITIES {
            return None;
        }
        let second_highest = all_players[1].1;
        let threshold = second_highest as f64 * STEAMROLL_CITY_GAP_MULTIPLIER;
        if top.1 as f64 >= threshold {
            return valid_targets.contains(&top.0).then_some(top.0);
        }
        None
    }

    fn get_valid_mirv_target_players(&mut self, game: &GameImpl) -> Vec<SmallId> {
        let player = self.me();
        if let Some((tick, players)) = &self.mirv_targets_cache
            && game.ticks() - tick < MIRV_TARGETS_CACHE_TICKS
        {
            return players.clone();
        }
        let players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                p != player
                    && game.player(p).player_type() != PlayerType::Bot
                    && !game.player(player).is_on_same_team(p)
            })
            .collect();
        self.mirv_targets_cache = Some((game.ticks(), players.clone()));
        players
    }

    fn is_inbound_mirv_from(&self, game: &GameImpl, attacker: SmallId) -> bool {
        for mirv in game.player(attacker).units_of(UnitType::MIRV) {
            let Some(dst) = game.unit_any(mirv).target_tile() else {
                continue;
            };
            // A tile 0 would be falsy
            if dst == 0 {
                continue;
            }
            if !game.has_owner(dst) {
                continue;
            }
            if game.owner_id(dst) == self.me() {
                return true;
            }
        }
        false
    }

    fn maybe_send_mirv(
        &mut self,
        game: &mut GameImpl,
        behavior: &mut BotBehaviorV28,
        enemy: SmallId,
    ) {
        behavior.maybe_send_emoji(game, enemy);
        let center_tile = calculate_territory_center(game, enemy);
        if let Some(center) = center_tile
            && center != 0
            && truthy(
                game.player(self.me())
                    .can_build(UnitType::MIRV, center, None),
            )
        {
            self.send_mirv(game, center);
        }
    }

    fn send_mirv(&mut self, game: &mut GameImpl, tile: TileRef) {
        self.trigger_mirv_cooldown(game, Some(tile));
        game.add_execution(Box::new(MirvExecution::new(self.me(), tile)));
    }

    fn trigger_mirv_cooldown(&mut self, game: &GameImpl, tile: Option<TileRef>) {
        self.remove_old_mirv_events(game);
        let tick = game.ticks();
        // Any tile of the territory serves for cooldown tracking when none is given
        let cooldown_tile = tile.unwrap_or_else(|| {
            game.player(self.me())
                .tiles()
                .iter()
                .next()
                .unwrap_or_else(|| game.tile_ref(0, 0))
        });
        self.last_mirv_sent.push((tick, cooldown_tile));
    }

    fn remove_old_mirv_events(&mut self, game: &GameImpl) {
        let tick = game.ticks();
        while let Some(&(sent, _)) = self.last_mirv_sent.first() {
            if sent + MIRV_COOLDOWN_TICKS <= tick {
                self.last_mirv_sent.remove(0);
            } else {
                break;
            }
        }
    }

    // ---- ship tracking ------------------------------------------------------------------------

    /// Sends out a warship if our transport ship got destroyed by an enemy.
    fn track_transport_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        for u in game.player(player).units_of(UnitType::TransportShip) {
            self.tracked_transport_ships.insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_transport_ships.iter().copied().collect();
        for ship in tracked {
            if !game.unit_any(ship).is_active() {
                // Arrival and retreat differ from destruction by an enemy
                if game.unit_any(ship).was_destroyed_by_enemy() {
                    let tile = game.unit_any(ship).tile();
                    self.maybe_retaliate_with_warship(game, tile);
                }
                self.tracked_transport_ships.remove(&ship);
            }
        }
    }

    /// Sends out a warship if our trade ship got captured.
    fn track_trade_ships_and_retaliate(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        for u in game.player(player).units_of(UnitType::TradeShip) {
            self.tracked_trade_ships.insert(u);
        }
        let tracked: Vec<UnitId> = self.tracked_trade_ships.iter().copied().collect();
        for ship in tracked {
            if !game.unit_any(ship).is_active() {
                self.tracked_trade_ships.remove(&ship);
                continue;
            }
            if game.unit_any(ship).owner_id() != player {
                // The ship was ours and is now owned by someone else: captured
                let tile = game.unit_any(ship).tile();
                self.maybe_retaliate_with_warship(game, tile);
                self.tracked_trade_ships.remove(&ship);
            }
        }
    }

    fn maybe_retaliate_with_warship(&mut self, game: &mut GameImpl, tile: TileRef) {
        let Some(player) = self.player else { return };
        let difficulty = game.config().game_config().difficulty;
        // Never on Easy; Medium retaliates 15 percent of the time, Hard 50, Impossible 80
        let retaliate = (difficulty == Difficulty::Medium && self.next_int(0.0, 100.0) < 15.0)
            || (difficulty == Difficulty::Hard && self.next_int(0.0, 100.0) < 50.0)
            || (difficulty == Difficulty::Impossible && self.next_int(0.0, 100.0) < 80.0);
        if retaliate {
            if game
                .player(player)
                .can_build(UnitType::Warship, tile, None)
                .is_none()
            {
                return;
            }
            game.add_execution(Box::new(ConstructionExecution::new(
                player,
                UnitType::Warship,
                tile,
                None,
                None,
            )));
        }
    }
}

impl Execution for FakeHumanExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        // Upstream draws once here for a never-used traitor role
        let _ = self.chance(10.0);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        // Ship tracking
        if let Some(player) = self.player
            && game.player(player).is_alive()
            && game.config().game_config().difficulty != Difficulty::Easy
        {
            self.track_transport_ships_and_retaliate(game);
            self.track_trade_ships_and_retaliate(game);
        }

        if ticks as f64 % self.attack_rate != self.attack_tick {
            return;
        }

        if game.in_spawn_phase() {
            let Some(rl) = self.random_spawn_land(game) else {
                return;
            };
            game.add_execution(Box::new(SpawnExecution::new(
                &self.game_id,
                self.nation.player_info.clone(),
                Some(rl),
                false,
                &self.rules,
            )));
            return;
        }

        if self.player.is_none() {
            self.player = game
                .players()
                .into_iter()
                .find(|&p| game.player(p).id() == self.nation.player_info.id);
            if self.player.is_none() {
                return;
            }
        }
        let player = self.me();

        if !game.player(player).is_alive() {
            self.active = false;
            return;
        }

        if self.behavior.is_none() || self.alliance_behavior.is_none() {
            // The player is unavailable during init
            let mut behavior = BotBehaviorV28::new(
                self.random.clone(),
                player,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
            );
            self.alliance_behavior = Some(AllianceBehaviorV28::new(self.random.clone(), player));
            // Send an attack on the first tick
            behavior.force_send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        self.update_relations_from_embargos(game);
        let mut alliance = self.alliance_behavior.take().expect("checked above");
        alliance.handle_alliance_requests(game);
        alliance.handle_alliance_extension_requests(game);
        self.alliance_behavior = Some(alliance);
        self.handle_units(game);
        self.handle_embargoes_to_hostile_nations(game);
        let mut behavior = self.behavior.take().expect("checked above");
        self.consider_mirv(game, &mut behavior);
        self.behavior = Some(behavior);
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
