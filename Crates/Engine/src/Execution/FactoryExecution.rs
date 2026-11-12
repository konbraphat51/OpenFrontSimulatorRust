// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/FactoryExecution.ts: a factory that becomes a station and pulls nearby structures into the rail network.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Execution::TrainStationExecution::TrainStationExecution;
use crate::Game::Game::{Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;

/// Creates the factory's own train-spawning station once and lives while the factory does.
pub struct FactoryExecution {
    active: bool,
    station_created: bool,
    factory: UnitId,
}

impl FactoryExecution {
    /// `new FactoryExecution(factory)`.
    pub fn new(factory: UnitId) -> Self {
        Self {
            active: true,
            station_created: false,
            factory,
        }
    }

    fn create_station(&self, game: &mut GameImpl) {
        let tile = game.unit_any(self.factory).tile();
        let structures = game.nearby_units(
            tile,
            game.config().train_station_max_range(),
            &[UnitType::City, UnitType::Port, UnitType::Factory],
            None,
            false,
        );

        let exec = TrainStationExecution::new(game, self.factory, true);
        game.add_execution(Box::new(exec));
        for (unit, _) in structures {
            if !game.unit_any(unit).has_train_station() {
                let exec = TrainStationExecution::new(game, unit, false);
                game.add_execution(Box::new(exec));
            }
        }
    }
}

impl Execution for FactoryExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !self.station_created {
            self.create_station(game);
            self.station_created = true;
        }
        if !game.unit_any(self.factory).is_active() {
            self.active = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
