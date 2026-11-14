// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AllianceBehavior.ts of v0.28.4: how fake humans answer and send alliance requests.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::Alliance::AllianceRequestExecution::AllianceRequestExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Game::Game::{Difficulty, PlayerType, Relation, SmallId};
use crate::Game::GameImpl::GameImpl;

/// Alliance decisions of one fake human.
pub struct AllianceBehaviorV28 {
    random: SharedRandom,
    player: SmallId,
}

impl AllianceBehaviorV28 {
    /// `new AllianceBehavior(random, game, player)`.
    pub fn new(random: SharedRandom, player: SmallId) -> Self {
        Self { random, player }
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn next_int(&self, min: f64, max: f64) -> f64 {
        self.random.borrow_mut().next_int(min, max)
    }

    /// `handleAllianceRequests()`: accept or reject every pending request.
    pub fn handle_alliance_requests(&mut self, game: &mut GameImpl) {
        for req in game.player(self.player).incoming_alliance_requests() {
            let requestor = game.alliance_request(req).requestor();
            if self.get_alliance_request_decision(game, requestor) {
                game.accept_request(req);
            } else {
                game.reject_alliance_request(req);
            }
        }
    }

    /// `handleAllianceExtensionRequests()`: renew when the human asked and we like them.
    pub fn handle_alliance_extension_requests(&mut self, game: &mut GameImpl) {
        for alliance in game.player(self.player).alliances().to_vec() {
            // Skip if no expiration yet, the ally did not ask, or we already agreed
            if !game.alliance(alliance).only_one_agreed_to_extend() {
                continue;
            }
            let human = game.alliance(alliance).other(self.player);
            if !self.get_alliance_request_decision(game, human) {
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
        let difficulty = game.config().game_config().difficulty;
        for &enemy in bordering_enemies {
            let should_send = match difficulty {
                Difficulty::Easy => self.chance(35.0),
                Difficulty::Medium => self.chance(30.0),
                Difficulty::Hard => self.chance(25.0),
                Difficulty::Impossible => self.chance(20.0),
            };
            // Only easy nations are allowed to send alliance requests to bots
            let enemy_type = game.player(enemy).player_type();
            let acceptable = (enemy_type == PlayerType::Bot && difficulty == Difficulty::Easy)
                || enemy_type != PlayerType::Bot;
            if should_send
                && acceptable
                && game.player(self.player).can_send_alliance_request(enemy)
                && self.get_alliance_request_decision(game, enemy)
            {
                let enemy_id = game.player(enemy).id().to_string();
                game.add_execution(Box::new(AllianceRequestExecution::new(
                    self.player,
                    enemy_id,
                )));
            }
        }
    }

    fn get_alliance_request_decision(&mut self, game: &GameImpl, other: SmallId) -> bool {
        // Easy (dumb) nations sometimes get confused and accept/reject randomly
        if self.is_confused(game) {
            return self.chance(2.0);
        }
        // Nearly always reject traitors
        if game.player(other).is_traitor() && self.next_int(0.0, 100.0) >= 10.0 {
            return false;
        }
        // Before the relation, check whether the other player is a threat
        if self.is_alliance_partner_threat(game, other) {
            return true;
        }
        // Reject if relation is bad
        if game.player(self.player).relation(other) < Relation::Neutral {
            return false;
        }
        // Maybe accept if relation is friendly
        if self.is_alliance_partner_friendly(game, other) {
            return true;
        }
        // Reject if we already have enough alliances
        if self.check_already_enough_alliances(game, other) {
            return false;
        }
        // Accept if we are similarly strong
        self.is_alliance_partner_similarly_strong(game, other)
    }

    fn is_confused(&mut self, game: &GameImpl) -> bool {
        match game.config().game_config().difficulty {
            Difficulty::Easy => self.chance(10.0),
            Difficulty::Medium => self.chance(20.0),
            Difficulty::Hard => self.chance(40.0),
            Difficulty::Impossible => false,
        }
    }

    fn is_alliance_partner_threat(&self, game: &GameImpl, other: SmallId) -> bool {
        let me = game.player(self.player);
        let them = game.player(other);
        let config = game.config();
        match config.game_config().difficulty {
            // On easy we are very dumb, we don't see anybody as a threat
            Difficulty::Easy => false,
            // On medium we just see players with much more troops as a threat
            Difficulty::Medium => them.troops() > me.troops() * 2.5,
            // On hard we check maxTroops to see the actual strength
            Difficulty::Hard => {
                them.troops() > me.troops() && them.max_troops() > me.max_troops() * 2.0
            }
            // On impossible we check multiple factors
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

    fn check_already_enough_alliances(&mut self, game: &GameImpl, other: SmallId) -> bool {
        let difficulty = game.config().game_config().difficulty;
        let me = game.player(self.player);
        match difficulty {
            // On easy we never think we have enough alliances
            Difficulty::Easy => false,
            Difficulty::Medium => me.alliances().len() as f64 >= self.next_int(5.0, 8.0),
            _ => {
                // On hard and impossible we try not to ally with all our neighbours (3+ neighbours)
                let bordering_players: Vec<SmallId> = me
                    .nearby()
                    .into_iter()
                    .filter(|&n| n != 0 && game.player(n).player_type() != PlayerType::Bot)
                    .collect();
                let bordering_friends = bordering_players
                    .iter()
                    .filter(|&&o| me.is_friendly(o, false))
                    .count();
                if bordering_players.len() >= 3 && bordering_players.contains(&other) {
                    return bordering_players.len() <= bordering_friends + 1;
                }
                if difficulty == Difficulty::Hard {
                    return me.alliances().len() as f64 >= self.next_int(3.0, 6.0);
                }
                me.alliances().len() as f64 >= self.next_int(2.0, 5.0)
            }
        }
    }

    fn is_alliance_partner_friendly(&mut self, game: &GameImpl, other: SmallId) -> bool {
        let friendly = game.player(self.player).relation(other) == Relation::Friendly;
        match game.config().game_config().difficulty {
            Difficulty::Easy | Difficulty::Medium => friendly,
            Difficulty::Hard => friendly && self.next_int(0.0, 100.0) >= 17.0,
            Difficulty::Impossible => friendly && self.next_int(0.0, 100.0) >= 33.0,
        }
    }

    // Upstream notes it would suit nextFloat, but floats "can cause desyncs".
    fn is_alliance_partner_similarly_strong(&mut self, game: &GameImpl, other: SmallId) -> bool {
        let (lo, hi) = match game.config().game_config().difficulty {
            Difficulty::Easy => (60.0, 70.0),
            Difficulty::Medium => (70.0, 80.0),
            Difficulty::Hard => (75.0, 85.0),
            Difficulty::Impossible => (80.0, 90.0),
        };
        game.player(other).troops()
            > game.player(self.player).troops() * (self.next_int(lo, hi) / 100.0)
    }
}
