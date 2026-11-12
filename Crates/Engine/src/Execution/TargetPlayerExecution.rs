// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TargetPlayerExecution.ts: marks a player as the requestor's target.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Targets `target_id` for `requestor` and sours the target's opinion of them.
pub struct TargetPlayerExecution {
    target: Option<SmallId>,
    active: bool,
    requestor: SmallId,
    target_id: String,
}

impl TargetPlayerExecution {
    /// `new TargetPlayerExecution(requestor, targetID)`.
    pub fn new(requestor: SmallId, target_id: String) -> Self {
        Self {
            target: None,
            active: true,
            requestor,
            target_id,
        }
    }
}

impl Execution for TargetPlayerExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.target_id) {
            self.active = false;
            return;
        }
        self.target = Some(game.player_id(&self.target_id));
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let target = self.target.expect("target set in init");
        if game.player(self.requestor).can_target(target) {
            game.player_mut(self.requestor).target(target);
            game.player_mut(target)
                .update_relation(self.requestor, -40.0);
        }
        self.active = false;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
