// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/RailNetworkImpl.ts (STUB until the train phase).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! No station can exist yet, so `recompute_clusters` is a true no-op (upstream returns early when
//! no cluster is dirty). Anything that would create or remove a station panics.

use super::Game::UnitId;

/// Rail network of a game (stub: always empty).
#[derive(Debug, Default)]
pub struct RailNetwork {}

impl RailNetwork {
    /// Creates the empty network.
    pub fn new() -> Self {
        Self {}
    }

    /// `recomputeClusters`: nothing is ever dirty without stations.
    pub fn recompute_clusters(&mut self) {}

    /// `removeStation`; only reachable for units with a train station, which cannot exist yet.
    pub fn remove_station(&mut self, _unit: UnitId) {
        panic!("not ported: RailNetwork::remove_station");
    }
}
