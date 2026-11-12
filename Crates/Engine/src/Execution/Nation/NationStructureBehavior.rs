// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts: nation structure building.
//! STUB(owner=N2): replaced at merge.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::SharedRandom;
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Decides which structures a nation builds or upgrades.
pub struct NationStructureBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
}

impl NationStructureBehavior {
    /// `new NationStructureBehavior(random, game, player)`.
    pub fn new(random: SharedRandom, _game: &GameImpl, player: SmallId) -> Self {
        if !inert_mode() {
            panic!("not ported: NationStructureBehavior");
        }
        Self { random, player }
    }

    /// `handleStructures()`.
    pub fn handle_structures(&mut self, _game: &mut GameImpl) -> bool {
        if !inert_mode() {
            panic!("not ported: NationStructureBehavior");
        }
        false
    }
}
