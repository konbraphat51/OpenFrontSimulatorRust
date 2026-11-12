// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TransportShipExecution.ts: a boat carrying troops to a shore.
//! STUB(owner=A1): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub: panics with `not ported: TransportShipExecution` (inert under `OFSIM_INERT`).
pub struct TransportShipExecution {
    _attacker: SmallId,
    _dst: TileRef,
    _troops: f64,
}

impl TransportShipExecution {
    /// `new TransportShipExecution(attacker, ref, troops)`.
    pub fn new(attacker: SmallId, dst: TileRef, troops: f64) -> Self {
        Self {
            _attacker: attacker,
            _dst: dst,
            _troops: troops,
        }
    }
}

impl Execution for TransportShipExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        if !inert_mode() {
            panic!("not ported: TransportShipExecution");
        }
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        !inert_mode()
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
