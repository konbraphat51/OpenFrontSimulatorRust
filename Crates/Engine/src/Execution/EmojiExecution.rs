// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/EmojiExecution.ts plus `respondToEmoji` of nation/NationEmojiBehavior.ts.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Sending an emoji records it for the cooldown checks and, when the recipient is a nation, lets
//! it respond (relation changes and follow-up emojis), which is why this is ported fully.

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::FLATTENED_EMOJI_TABLE;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{
    ALL_PLAYERS, Difficulty, EmojiRecipient, PlayerType, Relation, SmallId, Tick,
};
use crate::Game::GameImpl::GameImpl;

fn emoji_id(e: &str) -> f64 {
    FLATTENED_EMOJI_TABLE
        .iter()
        .position(|x| *x == e)
        .map_or(-1.0, |i| i as f64)
}

fn ids(list: &[&str]) -> Vec<f64> {
    list.iter().map(|e| emoji_id(e)).collect()
}

/// Sends one emoji from `requestor`.
pub struct EmojiExecution {
    requestor: SmallId,
    recipient_id: String,
    emoji: f64,
    recipient: Option<EmojiRecipient>,
    random: Option<PseudoRandom>,
    active: bool,
}

impl EmojiExecution {
    /// `new EmojiExecution(requestor, recipientID, emoji)`; `recipient_id` may be `"AllPlayers"`.
    pub fn new(requestor: SmallId, recipient_id: String, emoji: f64) -> Self {
        Self {
            requestor,
            recipient_id,
            emoji,
            recipient: None,
            random: None,
            active: true,
        }
    }
}

impl Execution for EmojiExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.random = Some(PseudoRandom::for_rules(
            game.config().rules(),
            game.ticks() as f64,
        ));
        if self.recipient_id != ALL_PLAYERS && !game.has_player(&self.recipient_id) {
            self.active = false;
            return;
        }
        self.recipient = Some(if self.recipient_id == ALL_PLAYERS {
            EmojiRecipient::All
        } else {
            EmojiRecipient::Player(game.player_id(&self.recipient_id))
        });
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let recipient = self.recipient.expect("recipient set in init");
        let emoji_string = if self.emoji >= 0.0 && self.emoji < FLATTENED_EMOJI_TABLE.len() as f64 {
            Some(FLATTENED_EMOJI_TABLE[self.emoji as usize])
        } else {
            None
        };
        if let Some(emoji_string) = emoji_string
            && game.player(self.requestor).can_send_emoji(recipient)
        {
            game.player_mut(self.requestor)
                .send_emoji(recipient, emoji_string);
            if game.has(Change::RelationPenalties28) {
                // Nations only take offence at the middle finger
                if let EmojiRecipient::Player(r) = recipient
                    && emoji_string == "\u{1f595}"
                    && game.player(r).player_type() == PlayerType::Nation
                {
                    game.player_mut(r).update_relation(self.requestor, -100.0);
                }
            } else {
                let random = self.random.as_mut().expect("random set in init");
                respond_to_emoji(game, random, self.requestor, recipient, emoji_string);
            }
        }
        self.active = false;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}

/// `respondToEmoji`: nations answer insults, clowns and peace signs.
pub fn respond_to_emoji(
    game: &mut GameImpl,
    random: &mut PseudoRandom,
    sender: SmallId,
    recipient: EmojiRecipient,
    emoji_string: &str,
) {
    let EmojiRecipient::Player(recipient) = recipient else {
        return;
    };
    if game.player(recipient).player_type() != PlayerType::Nation {
        return;
    }
    if !game
        .player(recipient)
        .can_send_emoji(EmojiRecipient::Player(sender))
    {
        return;
    }
    let sender_id = game.player(sender).id().to_string();
    let pick = |random: &mut PseudoRandom, list: &[f64]| *random.rand_element(list);

    if emoji_string == "\u{1f595}" {
        game.player_mut(recipient).update_relation(sender, -100.0);
        let got_insulted = ids(&[
            "\u{1f595}",
            "\u{1f621}",
            "\u{1f921}",
            "\u{1f61e}",
            "\u{1f62d}",
        ]);
        let e = pick(random, &got_insulted);
        game.add_execution(Box::new(EmojiExecution::new(
            recipient,
            sender_id.clone(),
            e,
        )));
    }

    if emoji_string == "\u{1f921}" {
        game.player_mut(recipient).update_relation(sender, -10.0);
        let confused = ids(&["\u{2753}", "\u{1f921}"]);
        let e = pick(random, &confused);
        game.add_execution(Box::new(EmojiExecution::new(
            recipient,
            sender_id.clone(),
            e,
        )));
    }

    const PEACE: [&str; 5] = [
        "\u{1f54a}\u{fe0f}",
        "\u{1f3f3}\u{fe0f}",
        "\u{2764}\u{fe0f}",
        "\u{1f970}",
        "\u{1f44f}",
    ];
    if PEACE.contains(&emoji_string) {
        if game.config().game_config().difficulty == Difficulty::Easy {
            game.player_mut(recipient).update_relation(sender, 15.0);
        }
        let love = ids(&["\u{2764}\u{fe0f}", "\u{1f60a}", "\u{1f970}"]);
        let confused = ids(&["\u{2753}", "\u{1f921}"]);
        let friendly = game.player(sender).relation(recipient) >= Relation::Neutral;
        let e = if friendly {
            pick(random, &love)
        } else {
            pick(random, &confused)
        };
        game.add_execution(Box::new(EmojiExecution::new(recipient, sender_id, e)));
    }
}
