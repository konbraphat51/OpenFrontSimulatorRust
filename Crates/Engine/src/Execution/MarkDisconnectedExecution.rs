// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/MarkDisconnectedExecution.ts: records a player's connection state.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{DisconnectSnapshot, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Marks a player (dis)connected in `init`; never ticks.
pub struct MarkDisconnectedExecution {
    player: SmallId,
    is_disconnected: bool,
}

impl MarkDisconnectedExecution {
    /// `new MarkDisconnectedExecution(player, isDisconnected)`.
    pub fn new(player: SmallId, is_disconnected: bool) -> Self {
        Self {
            player,
            is_disconnected,
        }
    }
}

impl Execution for MarkDisconnectedExecution {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick) {
        if self.is_disconnected {
            let team_tiles = match game.player(self.player).team().cloned() {
                Some(t) => game.team_tiles_owned(&t),
                None => 0.0,
            };
            let total_land = game.total_land_tiles();
            let was_alive = game.player(self.player).is_alive();
            game.player_mut(self.player).mark_disconnected(
                true,
                Some(DisconnectSnapshot {
                    current_tick: ticks,
                    team_tiles,
                    total_land,
                    was_alive,
                }),
            );
        } else {
            game.player_mut(self.player).mark_disconnected(false, None);
        }
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
