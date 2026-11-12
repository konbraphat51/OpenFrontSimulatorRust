// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ShellExecution.ts: a warship shell flying at a target unit.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A3): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Stub of the shell execution.
pub struct ShellExecution;

impl ShellExecution {
    /// `new ShellExecution(spawn, owner, ownerUnit, target)`.
    pub fn new(_spawn: TileRef, _owner: SmallId, _owner_unit: UnitId, _target: UnitId) -> Self {
        if std::env::var_os("OFSIM_INERT").is_none() {
            panic!("not ported: ShellExecution");
        }
        Self
    }
}

impl Execution for ShellExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
