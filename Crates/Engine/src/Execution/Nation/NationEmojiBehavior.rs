// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationEmojiBehavior.ts: nation emoji decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Nation::SharedRandom;
use crate::Game::Game::{EmojiRecipient, SmallId};
use crate::Game::GameImpl::GameImpl;

/// Decides when a nation sends emojis.
pub struct NationEmojiBehavior {
    #[allow(dead_code)]
    random: SharedRandom,
    #[allow(dead_code)]
    player: SmallId,
}

impl NationEmojiBehavior {
    /// `new NationEmojiBehavior(random, game, player)`.
    pub fn new(random: SharedRandom, _game: &GameImpl, player: SmallId) -> Self {
        Self { random, player }
    }
    /// `maybeSendCasualEmoji()`.
    pub fn maybe_send_casual_emoji(&mut self, _game: &mut GameImpl) {
        panic!("not ported: NationEmojiBehavior");
    }
    /// `maybeSendEmoji(otherPlayer, emojisList)`; emoji ids are indices of `FLATTENED_EMOJI_TABLE`.
    pub fn maybe_send_emoji(
        &mut self,
        _game: &mut GameImpl,
        _other: EmojiRecipient,
        _emojis: &[f64],
    ) {
        panic!("not ported: NationEmojiBehavior");
    }
    /// `maybeSendAttackEmoji(otherPlayer)`.
    pub fn maybe_send_attack_emoji(&mut self, _game: &mut GameImpl, _other: SmallId) {
        panic!("not ported: NationEmojiBehavior");
    }
    /// `sendEmoji(otherPlayer, emojisList)`.
    pub fn send_emoji(&mut self, _game: &mut GameImpl, _other: EmojiRecipient, _emojis: &[f64]) {
        panic!("not ported: NationEmojiBehavior");
    }
}

/// `EMOJI_NUKE` as emoji ids.
pub fn emoji_nuke() -> Vec<f64> {
    panic!("not ported: NationEmojiBehavior");
}
