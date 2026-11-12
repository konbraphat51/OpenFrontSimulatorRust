// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationAllianceBehavior.ts: nation alliance decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::Alliance::AllianceRequestExecution::AllianceRequestExecution;
use crate::Execution::Nation::NationEmojiBehavior::{
    NationEmojiBehavior, emoji_confused, emoji_handshake, emoji_love, emoji_scared_of_threat,
};
use crate::Execution::Nation::NationUtils::find_juiciest_target;
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Game::Game::{Difficulty, EmojiRecipient, GameMode, PlayerType, Relation, SmallId};
use crate::Game::GameImpl::GameImpl;

/// Decides alliance requests, extensions and betrayals of a nation.
pub struct NationAllianceBehavior {
    random: SharedRandom,
    player: SmallId,
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

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn next_int(&self, min: f64, max: f64) -> f64 {
        self.random.borrow_mut().next_int(min, max)
    }

    fn send_emoji(&self, game: &mut GameImpl, other: SmallId, list: &[f64]) {
        self.emoji_behavior
            .borrow_mut()
            .send_emoji(game, EmojiRecipient::Player(other), list);
    }

    /// `handleAllianceRequests()`.
    pub fn handle_alliance_requests(&mut self, game: &mut GameImpl) {
        if game.config().disable_alliances() {
            return;
        }
        for req in game.player(self.player).incoming_alliance_requests() {
            // Requests created during the spawn phase run on the first tick after it: reject them.
            if game.alliance_request(req).created_at() <= game.config().num_spawn_phase_turns() + 1
            {
                game.reject_request(req);
                continue;
            }
            let requestor = game.alliance_request(req).requestor();
            if self.get_alliance_decision(game, requestor, true) {
                game.accept_request(req);
            } else {
                game.reject_request(req);
            }
        }
    }

    /// `handleAllianceExtensionRequests()`.
    pub fn handle_alliance_extension_requests(&mut self, game: &mut GameImpl) {
        if game.config().disable_alliances() {
            return;
        }
        let alliances: Vec<_> = game.player(self.player).alliances().to_vec();
        for alliance in alliances {
            // Only act when the ally asked for an extension and we have not agreed yet.
            if !game.alliance(alliance).only_one_agreed_to_extend() {
                continue;
            }
            let human = game.alliance(alliance).other(self.player);
            if !self.get_alliance_decision(game, human, true) {
                continue;
            }
            let human_id = game.player(human).id().to_string();
            game.add_execution(Box::new(AllianceExtensionExecution::new(
                self.player,
                human_id,
            )));
        }
    }

    /// `maybeSendAllianceRequests(borderingEnemies)`.
    pub fn maybe_send_alliance_requests(
        &mut self,
        game: &mut GameImpl,
        bordering_enemies: &[SmallId],
    ) {
        if game.config().disable_alliances() {
            return;
        }
        // Only easy nations may send alliance requests to bots.
        let is_acceptable_player_type = |game: &GameImpl, p: SmallId| {
            (game.player(p).player_type() == PlayerType::Bot
                && game.config().game_config().difficulty == Difficulty::Easy)
                || game.player(p).player_type() != PlayerType::Bot
        };
        for &enemy in bordering_enemies {
            if self.chance(30.0)
                && is_acceptable_player_type(game, enemy)
                && game.player(self.player).can_send_alliance_request(enemy)
                && self.get_alliance_decision(game, enemy, false)
            {
                let enemy_id = game.player(enemy).id().to_string();
                game.add_execution(Box::new(AllianceRequestExecution::new(
                    self.player,
                    enemy_id,
                )));
            }
        }
    }

    fn get_alliance_decision(
        &mut self,
        game: &mut GameImpl,
        other: SmallId,
        is_response: bool,
    ) -> bool {
        // Easy (dumb) nations sometimes accept/reject randomly.
        if self.is_confused(game) {
            return self.chance(2.0);
        }
        // Nearly always reject traitors.
        if game.player(other).is_traitor() && self.next_int(0.0, 100.0) >= 10.0 {
            if is_response && self.chance(3.0) {
                self.send_emoji(game, other, &emoji_confused());
            }
            return false;
        }
        if self.has_too_many_alliances(game, other) {
            return false;
        }
        // Check whether the other player is a threat before looking at the relation.
        if self.is_alliance_partner_threat(game, other) {
            if !is_response && self.chance(6.0) {
                self.send_emoji(game, other, &emoji_scared_of_threat());
            }
            if is_response && self.chance(6.0) {
                self.send_emoji(game, other, &emoji_love());
            }
            return true;
        }
        if self.should_reject_in_team_game(game) {
            return false;
        }
        if game.player(self.player).relation(other) < Relation::Neutral {
            if is_response && self.chance(3.0) {
                self.send_emoji(game, other, &emoji_confused());
            }
            return false;
        }
        if self.is_alliance_partner_friendly(game, other) {
            if self.chance(3.0) {
                self.send_emoji(game, other, &emoji_handshake());
            }
            return true;
        }
        if self.check_already_enough_alliances(game, other) {
            return false;
        }
        if self.is_earlygame(game) {
            return true;
        }
        self.is_alliance_partner_similarly_strong(game, other)
    }

