// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/BoatRetreatExecution.ts: orders an outgoing transport ship to turn back.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;

/// Marks one of the player's transport ships as retreating.
pub struct BoatRetreatExecution {
    active: bool,
    player: SmallId,
    unit_id: UnitId,
}

impl BoatRetreatExecution {
    /// `new BoatRetreatExecution(player, unitID)`.
    pub fn new(player: SmallId, unit_id: UnitId) -> Self {
        Self {
            active: true,
            player,
            unit_id,
        }
    }
}

impl Execution for BoatRetreatExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let found = game.player(self.player).units().iter().copied().find(|&u| {
            u == self.unit_id && game.unit_any(u).unit_type() == UnitType::TransportShip
        });
        let Some(unit) = found else {
            self.active = false;
            return;
        };
        game.unit_mut(unit).update_transport_ship_state(true);
        self.active = false;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
