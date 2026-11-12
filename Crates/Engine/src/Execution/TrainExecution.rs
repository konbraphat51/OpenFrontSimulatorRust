// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TrainExecution.ts: a train that runs along the rail network and pays out at its stops.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The client motion plan (`recordMotionPlan`) is not ported; it does not touch the simulation.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, TrainType, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Game::Railroad::{OrientedRailroad, StationId};
use crate::Game::TrainStation::on_train_stop;

/// Moves an engine and its cars from `source` to `destination` station, one segment at a time.
pub struct TrainExecution {
    active: bool,
    train: Option<UnitId>,
    /// Cars stored back to front.
    cars: Vec<UnitId>,
    has_cargo: bool,
    current_tile: usize,
    usable_tiles: Vec<TileRef>,
    stations: Vec<StationId>,
    current_railroad: Option<OrientedRailroad>,
    trade_stops_visited: u32,
    path_tiles: Vec<TileRef>,
    path_index: usize,
    player: SmallId,
    source: StationId,
    destination: StationId,
    num_cars: u32,
}

const SPACING: usize = 2;
const SPEED: usize = 2;

/// `getOrientedRailroad(from, to)`.
fn oriented_railroad(game: &GameImpl, from: StationId, to: StationId) -> Option<OrientedRailroad> {
    let rail = &game.rail_network;
    let id = rail.station_railroad_to(from, to)?;
    let railroad = &rail.rails[id as usize];
    // If tiles are stored from -> to, we go forward when railroad.to === to
    Some(OrientedRailroad::new(railroad, railroad.to == to))
}

impl TrainExecution {
    /// `new TrainExecution(railNetwork, player, source, destination, numCars)`.
    pub fn new(player: SmallId, source: StationId, destination: StationId, num_cars: u32) -> Self {
        Self {
            active: true,
            train: None,
            cars: Vec::new(),
            has_cargo: false,
            current_tile: 0,
            usable_tiles: Vec::new(),
            stations: Vec::new(),
            current_railroad: None,
            trade_stops_visited: 0,
            path_tiles: Vec::new(),
            path_index: 0,
            player,
            source,
            destination,
            num_cars,
        }
    }

    /// `owner()`.
    pub fn owner(&self) -> SmallId {
        self.player
    }

    /// `tradeStopsVisited()`.
    pub fn trade_stops_visited(&self) -> u32 {
        self.trade_stops_visited
    }

    fn create_train_units(&mut self, game: &mut GameImpl, tile: TileRef) -> UnitId {
        let destination_unit = game.rail_network.station(self.destination).unit;
        let mut owner = game.player_mut(self.player);
        let train = owner.build_unit(
            UnitType::Train,
            tile,
            UnitParams {
                target_unit: Some(destination_unit),
                train_type: Some(TrainType::Engine),
                ..UnitParams::default()
            },
        );
        // Tail is also an engine, just for cosmetics
        let tail = owner.build_unit(
            UnitType::Train,
            tile,
            UnitParams {
                target_unit: Some(destination_unit),
                train_type: Some(TrainType::TailEngine),
                ..UnitParams::default()
            },
        );
        self.cars.push(tail);
        for _ in 0..self.num_cars {
            let car = owner.build_unit(
                UnitType::Train,
                tile,
                UnitParams {
                    train_type: Some(TrainType::Carriage),
                    loaded: Some(self.has_cargo),
                    ..UnitParams::default()
                },
            );
            self.cars.push(car);
        }
        train
    }

    fn delete_train(&mut self, game: &mut GameImpl) {
        self.active = false;
        if let Some(train) = self.train
            && game.unit_any(train).is_active()
        {
            game.unit_mut(train).delete(Some(false), None);
        }
        for &car in &self.cars {
            if game.unit_any(car).is_active() {
                game.unit_mut(car).delete(Some(false), None);
            }
        }
    }

    fn active_source_or_destination(&self, game: &GameImpl) -> bool {
        let rail = &game.rail_network;
        self.stations.len() > 1
            && rail.station_is_active(game, self.stations[1])
            && rail.station_is_active(game, self.stations[0])
    }

    /// Save the tiles the train goes through so the cars can reuse them.
    fn save_traversed_tiles(&mut self, from: usize, speed: usize) {
        let Some(railroad) = &self.current_railroad else {
            return;
        };
        let mut tile_to_save = from;
        let mut i = 0;
        while i < speed && tile_to_save < railroad.tiles().len() {
            let tile = railroad.tiles()[tile_to_save];
            self.usable_tiles.push(tile);
            if self.usable_tiles.len() > self.cars.len() * SPACING + 3 {
                self.usable_tiles.remove(0);
            }
            tile_to_save += 1;
            i += 1;
        }
    }

