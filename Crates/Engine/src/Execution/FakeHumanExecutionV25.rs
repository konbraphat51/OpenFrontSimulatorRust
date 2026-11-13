// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts of v0.25.20 to v0.25.23: the AI player of a map nation (the v25 branch, before the structure value function and betrayal rework).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use crate::Builds::Rules::Rules;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{calculate_bounding_box, simple_hash};
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::BotBehaviorV25::BotBehaviorV25;
use crate::Execution::Utils::BotBehaviorV26::emoji_heckle;
use crate::Game::Game::{
    Cell, Difficulty, Gold, Nation, PlayerType, Relation, SmallId, TerrainType, Tick, UnitId,
    UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{TileRef, eucl_dist_fn, manhattan_dist_fn};
use std::cell::RefCell;
use std::rc::Rc;

/// `canBuild(...)` used as a JS condition: a spawn tile 0 would be falsy.
fn truthy(tile: Option<TileRef>) -> bool {
    tile.is_some_and(|t| t != 0)
}

/// Drives one fake human: spawning, then periodic decisions.
pub struct FakeHumanExecutionV25 {
    active: bool,
    random: SharedRandom,
    behavior: Option<BotBehaviorV25>,
    player: Option<SmallId>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    last_nuke_sent: Vec<(Tick, TileRef)>,
    last_emoji_sent: OrderedMap<SmallId, Tick>,
    embargo_malus_applied: OrderedSet<SmallId>,
    game_id: String,
    nation: Nation,
    rules: Rules,
}

impl FakeHumanExecutionV25 {
    /// `new FakeHumanExecution(gameID, nation)`.
    pub fn new(game_id: &str, nation: Nation, rules: &Rules) -> Self {
        let seed = simple_hash(&nation.player_info.id) as f64 + simple_hash(game_id) as f64;
        let mut random = PseudoRandom::for_rules(rules, seed);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(60.0, 90.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 60.0) / 100.0;
        let expand_ratio = random.next_int(15.0, 25.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            behavior: None,
            player: None,
            attack_rate,
            attack_tick,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            last_nuke_sent: Vec::new(),
            last_emoji_sent: OrderedMap::new(),
            embargo_malus_applied: OrderedSet::new(),
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
            if relation <= Relation::Hostile && !game.player(player).has_embargo_against(other) {
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
        let enemy_border: Vec<TileRef> = game
            .player(player)
            .border_tiles()
            .iter()
            .flat_map(|t| map.neighbors(t))
            .filter(|&t| map.is_land(t) && map.owner_id(t) != player)
            .collect();

        if enemy_border.is_empty() {
            if self.chance(10.0) {
                self.send_boat_randomly(game);
            }
            return;
        }
        if self.chance(20.0) {
            self.send_boat_randomly(game);
            return;
        }

        let border_players: Vec<SmallId> = enemy_border.iter().map(|&t| map.owner_id(t)).collect();
        let mut behavior = self.behavior.take().expect("not initialized");
        if border_players.contains(&0) {
            behavior.send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        // Duplicates stay: the list is not de-duplicated upstream
        let mut enemies: Vec<SmallId> = border_players;
        enemies.sort_by(|&a, &b| {
            (game.player(a).troops() - game.player(b).troops())
                .partial_cmp(&0.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // 5% chance to send a random alliance request
        if self.chance(20.0) {
            let to_ally = *self.random.borrow_mut().rand_element(&enemies);
            if game.player(player).can_send_alliance_request(to_ally) {
                game.player_mut(player).create_alliance_request(to_ally);
                self.behavior = Some(behavior);
                return;
            }
        }

        // 50-50 attack weakest player vs random player
        let to_attack = if self.chance(2.0) {
            enemies[0]
        } else {
            *self.random.borrow_mut().rand_element(&enemies)
        };
        if self.should_attack(game, to_attack) {
            behavior.send_attack(game, to_attack);
            self.behavior = Some(behavior);
            return;
        }

        behavior.forget_old_enemies(game);
        behavior.assist_allies(game);
        let enemy = behavior.select_enemy(game);
        if let Some(enemy) = enemy {
            self.maybe_send_emoji(game, enemy);
            self.maybe_send_nuke(game, enemy);
            if game.player(player).shares_border_with(enemy) {
                behavior.send_attack(game, enemy);
            } else {
                self.maybe_send_boat_attack(game, enemy);
            }
        }
        self.behavior = Some(behavior);
    }

    fn should_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        let player = self.me();
        if game.player(player).is_on_same_team(other) {
            return false;
        }
        if game.player(player).is_friendly(other, false) {
            if self.should_discourage_attack(game, other) {
                self.chance(200.0)
            } else {
                self.chance(50.0)
            }
        } else if self.should_discourage_attack(game, other) {
            self.chance(4.0)
        } else {
            true
        }
    }

    fn should_discourage_attack(&self, game: &GameImpl, other: SmallId) -> bool {
        if game.player(other).is_traitor() {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible {
            return false;
        }
        // Only discourage attacks on humans who are not traitors on easy or medium difficulty
        game.player(other).player_type() == PlayerType::Human
    }

    fn maybe_send_emoji(&mut self, game: &mut GameImpl, enemy: SmallId) {
        if game.player(enemy).player_type() != PlayerType::Human {
            return;
        }
        let ticks = game.ticks();
        let last_sent = self
            .last_emoji_sent
            .get(&enemy)
            .map_or(-300.0, |&t| t as f64);
        if ticks as f64 - last_sent <= 300.0 {
            return;
        }
        self.last_emoji_sent.insert(enemy, ticks);
        let emoji = *self.random.borrow_mut().rand_element(&emoji_heckle());
        let id = game.player(enemy).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.me(), id, emoji)));
    }

    fn maybe_send_boat_attack(&mut self, game: &mut GameImpl, other: SmallId) {
        let player = self.me();
        if game.player(player).is_on_same_team(other) {
            return;
        }
        let map = &game.map;
        let ours: Vec<u32> = game
            .player(player)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let theirs: Vec<u32> = game
            .player(other)
            .border_tiles()
            .iter()
            .filter(|&t| map.is_ocean_shore(t))
            .collect();
        let Some((_, landing)) = closest_two_tiles(map, ours, theirs) else {
            return;
        };
        let troops = game.player(player).troops() / 5.0;
        let other_id = game.player(other).id().to_string();
        game.add_execution(Box::new(
            TransportShipExecution::new(player, landing, Some(troops))
                .with_intent_target(Some(other_id), None),
        ));
    }

    fn maybe_send_nuke(&mut self, game: &mut GameImpl, other: SmallId) {
        let player = self.me();
        let silos = game.player(player).units_of(UnitType::MissileSilo);
        if silos.is_empty()
            || game.player(player).gold() < self.cost(game, UnitType::AtomBomb)
            // Don't nuke bots (as opposed to fake humans and humans)
            || game.player(other).player_type() == PlayerType::Bot
            || game.player(player).is_on_same_team(other)
        {
            return;
        }
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
        let bounding_box =
            calculate_bounding_box(&game.map, game.player(other).border_tiles().iter());
        let mut random_tiles: Vec<Option<TileRef>> = Vec::with_capacity(10);
        for _ in 0..10 {
            random_tiles.push(self.rand_territory_tile(game, other, bounding_box));
        }
        let mut all_tiles: OrderedSet<TileRef> = OrderedSet::new();
        for t in random_tiles.into_iter().flatten().chain(structure_tiles) {
            all_tiles.insert(t);
        }

        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        self.remove_old_nuke_events(game);
        'outer: for &tile in all_tiles.iter() {
            let diamond = manhattan_dist_fn(tile, 15.0, false);
            let near = game.map.bfs(tile, |g, t| diamond(g, t));
            for &t in near.iter() {
                // Make sure we nuke at least 15 tiles in border
                if game.owner_id(t) != other {
                    continue 'outer;
                }
            }
            if !truthy(
                game.player(player)
                    .can_build(UnitType::AtomBomb, tile, None),
            ) {
                continue;
            }
            let value = self.nuke_tile_score(game, tile, &silos, &structures);
            if value > best_value {
                best_tile = Some(tile);
                best_value = value;
            }
        }
        if let Some(tile) = best_tile {
            self.send_nuke(game, tile);
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

    fn send_nuke(&mut self, game: &mut GameImpl, tile: TileRef) {
        let tick = game.ticks();
        self.last_nuke_sent.push((tick, tile));
        game.add_execution(Box::new(NukeExecution::new(
            UnitType::AtomBomb,
            self.me(),
            tile,
            None,
            -1.0,
            0.0,
            None,
        )));
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
        self.maybe_spawn_structure(game, UnitType::Port, 1)
            || self.maybe_spawn_structure(game, UnitType::City, 2)
            || self.maybe_spawn_warship(game)
            || self.maybe_spawn_structure(game, UnitType::Factory, 1)
            || self.maybe_spawn_structure(game, UnitType::MissileSilo, 1)
    }

    fn maybe_spawn_structure(
        &mut self,
        game: &mut GameImpl,
        unit_type: UnitType,
        max_num: i64,
    ) -> bool {
        let player = self.me();
        if game.player(player).units_owned(unit_type) >= max_num {
            return false;
        }
        if game.player(player).gold() < self.cost(game, unit_type) {
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
        let tiles: Vec<TileRef> = if unit_type == UnitType::Port {
            game.player(player)
                .border_tiles()
                .iter()
                .filter(|&t| game.map.is_ocean_shore(t))
                .collect()
        } else {
            game.player(player).tiles().iter().collect()
        };
        if tiles.is_empty() {
            return None;
        }
        Some(*self.random.borrow_mut().rand_element(&tiles))
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

    fn send_boat_randomly(&mut self, game: &mut GameImpl) {
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

        let Some(dst) = self.random_boat_target(game, src, 150.0) else {
            return;
        };
        let troops = game.player(player).troops() / 5.0;
        let owner = game.owner_id(dst);
        let target_id = (owner != 0).then(|| game.player(owner).id().to_string());
        game.add_execution(Box::new(
            TransportShipExecution::new(player, dst, Some(troops))
                .with_intent_target(target_id, None),
        ));
    }

    fn random_boat_target(&self, game: &GameImpl, tile: TileRef, dist: f64) -> Option<TileRef> {
        let player = self.me();
        let (x, y) = (game.x(tile) as f64, game.y(tile) as f64);
        for _ in 0..500 {
            let rand_x = self.next_int(x - dist, x + dist);
            let rand_y = self.next_int(y - dist, y + dist);
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if !game.is_land(rand_tile) {
                continue;
            }
            let owner = game.owner_id(rand_tile);
            if owner == 0 || !game.player(owner).is_friendly(player, false) {
                return Some(rand_tile);
            }
        }
        None
    }
}

impl Execution for FakeHumanExecutionV25 {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        // Upstream draws once here for a never-used traitor role
        let _ = self.chance(10.0);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
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

        if self.behavior.is_none() {
            // The player is unavailable during init
            let mut behavior = BotBehaviorV25::new(
                self.random.clone(),
                player,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
            );
            // Send an attack on the first tick
            behavior.send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }

        self.update_relations_from_embargos(game);
        let mut behavior = self.behavior.take().expect("checked above");
        behavior.handle_alliance_requests(game);
        behavior.handle_alliance_extension_requests(game);
        self.behavior = Some(behavior);
        self.handle_units(game);
        self.handle_embargoes_to_hostile_nations(game);
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
