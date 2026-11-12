// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts: attack decisions shared by nations and tribes.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A2): replaced at merge.

use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Not-ported stand-in of `AiAttackBehavior` (only the methods the nation behaviours call).
pub struct AiAttackBehavior;

impl AiAttackBehavior {
    /// `shouldAttack(other)`; `other` is a small id, 0 for terra nullius.
    pub fn should_attack(&mut self, _game: &mut GameImpl, _other: SmallId) -> bool {
        panic!("not ported: AiAttackBehavior")
    }

    /// `findIncomingAttackPlayer()`.
    pub fn find_incoming_attack_player(&mut self, _game: &mut GameImpl) -> Option<SmallId> {
        panic!("not ported: AiAttackBehavior")
    }
}
