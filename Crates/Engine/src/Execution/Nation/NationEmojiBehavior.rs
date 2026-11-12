// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationEmojiBehavior.ts: nation emoji reactions.
//! STUB(owner=N1): replaced at merge.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::Game::{EmojiRecipient, SmallId};
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// Stub: every method panics with `not ported: NationEmojiBehavior`.
pub struct NationEmojiBehavior {
    _random: Rc<RefCell<PseudoRandom>>,
    _player: SmallId,
}

impl NationEmojiBehavior {
    /// `new NationEmojiBehavior(random, game, player)`; the game is passed to each method.
    pub fn new(random: Rc<RefCell<PseudoRandom>>, player: SmallId) -> Self {
        Self {
            _random: random,
            _player: player,
        }
    }

    /// `sendEmoji(otherPlayer, emojisList)`.
    pub fn send_emoji(&mut self, _game: &mut GameImpl, _other: EmojiRecipient, _emojis: &[f64]) {
        panic!("not ported: NationEmojiBehavior::send_emoji");
    }

    /// `maybeSendAttackEmoji(otherPlayer)`.
    pub fn maybe_send_attack_emoji(&mut self, _game: &mut GameImpl, _other: SmallId) {
        panic!("not ported: NationEmojiBehavior::maybe_send_attack_emoji");
    }
}
