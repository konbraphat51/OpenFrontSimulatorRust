// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DonateGoldExecution.ts: gold donation with nation reactions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::to_int;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Utils::EmojiIds::{emoji_donation_ok, emoji_donation_too_small, emoji_love};
use crate::Game::Game::{Difficulty, EmojiRecipient, Gold, PlayerType, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Donates gold (a third of the sender's when unspecified) to a friendly player.
pub struct DonateGoldExecution {
    recipient: Option<SmallId>,
    gold: Option<Gold>,
    random: Option<PseudoRandom>,
    active: bool,
    sender: SmallId,
    recipient_id: String,
}

impl DonateGoldExecution {
    /// `new DonateGoldExecution(sender, recipientID, goldNum)`.
    pub fn new(sender: SmallId, recipient_id: String, gold_num: Option<f64>) -> Self {
        Self {
            recipient: None,
            gold: gold_num.map(to_int),
            random: None,
            active: true,
            sender,
            recipient_id,
        }
    }

    fn gold_chunk_size(game: &GameImpl) -> f64 {
        match game.config().game_config().difficulty {
            Difficulty::Easy => 2_500.0,
            Difficulty::Medium => 5_000.0,
            Difficulty::Hard => 12_500.0,
            Difficulty::Impossible => 25_000.0,
        }
    }

    fn calculate_relation_update(game: &GameImpl, gold_sent: Gold, ticks: Tick) -> f64 {
        let chunk_size = Self::gold_chunk_size(game);
        // Every 5 minutes the chunk size grows with the game's progression.
        let multiplier = ticks as f64 / (3000.0 + game.config().num_spawn_phase_turns() as f64);
        let adjusted = (chunk_size + chunk_size * multiplier + 0.5).floor() as i64;
        let chunks = (gold_sent / adjusted) as f64;
        let relation_update = chunks * 5.0;
        if relation_update > 100.0 {
            return 100.0;
        }
        relation_update
    }
}

impl Execution for DonateGoldExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.random = Some(PseudoRandom::for_rules(
            game.config().rules(),
            game.ticks() as f64,
        ));
        if !game.has_player(&self.recipient_id) {
            self.active = false;
            return;
        }
        self.recipient = Some(game.player_id(&self.recipient_id));
        if self.gold.is_none() {
            // Before PR 4892 a missing amount became 0 in the constructor.
            self.gold = Some(if game.has(Change::DonateGoldKeepNull4892) {
                game.player(self.sender).gold() / 3
            } else {
                0
            });
        }
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        let gold = self.gold.expect("not initialized");
        let recipient = self.recipient.expect("recipient set in init");
        if game.player(self.sender).can_donate_gold(recipient)
            && game.player_mut(self.sender).donate_gold(recipient, gold)
        {
            // Relation points grow with the amount donated.
            let relation_update = Self::calculate_relation_update(game, gold, ticks);
            if relation_update > 0.0 {
                game.player_mut(recipient)
                    .update_relation(self.sender, relation_update);
            }
            // Only AI nations answer with an emoji.
            if game.player(recipient).player_type() == PlayerType::Nation
                && !game.has(Change::RelationPenalties28)
                && game
                    .player(recipient)
                    .can_send_emoji(EmojiRecipient::Player(self.sender))
            {
                let list = if relation_update >= 50.0 {
                    emoji_love()
                } else if relation_update > 0.0 {
                    emoji_donation_ok()
                } else {
                    emoji_donation_too_small()
                };
                let random = self.random.as_mut().expect("random set in init");
                let emoji = *random.rand_element(&list);
                let sender_id = game.player(self.sender).id().to_string();
                game.add_execution(Box::new(EmojiExecution::new(recipient, sender_id, emoji)));
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
