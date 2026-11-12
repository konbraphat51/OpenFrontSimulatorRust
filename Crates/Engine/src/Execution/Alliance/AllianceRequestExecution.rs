// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/alliance/AllianceRequestExecution.ts: sends or accepts an alliance request.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The client messages of `cancelNukesBetweenAlliedPlayers` are dropped; the unit deletions stay.

use crate::Execution::Execution::Execution;
use crate::Execution::Util::would_nuke_break_alliance;
use crate::Game::AllianceRequestImpl::RequestStatus;
use crate::Game::Game::{AllianceRequestId, SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;

/// A request from `requestor` to `recipient_id` that expires after the configured duration.
pub struct AllianceRequestExecution {
    req: Option<AllianceRequestId>,
    active: bool,
    requestor: SmallId,
    recipient_id: String,
}

impl AllianceRequestExecution {
    /// `new AllianceRequestExecution(requestor, recipientID)`.
    pub fn new(requestor: SmallId, recipient_id: String) -> Self {
        Self {
            req: None,
            active: true,
            requestor,
            recipient_id,
        }
    }

    fn cancel_nukes_between_allied_players(&self, game: &mut GameImpl, recipient: SmallId) {
        let players = [self.requestor, recipient];
        for launcher in players {
            let other = if launcher == self.requestor {
                recipient
            } else {
                self.requestor
            };
            let units = game.player(launcher).units_of_types(&[
                UnitType::AtomBomb,
                UnitType::HydrogenBomb,
                UnitType::MIRV,
                UnitType::MIRVWarhead,
            ]);
            for unit in units {
                let (is_active, reached, target_tile, unit_type, target_player) = {
                    let u = game.unit_any(unit);
                    (
                        u.is_active(),
                        u.reached_target(),
                        u.target_tile(),
                        u.unit_type(),
                        u.target_player(),
                    )
                };
                if !is_active || reached {
                    continue;
                }
                if unit_type == UnitType::MIRV {
                    // The target captured at launch, not the tile's current owner.
                    let target = target_player.or_else(|| target_tile.map(|t| game.owner_id(t)));
                    if target != Some(other) {
                        continue;
                    }
                } else if unit_type == UnitType::MIRVWarhead {
                    match target_tile {
                        Some(t) if game.owner_id(t) == other => {}
                        _ => continue,
                    }
                } else {
                    let Some(t) = target_tile else {
                        continue;
                    };
                    let magnitude = game.config().nuke_magnitudes(unit_type);
                    if !would_nuke_break_alliance(
                        game,
                        t,
                        magnitude,
                        &[other],
                        game.config().nuke_alliance_break_threshold(),
                    ) {
                        continue;
                    }
                }
                game.unit_mut(unit).delete(Some(false), None);
            }
        }
    }
}

impl Execution for AllianceRequestExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has_player(&self.recipient_id) {
            return;
        }
        let recipient = game.player_id(&self.recipient_id);
        if !game
            .player(self.requestor)
            .can_send_alliance_request(recipient)
        {
            self.active = false;
            return;
        }
        let incoming = game
            .player(recipient)
            .outgoing_alliance_requests()
            .into_iter()
            .find(|&r| game.alliance_request(r).recipient() == self.requestor);
        if let Some(incoming) = incoming {
            // The recipient already asked: accept that request instead of creating a new one.
            self.active = false;
            game.accept_request(incoming);
            game.player_mut(self.requestor)
                .update_relation(recipient, 100.0);
            game.player_mut(recipient)
                .update_relation(self.requestor, 100.0);
            // Only embargoes that were created automatically are removed.
            if game.player(self.requestor).has_embargo_against(recipient) {
                game.player_mut(self.requestor)
                    .end_temporary_embargo(recipient);
            }
            if game.player(recipient).has_embargo_against(self.requestor) {
                game.player_mut(recipient)
                    .end_temporary_embargo(self.requestor);
            }
            self.cancel_nukes_between_allied_players(game, recipient);
        } else {
            self.req = game.create_alliance_request(self.requestor, recipient);
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if let Some(req) = self.req {
            let status = game.alliance_request(req).status();
            if status == RequestStatus::Accepted || status == RequestStatus::Rejected {
                self.active = false;
                return;
            }
        }
        let created_at = self
            .req
            .map_or(0.0, |r| game.alliance_request(r).created_at() as f64);
        if game.ticks() as f64 - created_at > game.config().alliance_request_duration() as f64 {
            if let Some(req) = self.req {
                game.reject_request(req);
            }
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
