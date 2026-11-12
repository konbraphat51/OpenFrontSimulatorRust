// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/SAMLauncherExecution.ts: a sam launcher.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A4): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::NotPorted;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub of upstream `SAMLauncherExecution`: panics on construction unless `OFSIM_INERT` is set, then it is inert.
pub struct SAMLauncherExecution;

impl SAMLauncherExecution {
    /// Upstream constructor.
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn new(player: SmallId, tile: Option<TileRef>, sam: Option<UnitId>) -> Self {
        NotPorted::require_inert("SAMLauncherExecution");
        Self
    }
}

impl Execution for SAMLauncherExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
