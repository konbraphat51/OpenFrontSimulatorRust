// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/RetreatExecution.ts: orders an attack retreat, executes it later.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

const CANCEL_DELAY: u32 = 20;

/// Cancels one of the player's outgoing attacks after a delay.
pub struct RetreatExecution {
    active: bool,
    retreat_ordered: bool,
    start_tick: Tick,
    player: SmallId,
    attack_id: String,
}

impl RetreatExecution {
    /// `new RetreatExecution(player, attackID)`.
    pub fn new(player: SmallId, attack_id: String) -> Self {
        Self {
            active: true,
            retreat_ordered: false,
            start_tick: 0,
            player,
            attack_id,
        }
    }

    /// `owner()`.
    pub fn owner(&self) -> SmallId {
        self.player
    }
}

impl Execution for RetreatExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.start_tick = game.ticks();
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !self.retreat_ordered {
            game.player_mut(self.player).order_retreat(&self.attack_id);
            self.retreat_ordered = true;
        }
        if game.ticks() >= self.start_tick + CANCEL_DELAY {
            game.player_mut(self.player)
                .execute_retreat(&self.attack_id);
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
