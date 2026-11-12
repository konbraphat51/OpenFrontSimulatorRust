// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/AllianceRejectExecution.ts: rejects a pending request.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Rejects the request `requestor_id` sent to `recipient`.
pub struct AllianceRejectExecution {
    active: bool,
    requestor_id: String,
    recipient: SmallId,
}

impl AllianceRejectExecution {
    /// `new AllianceRejectExecution(requestorID, recipient)`.
    pub fn new(requestor_id: String, recipient: SmallId) -> Self {
        Self {
            active: true,
            requestor_id,
            recipient,
        }
    }
}

impl Execution for AllianceRejectExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.requestor_id) {
            self.active = false;
            return;
        }
        let requestor = game.player_id(&self.requestor_id);
        if !game.player(requestor).is_friendly(self.recipient, false) {
            let request = game
                .player(requestor)
                .outgoing_alliance_requests()
                .into_iter()
                .find(|&r| game.alliance_request(r).recipient() == self.recipient);
            if let Some(r) = request {
                game.reject_request(r);
            }
        }
        self.active = false;
    }

    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
