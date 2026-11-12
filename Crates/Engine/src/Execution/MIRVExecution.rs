// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/MIRVExecution.ts: launches a MIRV and its warheads.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A4): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Not-ported stand-in of `MIRVExecution`.
pub struct MIRVExecution;

impl MIRVExecution {
    /// `new MIRVExecution(player, dst)`.
    #[allow(unused_variables, clippy::too_many_arguments)]
    pub fn new(player: SmallId, dst: TileRef) -> Self {
        panic!("not ported: MIRVExecution")
    }
}

impl Execution for MIRVExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: MIRVExecution")
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: MIRVExecution")
    }
    fn is_active(&self) -> bool {
        panic!("not ported: MIRVExecution")
    }
    fn active_during_spawn_phase(&self) -> bool {
        panic!("not ported: MIRVExecution")
    }
}
