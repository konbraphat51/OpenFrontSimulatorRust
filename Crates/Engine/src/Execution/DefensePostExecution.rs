// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DefensePostExecution.ts: a defense post (its ship targeting is disabled upstream).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream never assigns `target` (the targeting block is commented out), so `shoot` and the shell
//! bookkeeping are unreachable and not ported; the execution only tracks the post's lifetime.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// Lives while the post does; does nothing while it is under construction.
pub struct DefensePostExecution {
    active: bool,
    post: UnitId,
}

impl DefensePostExecution {
    /// `new DefensePostExecution(post)`.
    pub fn new(post: UnitId) -> Self {
        Self { active: true, post }
    }
}

impl Execution for DefensePostExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.unit_any(self.post).is_active() {
            self.active = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
