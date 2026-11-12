// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/BreakAllianceExecution.ts: breaks an alliance.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Breaks the alliance between `requestor` and `recipient_id` on the first tick.
pub struct BreakAllianceExecution {
    active: bool,
    recipient: Option<SmallId>,
    requestor: SmallId,
    recipient_id: String,
}

impl BreakAllianceExecution {
    /// `new BreakAllianceExecution(requestor, recipientID)`.
    pub fn new(requestor: SmallId, recipient_id: String) -> Self {
        Self {
            active: true,
            recipient: None,
            requestor,
            recipient_id,
        }
    }
}

impl Execution for BreakAllianceExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.recipient_id) {
            self.active = false;
            return;
        }
        self.recipient = Some(game.player_id(&self.recipient_id));
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let recipient = self.recipient.expect("Not initialized");
        let alliance = game.player(self.requestor).alliance_with(recipient);
        if let Some(alliance) = alliance {
            game.player_mut(self.requestor).break_alliance(alliance);
            if game.config().rules().has(Change::RelationPenalties28) {
                game.player_mut(recipient)
                    .update_relation(self.requestor, -200.0);
                for player in game.players() {
                    if player != self.requestor
                        && !game.player(player).is_on_same_team(self.requestor)
                    {
                        game.player_mut(player)
                            .update_relation(self.requestor, -40.0);
                    }
                }
                self.active = false;
                return;
            }
            game.player_mut(recipient)
                .update_relation(self.requestor, -100.0);
            let neighbors: Vec<SmallId> = game
                .player(self.requestor)
                .nearby()
                .into_iter()
                .filter(|&n| n != 0 && !game.player(n).is_on_same_team(recipient))
                .collect();
            for neighbor in neighbors {
                game.player_mut(neighbor)
                    .update_relation(self.requestor, -40.0);
            }
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
