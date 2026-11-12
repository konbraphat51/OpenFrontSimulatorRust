// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/MissileSiloExecution.ts: reloads a silo's missiles after the cooldown.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// Reloads the earliest fired missile of a silo once its cooldown is over.
pub struct MissileSiloExecution {
    active: bool,
    silo: UnitId,
}

impl MissileSiloExecution {
    /// `new MissileSiloExecution(silo)`.
    pub fn new(silo: UnitId) -> Self {
        Self { active: true, silo }
    }
}

impl Execution for MissileSiloExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let silo = game.unit_any(self.silo);
        if silo.is_under_construction() {
            return;
        }

        if !silo.is_active() && game.has(Change::SiloInactiveGuard3997) {
            self.active = false;
            return;
        }

        // frontTime is the time the earliest missile fired.
        let Some(&front_time) = silo.missile_timer_queue().first() else {
            return;
        };

        let cooldown = game.config().silo_cooldown() as f64 - (game.ticks() as f64 - front_time);

        if cooldown <= 0.0 {
            game.unit_mut(self.silo).reload_missile();
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
