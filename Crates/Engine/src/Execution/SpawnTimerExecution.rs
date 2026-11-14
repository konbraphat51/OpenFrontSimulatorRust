// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/SpawnTimerExecution.ts: ends the spawn phase after its turn budget.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::Tick;
use crate::Game::GameImpl::GameImpl;

/// Ends the spawn phase once `ticks > numSpawnPhaseTurns`.
#[derive(Default)]
pub struct SpawnTimerExecution {
    in_spawn_phase: bool,
}

impl SpawnTimerExecution {
    /// Creates the execution.
    pub fn new() -> Self {
        Self {
            in_spawn_phase: true,
        }
    }
}

impl Execution for SpawnTimerExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.in_spawn_phase = game.in_spawn_phase();
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if game.ticks() > game.config().num_spawn_phase_turns() {
            game.end_spawn_phase();
        }
        self.in_spawn_phase = game.in_spawn_phase();
    }

    fn is_active(&self) -> bool {
        // Upstream asks the game; the flag mirrors `inSpawnPhase()` as of the last init/tick,
        // which is the only time the game can change it for this execution.
        self.in_spawn_phase
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
