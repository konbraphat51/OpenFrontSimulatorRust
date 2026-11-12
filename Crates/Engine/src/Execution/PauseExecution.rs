// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/PauseExecution.ts: toggles the pause flag for the lobby creator.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{GameType, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Sets the game's paused flag (never affects the simulation itself).
pub struct PauseExecution {
    player: SmallId,
    paused: bool,
}

impl PauseExecution {
    /// `new PauseExecution(player, paused)`.
    pub fn new(player: SmallId, paused: bool) -> Self {
        Self { player, paused }
    }
}

impl Execution for PauseExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if game.player(self.player).is_lobby_creator()
            || game.config().game_config().game_type == GameType::Singleplayer
        {
            game.set_paused(self.paused);
        }
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
