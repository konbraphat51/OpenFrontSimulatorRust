// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/AllianceExtensionExecution.ts: alliance extension execution.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A2): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Alliance extension execution (stub).
pub struct AllianceExtensionExecution;

impl AllianceExtensionExecution {
    /// `new AllianceExtensionExecution(from, to_idID)`; the player is a small id, the other a player id string.
    pub fn new(_from: SmallId, _to_id: String) -> Self {
        if !inert_mode() {
            panic!("not ported: AllianceExtensionExecution");
        }
        Self
    }
}

impl Execution for AllianceExtensionExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
