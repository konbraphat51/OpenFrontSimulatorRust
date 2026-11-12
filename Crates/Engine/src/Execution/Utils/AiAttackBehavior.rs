// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts: attack decisions shared by nations and tribes.
//! STUB(owner=A2): replaced at merge.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::NationAllianceBehavior::NationAllianceBehavior;
use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Chooses and sends attacks for an AI player.
pub struct AiAttackBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
}

impl AiAttackBehavior {
    /// `new AiAttackBehavior(random, game, player, triggerRatio, reserveRatio, expandRatio,
    /// allianceBehavior?, emojiBehavior?)`; a `Player | TerraNullius` target is a `SmallId` (0 = terra nullius).
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        random: SharedRandom,
        _game: &GameImpl,
        player: SmallId,
        _trigger_ratio: f64,
        _reserve_ratio: f64,
        _expand_ratio: f64,
        _alliance_behavior: Option<Shared<NationAllianceBehavior>>,
        _emoji_behavior: Option<Shared<NationEmojiBehavior>>,
    ) -> Self {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
        Self { random, player }
    }

    /// `maybeAttack()`.
    pub fn maybe_attack(&mut self, _game: &mut GameImpl) {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
    }

    /// `forceSendAttack(target)`.
    pub fn force_send_attack(&mut self, _game: &mut GameImpl, _target: SmallId) {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
    }

    /// `sendAttack(target, force)`.
    pub fn send_attack(&mut self, _game: &mut GameImpl, _target: SmallId, _force: bool) -> bool {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
        false
    }

    /// `attackRandomTarget()`.
    pub fn attack_random_target(&mut self, _game: &mut GameImpl) {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
    }

    /// `findIncomingAttackPlayer()`.
    pub fn find_incoming_attack_player(&mut self, _game: &mut GameImpl) -> Option<SmallId> {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
        None
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(&mut self, _game: &mut GameImpl) -> Option<SmallId> {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
        None
    }

    /// `shouldAttack(other)`.
    pub fn should_attack(&mut self, _game: &mut GameImpl, _other: SmallId) -> bool {
        if !inert_mode() {
            panic!("not ported: AiAttackBehavior");
        }
        false
    }
}
