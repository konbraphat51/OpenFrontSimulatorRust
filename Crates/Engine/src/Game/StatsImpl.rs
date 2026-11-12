// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the simulation-visible part of src/core/game/StatsImpl.ts.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The simulation reads back only two values: the attacks a human has sent (`conquerPlayer` skips
//! the gold transfer when it is 0) and the number of MIRVs launched (MIRV cost). Every other
//! stat is record-keeping for the client and is not ported; callers of the other `Stats` methods
//! upstream may skip them.

use std::collections::HashMap;

/// Running totals the simulation depends on.
#[derive(Clone, Debug, Default)]
pub struct StatsImpl {
    attacks_sent: HashMap<String, i64>,
    num_mirvs_launched: i64,
}

impl StatsImpl {
    /// `attack(player, target, troops)`: records `troops` (floored) as sent by the client; players
    /// without a client id (nations, tribes) have no stats upstream.
    pub fn attack(&mut self, client_id: Option<&str>, troops: f64) {
        if let Some(id) = client_id {
            *self.attacks_sent.entry(id.to_string()).or_insert(0) += troops.floor() as i64;
        }
    }

    /// `getPlayerStats(player)?.attacks?.[ATTACK_INDEX_SENT] ?? 0n`.
    pub fn attacks_sent(&self, client_id: Option<&str>) -> i64 {
        client_id
            .and_then(|id| self.attacks_sent.get(id).copied())
            .unwrap_or(0)
    }

    /// `bombLaunch` for a MIRV.
    pub fn mirv_launched(&mut self) {
        self.num_mirvs_launched += 1;
    }

    /// `numMirvsLaunched()`.
    pub fn num_mirvs_launched(&self) -> i64 {
        self.num_mirvs_launched
    }
}
