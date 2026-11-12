// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/UpgradeStructureExecution.ts: upgrades a structure by levels.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A3): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// Not-ported stand-in of `UpgradeStructureExecution`.
pub struct UpgradeStructureExecution;

impl UpgradeStructureExecution {
    /// `new UpgradeStructureExecution(player, unitId, amount = 1)`.
    #[allow(unused_variables, clippy::too_many_arguments)]
    pub fn new(player: SmallId, unit_id: UnitId, amount: f64) -> Self {
        panic!("not ported: UpgradeStructureExecution")
    }
}

impl Execution for UpgradeStructureExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: UpgradeStructureExecution")
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: UpgradeStructureExecution")
    }
    fn is_active(&self) -> bool {
        panic!("not ported: UpgradeStructureExecution")
    }
    fn active_during_spawn_phase(&self) -> bool {
        panic!("not ported: UpgradeStructureExecution")
    }
}
