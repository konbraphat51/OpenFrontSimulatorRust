// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/WarshipExecution.ts: a warship.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A1): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::NotPorted;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub of upstream `WarshipExecution`: panics on construction unless `OFSIM_INERT` is set, then it is inert.
pub struct WarshipExecution;

impl WarshipExecution {
    /// Upstream constructor.
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn new(owner: SmallId, patrol_tile: TileRef) -> Self {
        NotPorted::require_inert("WarshipExecution");
        Self
    }
}

impl Execution for WarshipExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