    fn has_too_many_alliances(&self, game: &GameImpl, other: SmallId) -> bool {
        let difficulty = game.config().game_config().difficulty;
        if difficulty != Difficulty::Hard && difficulty != Difficulty::Impossible {
            return false;
        }
        let total_players = game
            .players()
            .into_iter()
            .filter(|&p| game.player(p).player_type() != PlayerType::Bot)
            .count() as f64;
        let other_alliances = game.player(other).alliances().len() as f64;
        if difficulty == Difficulty::Hard {
            other_alliances >= total_players * 0.5
        } else {
            other_alliances >= total_players * 0.25
        }
    }

    fn is_confused(&self, game: &GameImpl) -> bool {
        match game.config().game_config().difficulty {
            Difficulty::Easy => self.chance(10.0),
            Difficulty::Medium => self.chance(20.0),
            Difficulty::Hard => self.chance(40.0),
            Difficulty::Impossible => false,
        }
    }

    fn is_earlygame(&self, game: &GameImpl) -> bool {
        let spawn_ticks = game.config().num_spawn_phase_turns();
        let ticks = game.ticks();
        match game.config().game_config().difficulty {
            Difficulty::Easy => ticks < 3000 + spawn_ticks && self.next_int(0.0, 100.0) >= 10.0,
            Difficulty::Medium => ticks < 1800 + spawn_ticks && self.next_int(0.0, 100.0) >= 30.0,
            Difficulty::Hard => ticks < 1800 + spawn_ticks && self.next_int(0.0, 100.0) >= 50.0,
            Difficulty::Impossible => {
                ticks < 600 + spawn_ticks && self.next_int(0.0, 100.0) >= 70.0
            }
        }
    }

    fn is_alliance_partner_threat(&self, game: &GameImpl, other: SmallId) -> bool {
        let me = game.player(self.player);
        let them = game.player(other);
        match game.config().game_config().difficulty {
            Difficulty::Easy => false,
            Difficulty::Medium => them.troops() > me.troops() * 2.5,
            Difficulty::Hard => {
                them.troops() > me.troops() && them.max_troops() > me.max_troops() * 2.0
            }
            Difficulty::Impossible => {
                let other_has_more_troops = them.troops() > me.troops() * 1.5;
                let other_has_more_max_troops =
                    them.troops() > me.troops() && them.max_troops() > me.max_troops() * 1.5;
                let other_has_more_tiles = them.troops() > me.troops()
                    && them.num_tiles_owned() as f64 > me.num_tiles_owned() as f64 * 1.5;
                other_has_more_troops || other_has_more_max_troops || other_has_more_tiles
            }
        }
    }

    fn should_reject_in_team_game(&self, game: &GameImpl) -> bool {
        if game.config().game_config().game_mode != GameMode::Team {
            return false;
        }
        match game.config().game_config().difficulty {
            Difficulty::Easy => self.next_int(0.0, 100.0) < 25.0,
            Difficulty::Medium => self.next_int(0.0, 100.0) < 50.0,
            Difficulty::Hard => self.next_int(0.0, 100.0) < 75.0,
            Difficulty::Impossible => true,
        }
    }

    fn check_already_enough_alliances(&self, game: &GameImpl, other: SmallId) -> bool {
        let difficulty = game.config().game_config().difficulty;
        let alliances = game.player(self.player).alliances().len() as f64;
        match difficulty {
            Difficulty::Easy => false,
            Difficulty::Medium => alliances >= self.next_int(4.0, 6.0),
            Difficulty::Hard | Difficulty::Impossible => {
                // Try not to ally with all neighbours (when there are 2 or more).
                let bordering_players: Vec<SmallId> = game
                    .player(self.player)
                    .nearby()
                    .into_iter()
                    .filter(|&n| n != 0 && game.player(n).player_type() != PlayerType::Bot)
                    .collect();
                let bordering_friends = bordering_players
                    .iter()
                    .filter(|&&o| game.player(self.player).is_friendly(o, false))
                    .count();
                if bordering_players.len() >= 2 && bordering_players.contains(&other) {
                    return bordering_players.len() <= bordering_friends + 1;
                }
                if difficulty == Difficulty::Hard {
                    return alliances >= self.next_int(3.0, 5.0);
                }
                alliances >= self.next_int(2.0, 4.0)
            }
        }
    }

