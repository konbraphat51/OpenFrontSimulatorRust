// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationAllianceBehavior.ts: nation alliance decisions.
//! STUB(owner=N1): replaced at merge.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// Stub: every method panics with `not ported: NationAllianceBehavior`.
pub struct NationAllianceBehavior {
    _random: Rc<RefCell<PseudoRandom>>,
    _player: SmallId,
    _emoji_behavior: Rc<RefCell<NationEmojiBehavior>>,
}

impl NationAllianceBehavior {
    /// `new NationAllianceBehavior(random, game, player, emojiBehavior)`; the game is passed to each method.
    pub fn new(
        random: Rc<RefCell<PseudoRandom>>,
        player: SmallId,
        emoji_behavior: Rc<RefCell<NationEmojiBehavior>>,
    ) -> Self {
        Self {
            _random: random,
            _player: player,
            _emoji_behavior: emoji_behavior,
        }
    }

    /// `maybeSendAllianceRequests(borderingEnemies)`.
    pub fn maybe_send_alliance_requests(
        &mut self,
        _game: &mut GameImpl,
        _bordering_enemies: &[SmallId],
    ) {
        panic!("not ported: NationAllianceBehavior::maybe_send_alliance_requests");
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
        panic!("not ported: NationAllianceBehavior::maybe_betray");
    }

    /// `findJuiciestAlly(borderingFriends)`.
    pub fn find_juiciest_ally(
        &mut self,
        _game: &mut GameImpl,
        _bordering_friends: &[SmallId],
    ) -> Option<SmallId> {
        panic!("not ported: NationAllianceBehavior::find_juiciest_ally");
    }
}
