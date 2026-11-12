// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/QuickChatExecution.ts: a quick chat message between two players.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The only state it touches is the sender's quick-chat cooldown; the chat display events are
//! client plumbing.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Sends a quick chat (cooldown bookkeeping only).
pub struct QuickChatExecution {
    sender: SmallId,
    recipient_id: String,
    recipient: Option<SmallId>,
    active: bool,
}

impl QuickChatExecution {
    /// `new QuickChatExecution(sender, recipientID, quickChatKey, target)`.
    pub fn new(sender: SmallId, recipient_id: String) -> Self {
        Self {
            sender,
            recipient_id,
            recipient: None,
            active: true,
        }
    }
}

impl Execution for QuickChatExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        match game.find_player(&self.recipient_id) {
            None => self.active = false,
            Some(r) => self.recipient = Some(r),
        }
    }
    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let recipient = self.recipient.expect("recipient set in init");
        if !game.player(self.sender).can_send_quick_chat(recipient) {
            self.active = false;
            return;
        }
        game.player_mut(self.sender).record_quick_chat(recipient);
        self.active = false;
    }
    fn is_active(&self) -> bool {
        self.active
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
