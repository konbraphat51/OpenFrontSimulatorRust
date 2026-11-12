// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/AllianceRequestImpl.ts: a pending, accepted or rejected alliance request.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Requests live in `GameImpl.alliance_requests` (never freed, so `pastOutgoingAllianceRequests`
//! can keep ids); the pending list upstream calls `allianceRequests` is `GameImpl.pending_requests`.

use super::Game::{AllianceRequestId, SmallId, Tick};
use super::GameImpl::GameImpl;

/// Status of a request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RequestStatus {
    /// Waiting for an answer.
    Pending,
    /// Accepted.
    Accepted,
    /// Rejected.
    Rejected,
}

/// State of an alliance request.
#[derive(Clone, Debug)]
pub struct AllianceRequestImpl {
    pub(crate) status: RequestStatus,
    pub(crate) requestor: SmallId,
    pub(crate) recipient: SmallId,
    pub(crate) tick_created: Tick,
}

impl AllianceRequestImpl {
    /// `status`.
    pub fn status(&self) -> RequestStatus {
        self.status
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
        self.tick_created
    }
}

impl GameImpl {
    /// `AllianceRequestImpl.accept`.
    pub fn accept_request(&mut self, id: AllianceRequestId) {
        self.alliance_requests[id as usize].status = RequestStatus::Accepted;
        self.accept_alliance_request(id);
    }

    /// `AllianceRequestImpl.reject`.
    pub fn reject_request(&mut self, id: AllianceRequestId) {
        self.alliance_requests[id as usize].status = RequestStatus::Rejected;
        self.reject_alliance_request(id);
    }
}
