// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationWarshipBehavior.ts: nation warship decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Decides warship spawning, retaliation and infestation counters of a nation.
pub struct NationWarshipBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
    #[allow(dead_code)]
    emoji_behavior: Shared<NationEmojiBehavior>,
}

impl NationWarshipBehavior {
    /// `new NationWarshipBehavior(random, game, player, emojiBehavior)`.
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        emoji_behavior: Shared<NationEmojiBehavior>,
    ) -> Self {
        Self {
            random,
            player,
            emoji_behavior,
        }
    }
    /// `maybeSpawnWarship()`.
    pub fn maybe_spawn_warship(&mut self, _game: &mut GameImpl) -> bool {
        panic!("not ported: NationWarshipBehavior");
    }
    /// `trackShipsAndRetaliate()`.
    pub fn track_ships_and_retaliate(&mut self, _game: &mut GameImpl) {
        panic!("not ported: NationWarshipBehavior");
    }
    /// `counterWarshipInfestation()`.
    pub fn counter_warship_infestation(&mut self, _game: &mut GameImpl) {
        panic!("not ported: NationWarshipBehavior");
    }
}
