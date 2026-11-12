// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/NukeExecution.ts: a nuke flight.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A4): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::NotPorted;
use crate::Game::Game::{SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub of upstream `NukeExecution`: panics on construction unless `OFSIM_INERT` is set, then it is inert.
pub struct NukeExecution;

impl NukeExecution {
    /// Upstream constructor.
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn new(
        nuke_type: UnitType,
        player: SmallId,
        dst: TileRef,
        src: Option<TileRef>,
        speed: f64,
        wait_ticks: f64,
        rocket_direction_up: Option<bool>,
    ) -> Self {
        NotPorted::require_inert("NukeExecution");
        Self
    }
}

impl Execution for NukeExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