    fn is_alliance_partner_friendly(&self, game: &GameImpl, other: SmallId) -> bool {
        let friendly = game.player(self.player).relation(other) == Relation::Friendly;
        match game.config().game_config().difficulty {
            Difficulty::Easy | Difficulty::Medium => friendly,
            Difficulty::Hard => friendly && self.next_int(0.0, 100.0) >= 17.0,
            Difficulty::Impossible => friendly && self.next_int(0.0, 100.0) >= 33.0,
        }
    }

    fn outgoing_troops(game: &GameImpl, player: SmallId) -> f64 {
        game.player(player)
            .outgoing_attacks()
            .iter()
            .fold(0.0, |sum, &a| sum + game.attack(a).troops())
    }

    fn is_alliance_partner_similarly_strong(&self, game: &GameImpl, other: SmallId) -> bool {
        let (troop_range, tile_range) = match game.config().game_config().difficulty {
            Difficulty::Easy => ((60.0, 70.0), (70.0, 80.0)),
            Difficulty::Medium => ((70.0, 80.0), (80.0, 90.0)),
            Difficulty::Hard => ((75.0, 85.0), (85.0, 95.0)),
            Difficulty::Impossible => ((80.0, 90.0), (90.0, 100.0)),
        };
        let player_outgoing = Self::outgoing_troops(game, self.player);
        let other_outgoing = Self::outgoing_troops(game, other);
        let player_total_troops = game.player(self.player).troops() + player_outgoing;
        let other_total_troops = game.player(other).troops() + other_outgoing;

        let troop_threshold =
            player_total_troops * (self.next_int(troop_range.0, troop_range.1) / 100.0);
        let tile_threshold = game.player(self.player).num_tiles_owned() as f64
            * (self.next_int(tile_range.0, tile_range.1) / 100.0);

        let has_comparable_troops = other_total_troops > troop_threshold;
        let has_comparable_tiles = game.player(other).num_tiles_owned() as f64 > tile_threshold
            && other_total_troops > player_total_troops * 0.5;
        has_comparable_troops || has_comparable_tiles
    }

    /// `maybeBetray(otherPlayer, juiciestAlly, borderingFriends, borderingEnemies)`.
    pub fn maybe_betray(
        &mut self,
        game: &mut GameImpl,
        other_player: SmallId,
        juiciest_ally: Option<SmallId>,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        if !game.player(self.player).is_allied_with(other_player) {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;

        // Betray our juiciest ally when everybody around us is weak.
        if (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && juiciest_ally == Some(other_player)
            && self.is_safe_to_betray(game, other_player, bordering_friends, bordering_enemies)
        {
            self.betray(game, other_player);
            return true;
        }

        // Betray very weak players on the easier difficulties (not humans on easy).
        if (difficulty == Difficulty::Easy || difficulty == Difficulty::Medium)
            && !(difficulty == Difficulty::Easy
                && game.player(other_player).player_type() == PlayerType::Human)
            && game.player(self.player).troops() >= game.player(other_player).troops() * 10.0
        {
            self.betray(game, other_player);
            return true;
        }

        // Betray traitors who are not significantly stronger than us.
        if difficulty != Difficulty::Easy
            && game.player(other_player).is_traitor()
            && game.player(other_player).troops() < game.player(self.player).troops() * 1.2
        {
            self.betray(game, other_player);
            return true;
        }

        // Betray our only bordering player when we are much stronger.
        if difficulty != Difficulty::Easy
            && bordering_friends.len() + bordering_enemies.len() == 1
            && game.player(other_player).troops() * 3.0 < game.player(self.player).troops()
        {
            self.betray(game, other_player);
            return true;
        }

        false
    }

    /// `findJuiciestAlly(borderingFriends)`.
    pub fn find_juiciest_ally(
        &mut self,
        game: &GameImpl,
        bordering_friends: &[SmallId],
    ) -> Option<SmallId> {
        let candidates: Vec<SmallId> = bordering_friends
            .iter()
            .copied()
            .filter(|&f| game.player(self.player).is_allied_with(f))
            .collect();
        find_juiciest_target(game, &candidates)
    }

    fn is_safe_to_betray(
        &self,
        game: &GameImpl,
        target: SmallId,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        let other_allies: Vec<SmallId> = if game.player(target).is_traitor() {
            Vec::new()
        } else {
            bordering_friends
                .iter()
                .copied()
                .filter(|&f| f != target && game.player(self.player).is_allied_with(f))
                .collect()
        };
        let mut threats = vec![target];
        threats.extend_from_slice(bordering_enemies);
        threats.extend(other_allies);
        let nearby_threat_troops = threats.iter().fold(0.0, |sum, &threat| {
            let outgoing = Self::outgoing_troops(game, threat);
            sum + game.player(threat).troops() + outgoing
        });
        nearby_threat_troops < game.player(self.player).troops() * 0.33
    }

    fn betray(&self, game: &mut GameImpl, target: SmallId) {
        let Some(alliance) = game.player(self.player).alliance_with(target) else {
            return;
        };
        game.player_mut(self.player).break_alliance(alliance);
    }
}
