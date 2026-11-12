// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TradeShipExecution.ts: a trade ship.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A1): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::NotPorted;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// Stub of upstream `TradeShipExecution`: panics on construction unless `OFSIM_INERT` is set, then it is inert.
pub struct TradeShipExecution;

impl TradeShipExecution {
    /// Upstream constructor.
    #[allow(clippy::too_many_arguments, unused_variables)]
    pub fn new(orig_owner: SmallId, src_port: UnitId, dst_port: UnitId) -> Self {
        NotPorted::require_inert("TradeShipExecution");
        Self
    }
}

impl Execution for TradeShipExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
