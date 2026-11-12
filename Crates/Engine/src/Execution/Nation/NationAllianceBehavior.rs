// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationAllianceBehavior.ts: nation alliance decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// Decides alliance requests, extensions and betrayals of a nation.
pub struct NationAllianceBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
    #[allow(dead_code)]
    emoji_behavior: Shared<NationEmojiBehavior>,
}

impl NationAllianceBehavior {
    /// `new NationAllianceBehavior(random, game, player, emojiBehavior)`.
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
    /// `handleAllianceRequests()`.
    pub fn handle_alliance_requests(&mut self, _game: &mut GameImpl) {
        panic!("not ported: NationAllianceBehavior");
    }
    /// `handleAllianceExtensionRequests()`.
    pub fn handle_alliance_extension_requests(&mut self, _game: &mut GameImpl) {
        panic!("not ported: NationAllianceBehavior");
    }
    /// `maybeSendAllianceRequests(borderingEnemies)`.
    pub fn maybe_send_alliance_requests(
        &mut self,
        _game: &mut GameImpl,
        _bordering_enemies: &[SmallId],
    ) {
        panic!("not ported: NationAllianceBehavior");
    }
    /// `maybeBetray(otherPlayer, juiciestAlly, borderingFriends, borderingEnemies)`.
    pub fn maybe_betray(
        &mut self,
        _game: &mut GameImpl,
        _other_player: SmallId,
        _juiciest_ally: Option<SmallId>,
        _bordering_friends: &[SmallId],
        _bordering_enemies: &[SmallId],
    ) -> bool {
        panic!("not ported: NationAllianceBehavior");
    }
    /// `findJuiciestAlly(borderingFriends)`.
    pub fn find_juiciest_ally(
        &mut self,
        _game: &GameImpl,
        _bordering_friends: &[SmallId],
    ) -> Option<SmallId> {
        panic!("not ported: NationAllianceBehavior");
    }
}
