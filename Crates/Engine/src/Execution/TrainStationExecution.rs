// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TrainStationExecution.ts: creates a structure's station and, for factories, spawns trains.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Execution::TrainExecution::TrainExecution;
use crate::Game::Game::{Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::Railroad::StationId;

const NUM_CARS: u32 = 5;
/// Minimum cooldown between two trains.
const TICKS_COOLDOWN: Tick = 10;

/// Owns the station of one structure; with `spawn_trains` it also launches trains from it.
pub struct TrainStationExecution {
    active: bool,
    random: Option<PseudoRandom>,
    station: Option<StationId>,
    last_spawn_tick: Tick,
    unit: UnitId,
    spawn_trains: bool,
    initialised: bool,
}

impl TrainStationExecution {
    /// `new TrainStationExecution(unit, spawnTrains)`: flags the unit as having a station right away.
    pub fn new(game: &mut GameImpl, unit: UnitId, spawn_trains: bool) -> Self {
        game.unit_mut(unit).set_train_station(true);
        Self {
            active: true,
            random: None,
            station: None,
            last_spawn_tick: 0,
            unit,
            spawn_trains,
            initialised: false,
        }
    }

    fn should_spawn_train(&mut self, game: &GameImpl) -> bool {
        let owner = game.unit_any(self.unit).owner_id();
        let spawn_rate = game.config().train_spawn_rate(
            game.player(owner).unit_count(UnitType::Factory) as f64,
            game.unit_count(UnitType::Train) as f64,
        );
        let random = self.random.as_mut().expect("initialised");
        for _ in 0..game.unit_any(self.unit).level() {
            if random.chance(spawn_rate) {
                return true;
            }
        }
        false
    }

    fn spawn_train(&mut self, game: &mut GameImpl, station: StationId, current_tick: Tick) {
        if !self.spawn_trains {
            return;
        }
        if current_tick < self.last_spawn_tick + TICKS_COOLDOWN {
            return;
        }
        let Some(cluster) = game.rail_network.station_cluster(station) else {
            return;
        };
        let owner = game.unit_any(self.unit).owner_id();
        if !game
            .config()
            .rules()
            .has(crate::Builds::Rules::Change::TrainDestinationReservoir3130)
        {
            let available = game
                .rail_network
                .cluster_available_for_trade(game, cluster, owner);
            if available.is_empty() || !self.should_spawn_train(game) {
                return;
            }
            let random = self.random.as_mut().expect("initialised");
            let destination = *random.rand_from_set(&available);
            if destination != station {
                game.add_execution(Box::new(TrainExecution::new(
                    owner,
                    station,
                    destination,
                    NUM_CARS,
                )));
                self.last_spawn_tick = current_tick;
            }
            return;
        }
        if !game
            .rail_network
            .cluster_has_any_trade_destination(game, cluster, owner)
        {
            return;
        }
        if !self.should_spawn_train(game) {
            return;
        }

        // Pick a destination randomly. Could be improved to pick a lucrative trip
        let random = self.random.as_mut().expect("initialised");
        let Some(destination) = game
            .rail_network
            .cluster_random_trade_destination(game, cluster, owner, random)
        else {
            return;
        };
        if destination == station {
            return;
        }

        game.add_execution(Box::new(TrainExecution::new(
            owner,
            station,
            destination,
            NUM_CARS,
        )));
        self.last_spawn_tick = current_tick;
    }
}

impl Execution for TrainStationExecution {
    fn is_active(&self) -> bool {
        self.active
    }

    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.initialised = true;
        if self.spawn_trains {
            self.random = Some(PseudoRandom::for_rules(
                game.config().rules(),
                game.ticks() as f64,
            ));
        }
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        assert!(self.initialised, "Not initialized");
        if !self.is_active() {
            return;
        }
        let station = match self.station {
            Some(station) => station,
            None => {
                // Can't create new executions on init, so it has to be done in the tick
                let unit = self.unit;
                let station = game.with_rail_network(|rail, game| rail.connect_station(game, unit));
                self.station = Some(station);
                station
            }
        };
        if !game.rail_network.station_is_active(game, station) {
            self.active = false;
            return;
        }
        if self.spawn_trains {
            self.spawn_train(game, station, ticks);
        }
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
