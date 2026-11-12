// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts: nation nuke decisions.
//! STUB(owner=N2): replaced at merge.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Execution::NotPorted::inert_mode;
use crate::Execution::Utils::AiAttackBehavior::AiAttackBehavior;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Decides when and where a nation launches nukes.
pub struct NationNukeBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
    #[allow(dead_code)]
    attack_behavior: Shared<AiAttackBehavior>,
    #[allow(dead_code)]
    emoji_behavior: Shared<NationEmojiBehavior>,
}

impl NationNukeBehavior {
    /// `new NationNukeBehavior(random, game, player, attackBehavior, emojiBehavior)`.
    /// Upstream draws `random.chance(3)` (isHydroNation) here, after the cost field initialisers.
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        attack_behavior: Shared<AiAttackBehavior>,
        emoji_behavior: Shared<NationEmojiBehavior>,
    ) -> Self {
        if !inert_mode() {
            panic!("not ported: NationNukeBehavior");
        }
        random.borrow_mut().chance(3.0);
        Self {
            random,
            player,
            attack_behavior,
            emoji_behavior,
        }
    }

    /// `maybeSendNuke()`.
    pub fn maybe_send_nuke(&mut self, _game: &mut GameImpl) {
        if !inert_mode() {
            panic!("not ported: NationNukeBehavior");
        }
    }

    /// `findBestNukeTarget()`.
    pub fn find_best_nuke_target(&mut self, _game: &mut GameImpl) -> Option<SmallId> {
        if !inert_mode() {
            panic!("not ported: NationNukeBehavior");
        }
        None
    }
}
