// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/NoOpExecution.ts: an inert execution.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::Tick;
use crate::Game::GameImpl::GameImpl;

/// Does nothing and is never active.
pub struct NoOpExecution;

impl Execution for NoOpExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
