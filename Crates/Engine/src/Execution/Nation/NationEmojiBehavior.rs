// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationEmojiBehavior.ts: when nations send emojis.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=N1): replaced at merge.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::Game::{EmojiRecipient, SmallId};
use crate::Game::GameImpl::GameImpl;

/// `EMOJI_NUKE`: the emoji ids to choose from.
pub const EMOJI_NUKE: &[usize] = &[];

/// Not-ported stand-in of `NationEmojiBehavior` (only the methods the N2 behaviours call).
pub struct NationEmojiBehavior;

impl NationEmojiBehavior {
    /// `maybeSendEmoji(otherPlayer, emojisList)`.
    pub fn maybe_send_emoji(&mut self, _game: &mut GameImpl, _other: SmallId, _emojis: &[usize]) {
        panic!("not ported: NationEmojiBehavior")
    }

    /// `maybeSendAttackEmoji(otherPlayer)`.
    pub fn maybe_send_attack_emoji(&mut self, _game: &mut GameImpl, _other: SmallId) {
        panic!("not ported: NationEmojiBehavior")
    }

    /// `sendEmoji(otherPlayer, emojisList)`.
    pub fn send_emoji(&mut self, _game: &mut GameImpl, _other: EmojiRecipient, _emojis: &[usize]) {
        panic!("not ported: NationEmojiBehavior")
    }
}

/// `respondToMIRV(game, random, mirvTarget)`.
pub fn respond_to_mirv(_game: &mut GameImpl, _random: &mut PseudoRandom, _mirv_target: SmallId) {
    panic!("not ported: NationEmojiBehavior")
}
