// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/EmbargoExecution.ts: starts or stops an embargo against one player.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Starts (`action == "start"`) or stops an embargo of `player` against `target_id`.
pub struct EmbargoExecution {
    active: bool,
    target: Option<SmallId>,
    player: SmallId,
    target_id: String,
    action: String,
}

impl EmbargoExecution {
    /// `new EmbargoExecution(player, targetID, action)`.
    pub fn new(player: SmallId, target_id: String, action: String) -> Self {
        Self {
            active: true,
            target: None,
            player,
            target_id,
            action,
        }
    }
}

impl Execution for EmbargoExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.target_id) {
            self.active = false;
            return;
        }
        self.target = Some(game.player_id(&self.target_id));
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let target = self.target.expect("target set in init");
        if self.action == "start" {
            game.player_mut(self.player).add_embargo(target, false);
        } else {
            game.player_mut(self.player).stop_embargo(target);
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
