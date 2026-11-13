// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/AllianceImpl.ts: one alliance between two players.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Alliances live in `GameImpl.alliances`; the handle `AllianceId` equals the upstream alliance
//! id (sequential from 0). Methods that call back into the game are `GameImpl` methods.

use super::Game::{AllianceId, SmallId, Tick};
use super::GameImpl::GameImpl;

/// State of an alliance.
#[derive(Clone, Debug)]
pub struct AllianceImpl {
    pub(crate) requestor: SmallId,
    pub(crate) recipient: SmallId,
    pub(crate) created_at: Tick,
    pub(crate) id: AllianceId,
    pub(crate) extension_requested_requestor: bool,
    pub(crate) extension_requested_recipient: bool,
    pub(crate) expires_at: Tick,
}

impl AllianceImpl {
    /// `new AllianceImpl(...)`: expires `allianceDuration()` after creation.
    pub fn new(
        game: &GameImpl,
        requestor: SmallId,
        recipient: SmallId,
        created_at: Tick,
        id: AllianceId,
    ) -> Self {
        Self {
            requestor,
            recipient,
            created_at,
            id,
            extension_requested_requestor: false,
            extension_requested_recipient: false,
            expires_at: created_at + game.config().alliance_duration(),
        }
    }

    /// `other(player)`.
    pub fn other(&self, player: SmallId) -> SmallId {
        if self.requestor == player {
            self.recipient
        } else {
            self.requestor
        }
    }
    /// `requestor`.
    pub fn requestor(&self) -> SmallId {
        self.requestor
    }
    /// `recipient`.
    pub fn recipient(&self) -> SmallId {
        self.recipient
    }
    /// `createdAt`.
    pub fn created_at(&self) -> Tick {
        self.created_at
    }
    /// `id`.
    pub fn id(&self) -> AllianceId {
        self.id
    }
    /// `expiresAt`.
    pub fn expires_at(&self) -> Tick {
        self.expires_at
    }
    /// `addExtensionRequest(player)`.
    pub fn add_extension_request(&mut self, player: SmallId) {
        if self.requestor == player {
            self.extension_requested_requestor = true;
        } else if self.recipient == player {
            self.extension_requested_recipient = true;
        }
    }
    /// `bothAgreedToExtend`.
    pub fn both_agreed_to_extend(&self) -> bool {
        self.extension_requested_requestor && self.extension_requested_recipient
    }
    /// `onlyOneAgreedToExtend`.
    pub fn only_one_agreed_to_extend(&self) -> bool {
        self.extension_requested_requestor != self.extension_requested_recipient
    }
}

impl GameImpl {
    /// `AllianceImpl.expire`.
    pub fn expire_alliance_by_id(&mut self, id: AllianceId) {
        self.expire_alliance(id);
    }

    /// `AllianceImpl.extend`: clears both requests and restarts the clock.
    pub fn extend_alliance(&mut self, id: AllianceId) {
        let duration = self.config().alliance_duration();
        let now = self.ticks();
        let a = &mut self.alliances[id as usize];
        a.extension_requested_requestor = false;
        a.extension_requested_recipient = false;
        a.expires_at = now + duration;
    }
}
