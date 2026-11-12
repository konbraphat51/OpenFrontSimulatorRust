// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationMIRVBehavior.ts: nation MIRV decisions.
//! STUB(owner=N2): replaced at merge.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Decides when a nation launches a MIRV.
pub struct NationMIRVBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
    #[allow(dead_code)]
    emoji_behavior: Shared<NationEmojiBehavior>,
}

impl NationMIRVBehavior {
    /// `new NationMIRVBehavior(random, game, player, emojiBehavior)`.
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        emoji_behavior: Shared<NationEmojiBehavior>,
    ) -> Self {
        if !inert_mode() {
            panic!("not ported: NationMIRVBehavior");
        }
        Self {
            random,
            player,
            emoji_behavior,
        }
    }

    /// `considerMIRV()`.
    pub fn consider_mirv(&mut self, _game: &mut GameImpl) -> bool {
        if !inert_mode() {
            panic!("not ported: NationMIRVBehavior");
        }
        false
    }
}
