// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DonateTroopExecution.ts: troop donation with nation reactions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Utils::EmojiIds::{emoji_donation_too_small, emoji_love};
use crate::Game::Game::{Difficulty, EmojiRecipient, PlayerType, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Donates troops (the default donation amount when unspecified) to a friendly player.
pub struct DonateTroopsExecution {
    recipient: Option<SmallId>,
    random: Option<PseudoRandom>,
    active: bool,
    sender: SmallId,
    recipient_id: String,
    troops: Option<f64>,
}

impl DonateTroopsExecution {
    /// `new DonateTroopsExecution(sender, recipientID, troops)`.
    pub fn new(sender: SmallId, recipient_id: String, troops: Option<f64>) -> Self {
        Self {
            recipient: None,
            random: None,
            active: true,
            sender,
            recipient_id,
            troops,
        }
    }

    fn min_troops_for_relation_update(&mut self, game: &GameImpl, recipient: SmallId) -> f64 {
        let max = game.player(recipient).max_troops();
        let random = self.random.as_mut().expect("random set in init");
        match game.config().game_config().difficulty {
            Difficulty::Easy => random.next_int(max / 13.0, max / 11.0),
            Difficulty::Medium => random.next_int(max / 11.0, max / 9.0),
            Difficulty::Hard => random.next_int(max / 9.0, max / 7.0),
            Difficulty::Impossible => random.next_int(max / 7.0, max / 5.0),
        }
    }
}

impl Execution for DonateTroopsExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.random = Some(PseudoRandom::for_rules(
            game.config().rules(),
            game.ticks() as f64,
        ));
        if !game.has_player(&self.recipient_id) {
            self.active = false;
            return;
        }
        let recipient = game.player_id(&self.recipient_id);
        self.recipient = Some(recipient);
        let troops = self.troops.unwrap_or_else(|| {
            game.config()
                .default_donation_amount(game.player(self.sender).troops())
        });
        let max_donation = game.player(recipient).max_troops() - game.player(recipient).troops();
        let troops = troops.min(max_donation);
        self.troops = Some(troops);
        if troops <= 0.0 && game.config().rules().has(Change::DonateTroopZeroInactive31) {
            self.active = false;
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let troops = self.troops.expect("not initialized");
        let recipient = self.recipient.expect("recipient set in init");
        let min_troops = self.min_troops_for_relation_update(game, recipient);
        if game.player(self.sender).can_donate_troops(recipient)
            && game
                .player_mut(self.sender)
                .donate_troops(recipient, troops)
        {
            // A minimum (random) amount is needed, so a good relation cannot be bought with 1%.
            if troops >= min_troops {
                game.player_mut(recipient)
                    .update_relation(self.sender, 50.0);
            }
            if game.player(recipient).player_type() == PlayerType::Nation
                && !game.config().rules().has(Change::RelationPenalties28)
                && game
                    .player(recipient)
                    .can_send_emoji(EmojiRecipient::Player(self.sender))
            {
                let list = if troops >= min_troops {
                    emoji_love()
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
