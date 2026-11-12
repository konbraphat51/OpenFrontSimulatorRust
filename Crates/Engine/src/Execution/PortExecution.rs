// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/PortExecution.ts: a port that joins the rail network and spawns trade ships.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Execution::TradeShipExecution::TradeShipExecution;
use crate::Execution::TrainStationExecution::TrainStationExecution;
use crate::Game::Game::{Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;

/// Spawns trade ships from a finished port every ten ticks (offset per port).
pub struct PortExecution {
    active: bool,
    port: UnitId,
    random: Option<PseudoRandom>,
    check_offset: u32,
    trade_ship_spawn_rejections: f64,
}

impl PortExecution {
    /// `new PortExecution(port)`.
    pub fn new(port: UnitId) -> Self {
        Self {
            active: true,
            port,
            random: None,
            check_offset: 0,
            trade_ship_spawn_rejections: 0.0,
        }
    }

    fn should_spawn_trade_ship(&mut self, game: &GameImpl) -> bool {
        let num_trade_ships = game.unit_count(UnitType::TradeShip);
        let random = self.random.as_mut().expect("initialised");
        // Before PR 5302 the rate was computed once for all port levels.
        let per_level = game
            .config()
            .rules()
            .has(crate::Builds::Rules::Change::TradeTrainSaturation5302);
        let mut spawn_rate = game
            .config()
            .trade_ship_spawn_rate(self.trade_ship_spawn_rejections, num_trade_ships as f64);
        for _ in 0..game.unit_any(self.port).level() {
            if per_level {
                spawn_rate = game.config().trade_ship_spawn_rate(
                    self.trade_ship_spawn_rejections,
                    num_trade_ships as f64,
                );
            }
            if random.chance(spawn_rate) {
                self.trade_ship_spawn_rejections = 0.0;
                return true;
            }
            self.trade_ship_spawn_rejections += 1.0;
        }
        false
    }

    fn create_station(&self, game: &mut GameImpl) {
        let tile = game.unit_any(self.port).tile();
        let nearby_factory = game.has_unit_nearby(
            tile,
            game.config().train_station_max_range(),
            UnitType::Factory,
            None,
            false,
        );
        if nearby_factory {
            let exec = TrainStationExecution::new(game, self.port, false);
            game.add_execution(Box::new(exec));
        }
    }

    /// It's a probability list, so if an element appears twice it's because it's twice more likely to be picked later.
    fn trading_ports(&self, game: &GameImpl) -> Vec<UnitId> {
        let port_tile = game.unit_any(self.port).tile();
        let owner = game.unit_any(self.port).owner_id();

        let mut source_components: Vec<u32> = Vec::new();
        for neighbor in game.map.neighbors(port_tile) {
            if !game.map.is_water(neighbor) {
                continue;
            }
            if let Some(comp) = game.water_manager.get_water_component(&game.map, neighbor)
                && !source_components.contains(&comp)
            {
                source_components.push(comp);
            }
        }

        let mut ports: Vec<UnitId> = Vec::new();
        for p in game.players() {
            if p == owner || !game.player(p).can_trade(owner) {
                continue;
            }
            for port in game.player(p).units_of(UnitType::Port) {
                let tile = game.unit_any(port).tile();
                if source_components.iter().any(|&comp| {
                    game.water_manager
                        .has_water_component(&game.map, tile, comp)
                }) {
                    ports.push(port);
                }
            }
        }
        ports.sort_by(|&p1, &p2| {
            let d1 = game.map.manhattan_dist(port_tile, game.unit_any(p1).tile());
            let d2 = game.map.manhattan_dist(port_tile, game.unit_any(p2).tile());
            d1.cmp(&d2)
        });

        let mut weighted_ports: Vec<UnitId> = Vec::new();
        let short_range_debuff = game.config().trade_ship_short_range_debuff();
        let close_bonus_ports = game.config().proximity_bonus_ports_nb(ports.len() as f64);
        for (i, &other_port) in ports.iter().enumerate() {
            let other = game.unit_any(other_port);
            let expanded = other.level() as usize;
            let other_owner = other.owner_id();
            let dist = game.map.manhattan_dist(port_tile, other.tile()) as f64;
            weighted_ports.extend(std::iter::repeat_n(other_port, expanded));
            let too_close = dist < short_range_debuff;
            let close_bonus = (i as f64) < close_bonus_ports;
            if !too_close && close_bonus {
                // If the port is close, but not too close, add it again to increase the chances of trading with it.
                weighted_ports.extend(std::iter::repeat_n(other_port, expanded));
            }
            if !too_close && game.player(owner).is_friendly(other_owner, false) {
                weighted_ports.extend(std::iter::repeat_n(other_port, expanded));
            }
        }
        weighted_ports
    }
}

impl Execution for PortExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.random = Some(PseudoRandom::new(game.ticks() as f64));
        self.check_offset = game.ticks() % 10;
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.unit_any(self.port).is_active() {
            self.active = false;
            return;
        }

        if game.unit_any(self.port).is_under_construction() {
            return;
        }

        if !game.unit_any(self.port).has_train_station() {
            self.create_station(game);
        }

        // Only check every 10 ticks for performance.
        if !(game.ticks() + self.check_offset).is_multiple_of(10) {
            return;
        }

        if !self.should_spawn_trade_ship(game) {
            return;
        }

        let ports = self.trading_ports(game);

        if ports.is_empty() {
            return;
        }

        let port = *self
            .random
            .as_mut()
            .expect("initialised")
            .rand_element(&ports);
        let owner = game.unit_any(self.port).owner_id();
        game.add_execution(Box::new(TradeShipExecution::new(owner, self.port, port)));
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
