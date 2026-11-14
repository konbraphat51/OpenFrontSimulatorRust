// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FakeHumanExecution.ts: the helpers every version of the fake human (nation) AI shares unchanged.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `FakeHumanExecutionV25` to `V28` keep their own decision logic; the code that never changed between them lives here as
//! default methods over a few accessors. A version only uses the helpers its upstream build has.

use crate::Compat::OrderedMap::OrderedMap;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::Util::calculate_bounding_box;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::EmojiIds::emoji_heckle;
use crate::Execution::Utils::StructureSpawnTileValueV28::structure_spawn_tile_value;
use crate::Game::Game::{
    Cell, Gold, Nation, PlayerType, Relation, SmallId, TerrainType, Tick, UnitId, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::{TileRef, eucl_dist_fn};

/// `canBuild(...)` used as a JS condition: a spawn tile 0 would be falsy.
pub fn truthy(tile: Option<TileRef>) -> bool {
    tile.is_some_and(|t| t != 0)
}

/// State accessors and shared helpers of a fake human execution.
pub trait FakeHumanShared {
    /// The shared random stream of this fake human.
    fn random(&self) -> &SharedRandom;
    /// The controlled player once initialised.
    fn player(&self) -> Option<SmallId>;
    /// The nation this fake human plays.
    fn nation(&self) -> &Nation;
    /// Recent nuke targets with their tick.
    fn last_nuke_sent(&self) -> &[(Tick, TileRef)];
    /// Mutable access to the recent nuke targets.
    fn last_nuke_sent_mut(&mut self) -> &mut Vec<(Tick, TileRef)>;
    /// Players whose embargo relation malus is applied.
    fn embargo_malus_applied_mut(&mut self) -> &mut OrderedSet<SmallId>;

    fn chance(&self, odds: f64) -> bool {
        self.random().borrow_mut().chance(odds)
    }

    fn next_int(&self, min: f64, max: f64) -> f64 {
        self.random().borrow_mut().next_int(min, max)
    }

    fn me(&self) -> SmallId {
        self.player().expect("not initialized")
    }

    fn cost(&self, game: &GameImpl, unit_type: UnitType) -> Gold {
        game.player(self.me()).unit_cost(unit_type, 0)
    }

    fn update_relations_from_embargos(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player() else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        for other in others {
            let embargo_malus = -20.0;
            let has = game.player(other).has_embargo_against(player);
            let applied = self.embargo_malus_applied_mut().contains(&other);
            if has && !applied {
                game.player_mut(player)
                    .update_relation(other, embargo_malus);
                self.embargo_malus_applied_mut().insert(other);
            } else if !has && applied {
                game.player_mut(player)
                    .update_relation(other, -embargo_malus);
                self.embargo_malus_applied_mut().remove(&other);
            }
        }
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
            let port = *self.random().borrow_mut().rand_element(&ports);
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
            .last_nuke_sent()
            .iter()
            .filter(|&&(_, t)| dist(&game.map, t))
            .map(|_| 1_000_000.0)
            .fold(0.0, |prev, cur| prev + cur);
        tile_value -= recent;
        tile_value
    }

    fn remove_old_nuke_events(&mut self, game: &GameImpl) {
        let max_age = 500.0;
        let tick = game.ticks() as f64;
        while let Some(&(sent, _)) = self.last_nuke_sent().first() {
            if sent as f64 + max_age < tick {
                self.last_nuke_sent_mut().remove(0);
            } else {
                break;
            }
        }
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
            let t = *self.random().borrow_mut().rand_from_set(&remaining);
            remaining.remove(&t);
            out.push(t);
        }
        out
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

    fn random_spawn_land(&self, game: &GameImpl) -> Option<TileRef> {
        let delta = 25.0;
        let cell = self
            .nation()
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

    fn handle_embargoes_to_hostile_nations(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player() else { return };
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
}

/// Emoji bookkeeping of the fake humans that answer a target with an emoji (v0.25 to v0.27).
pub trait FakeHumanEmoji: FakeHumanShared {
    /// Tick of the last emoji sent to each player.
    fn last_emoji_sent_mut(&mut self) -> &mut OrderedMap<SmallId, Tick>;

    fn maybe_send_emoji(&mut self, game: &mut GameImpl, enemy: SmallId) {
        if game.player(enemy).player_type() != PlayerType::Human {
            return;
        }
        let ticks = game.ticks();
        let last_sent = self
            .last_emoji_sent_mut()
            .get(&enemy)
            .map_or(-300.0, |&t| t as f64);
        if ticks as f64 - last_sent <= 300.0 {
            return;
        }
        self.last_emoji_sent_mut().insert(enemy, ticks);
        let emoji = *self.random().borrow_mut().rand_element(&emoji_heckle());
        let id = game.player(enemy).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.me(), id, emoji)));
    }
}

/// Structure building of the fake humans of v0.26 to v0.28 (value-ranked tiles, perceived cost).
pub trait FakeHumanStructures: FakeHumanShared {
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
}

/// Random boat attacks of the fake humans of v0.25 to v0.26.
pub trait FakeHumanBoats: FakeHumanShared {
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
        let src = *self.random().borrow_mut().rand_element(&ocean_shore);

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
}
