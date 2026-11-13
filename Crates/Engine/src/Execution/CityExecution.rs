// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/CityExecution.ts: a city that joins the rail network when a factory is near.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Execution::TrainStationExecution::TrainStationExecution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Makes a one-time station check for a finished city and lives while the city does.
pub struct CityExecution {
    active: bool,
    station_created: bool,
    city: UnitId,
    /// Owner and tile of a structure that is built on the first tick (before PR 2378).
    pending: Option<(SmallId, TileRef)>,
}

impl CityExecution {
    /// `new CityExecution(city)`.
    pub fn new(city: UnitId) -> Self {
        Self {
            active: true,
            station_created: false,
            city,
            pending: None,
        }
    }

    /// `new CityExecution(player, tile)`: the structure is built on the first tick.
    pub fn from_tile(player: SmallId, tile: TileRef) -> Self {
        let mut e = Self::new(0);
        e.pending = Some((player, tile));
        e
    }

    fn create_station(&self, game: &mut GameImpl) {
        let tile = game.unit_any(self.city).tile();
        let nearby_factory = game.has_unit_nearby(
            tile,
            game.config().train_station_max_range(),
            UnitType::Factory,
            None,
            false,
        );
        if nearby_factory {
            let exec = TrainStationExecution::new(game, self.city, false);
            game.add_execution(Box::new(exec));
        }
    }
}

impl Execution for CityExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if let Some((player, tile)) = self.pending.take() {
            let Some(spawn) = game.player(player).can_build(UnitType::City, tile, None) else {
                self.active = false;
                return;
            };
            self.city =
                game.player_mut(player)
                    .build_unit(UnitType::City, spawn, UnitParams::default());
            self.create_station(game);
            self.station_created = true;
        }
        if !self.station_created {
            self.create_station(game);
            self.station_created = true;
        }
        if !game.unit_any(self.city).is_active() {
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