    fn update_cars_positions(&mut self, game: &mut GameImpl, new_tile: TileRef) {
        for i in (0..self.cars.len()).rev() {
            let car_tile_index = (i + 1) * SPACING + 2;
            if self.usable_tiles.len() > car_tile_index {
                game.unit_mut(self.cars[i])
                    .move_to(self.usable_tiles[car_tile_index]);
            }
        }
        if let Some(train) = self.train {
            game.unit_mut(train).move_to(new_tile);
        }
    }

    fn next_station(&mut self, game: &GameImpl) -> bool {
        if self.stations.len() > 2 {
            self.path_index += self
                .current_railroad
                .as_ref()
                .map_or(0, |r| r.tiles().len());
            self.stations.remove(0);
            let rail_road = oriented_railroad(game, self.stations[0], self.stations[1])
                .or_else(|| self.resolve_split_railroad(game));
            if let Some(rail_road) = rail_road {
                self.current_railroad = Some(rail_road);
                return true;
            }
        }
        false
    }

    fn resolve_split_railroad(&mut self, game: &GameImpl) -> Option<OrientedRailroad> {
        let (station0, station1) = (self.stations[0], self.stations[1]);
        let path = game
            .rail_network
            .find_stations_path(game, station0, station1);
        if path.len() <= 2 {
            return None;
        }
        let mut cursor = self.path_index;
        for i in 0..path.len() - 1 {
            let segment = oriented_railroad(game, path[i], path[i + 1])?;
            for &tile in segment.tiles() {
                let expected = self.path_tiles.get(cursor).copied();
                cursor += 1;
                if expected != Some(tile) {
                    return None;
                }
            }
        }
        self.stations.splice(0..2, path);
        oriented_railroad(game, self.stations[0], self.stations[1])
    }

    fn can_trade_with_destination(&self, game: &GameImpl) -> bool {
        self.stations.len() > 1
            && game
                .rail_network
                .station_trade_available(game, self.stations[1], self.player)
    }

    fn get_next_tile(&mut self, game: &mut GameImpl) -> Option<TileRef> {
        if self.current_railroad.is_none() || !self.can_trade_with_destination(game) {
            return None;
        }
        self.save_traversed_tiles(self.current_tile, SPEED);
        self.current_tile += SPEED;
        let rail_len = self.current_railroad.as_ref()?.tiles().len();
        if self.current_tile >= rail_len {
            let left_over = self.current_tile - rail_len;
            // Station reached, pick the next station
            self.station_reached(game);
            if !self.next_station(game) {
                return None; // Destination reached (or no valid connection)
            }
            self.current_tile = left_over;
            self.save_traversed_tiles(0, left_over);
        }
        self.current_railroad
            .as_ref()?
            .tiles()
            .get(self.current_tile)
            .copied()
    }

    fn station_reached(&mut self, game: &mut GameImpl) {
        let unit = game.rail_network.station(self.stations[1]).unit;
        on_train_stop(game, unit, self.player, self.trade_stops_visited);
        let station_type = game.unit_any(unit).unit_type();
        if matches!(station_type, UnitType::City | UnitType::Port) {
            self.trade_stops_visited += 1;
        }
    }

    fn target_reached(&self, game: &mut GameImpl) {
        let Some(train) = self.train else {
            return;
        };
        game.unit_mut(train).set_reached_target();
        for &car in &self.cars {
            game.unit_mut(car).set_reached_target();
        }
    }
}

impl Execution for TrainExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let stations = game
            .rail_network
            .find_stations_path(game, self.source, self.destination);
        if stations.len() <= 1 {
            self.active = false;
            return;
        }

        self.stations = stations;
        let Some(railroad) = oriented_railroad(game, self.stations[0], self.stations[1]) else {
            self.active = false;
            return;
        };
        self.current_railroad = Some(railroad);

        let first_tile = game.rail_network.station_tile(game, self.stations[0]);
        let Some(spawn) = game
            .player(self.player)
            .can_build(UnitType::Train, first_tile, None)
        else {
            self.active = false;
            return;
        };
        let train = self.create_train_units(game, spawn);
        self.train = Some(train);

        let mut path_tiles: Vec<TileRef> = Vec::new();
        for i in 0..self.stations.len() - 1 {
            let Some(segment) = oriented_railroad(game, self.stations[i], self.stations[i + 1])
            else {
                self.active = false;
                return;
            };
            path_tiles.extend_from_slice(segment.tiles());
        }
        let start_tile = game.unit_any(train).tile();
        if path_tiles.is_empty() || path_tiles[0] != start_tile {
            path_tiles.insert(0, start_tile);
            self.path_index = 1;
        }
        self.path_tiles = path_tiles;
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let train = self.train.expect("Not initialized");

        if !game.unit_any(train).is_active() || !self.active_source_or_destination(game) {
            self.delete_train(game);
            return;
        }

        // `if (tile)` upstream: a tile ref of 0 is falsy too
        match self.get_next_tile(game).filter(|&t| t != 0) {
            Some(tile) => self.update_cars_positions(game, tile),
            None => {
                self.target_reached(game);
                self.delete_train(game);
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
