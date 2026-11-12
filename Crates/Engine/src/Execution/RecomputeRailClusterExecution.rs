// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/RecomputeRailClusterExecution.ts: recomputes rail clusters each tick.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::Tick;
use crate::Game::GameImpl::GameImpl;

/// Calls `railNetwork.recomputeClusters()` every tick (a no-op until stations exist).
pub struct RecomputeRailClusterExecution;

impl Execution for RecomputeRailClusterExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        game.rail_network.recompute_clusters();
    }
    fn is_active(&self) -> bool {
        true
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
