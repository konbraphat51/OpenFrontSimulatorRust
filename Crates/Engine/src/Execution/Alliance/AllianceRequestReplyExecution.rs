// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/AllianceRequestReplyExecution.ts: accepts or rejects a pending request.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Answers the request `requestor_id` sent to `recipient` (builds before PR 3155).
pub struct AllianceRequestReplyExecution {
    active: bool,
    requestor_id: String,
    recipient: SmallId,
    accept: bool,
}

impl AllianceRequestReplyExecution {
    /// `new AllianceRequestReplyExecution(requestorID, recipient, accept)`.
    pub fn new(requestor_id: String, recipient: SmallId, accept: bool) -> Self {
        Self {
            active: true,
            requestor_id,
            recipient,
            accept,
        }
    }
}

impl Execution for AllianceRequestReplyExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.requestor_id) {
            self.active = false;
            return;
        }
        if !game.has(Change::AllianceReplyInit28) {
            // Before v0.28 the answer is given on the first tick, after the older executions ticked.
            return;
        }
        self.answer(game);
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has(Change::AllianceReplyInit28) && self.active {
            self.answer(game);
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

impl AllianceRequestReplyExecution {
    fn answer(&mut self, game: &mut GameImpl) {
        let requestor = game.player_id(&self.requestor_id);
        if !game.player(requestor).is_friendly(self.recipient, false) {
            let request = game
                .player(requestor)
                .outgoing_alliance_requests()
                .into_iter()
                .find(|&r| game.alliance_request(r).recipient() == self.recipient);
            if let Some(r) = request {
                if self.accept {
                    game.accept_request(r);
                    game.player_mut(requestor)
                        .update_relation(self.recipient, 100.0);
                    game.player_mut(self.recipient)
                        .update_relation(requestor, 100.0);
                } else {
                    game.reject_request(r);
                }
            }
        }
        self.active = false;
    }
}
