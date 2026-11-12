// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/AllianceExtensionExecution.ts: agrees to extend an alliance.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// One player's agreement to extend the alliance with `to_id`; does all its work in `init`.
pub struct AllianceExtensionExecution {
    from: SmallId,
    to_id: String,
}

impl AllianceExtensionExecution {
    /// `new AllianceExtensionExecution(from, toID)`.
    pub fn new(from: SmallId, to_id: String) -> Self {
        Self { from, to_id }
    }
}

impl Execution for AllianceExtensionExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.to_id) {
            return;
        }
        let to = game.player_id(&self.to_id);
        if !game.player(self.from).is_alive() || !game.player(to).is_alive() {
            return;
        }
        let Some(alliance) = game.player(self.from).alliance_with(to) else {
            return;
        };
        game.alliance_mut(alliance).add_extension_request(self.from);
        if game.alliance(alliance).both_agreed_to_extend() {
            game.extend_alliance(alliance);
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
