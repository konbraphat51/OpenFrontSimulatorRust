// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/AttackExecution.ts: a land attack.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A2): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub of the land attack execution.
pub struct AttackExecution;

impl AttackExecution {
    /// `new AttackExecution(startTroops, owner, targetID, sourceTile, removeTroops)`; `target_id` is `None` for terra nullius.
    pub fn new(
        _start_troops: Option<f64>,
        _owner: SmallId,
        _target_id: Option<SmallId>,
        _source_tile: Option<TileRef>,
        _remove_troops: bool,
    ) -> Self {
        if std::env::var_os("OFSIM_INERT").is_none() {
            panic!("not ported: AttackExecution");
        }
        Self
    }
}

impl Execution for AttackExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
