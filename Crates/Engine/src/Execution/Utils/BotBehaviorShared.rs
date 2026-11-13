// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/BotBehavior.ts: the helpers every version of the tribe and fake human attack behaviour shares unchanged.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `BotBehaviorV25` to `V28` keep their own enemy selection and attack decisions; code that never changed between them lives
//! here as default methods over a few accessors. A version only uses the helpers its upstream build has.

use crate::Core::Util::{bounding_box_center, calculate_bounding_box_center};
use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Game::Game::{Cell, PlayerType, Relation, SmallId};
use crate::Game::GameImpl::GameImpl;

/// State accessors and shared helpers of a tribe or fake human behaviour.
pub trait BotBehaviorShared {
    /// The shared random stream.
    fn random(&self) -> &SharedRandom;
    /// The controlled player.
    fn player(&self) -> SmallId;
    /// Troop ratio above which the player attacks.
    fn trigger_ratio(&self) -> f64;
    /// Troop ratio kept in reserve.
    fn reserve_ratio(&self) -> f64;

    fn add_attack(&self, game: &mut GameImpl, troops: f64, target: SmallId) {
        let target_id = (target != 0).then(|| game.player(target).id().to_string());
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player(),
            target_id,
            None,
            true,
        )));
    }

    fn chance(&self, odds: f64) -> bool {
        self.random().borrow_mut().chance(odds)
    }

    fn emoji(&self, game: &mut GameImpl, player: SmallId, emoji: f64) {
        if game.player(player).player_type() != PlayerType::Human {
            return;
        }
        let id = game.player(player).id().to_string();
        game.add_execution(Box::new(EmojiExecution::new(self.player(), id, emoji)));
    }

    fn is_friendly(&self, game: &GameImpl, other: SmallId) -> bool {
        game.player(self.player()).is_friendly(other, false)
    }

    fn betray(&self, game: &mut GameImpl, target: SmallId) {
        let Some(alliance) = game.player(self.player()).alliance_with(target) else {
            return;
        };
        game.player_mut(self.player()).break_alliance(alliance);
    }

    /// `forceSendAttack(target)`: half our troops, whatever the reserve.
    fn force_send_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        let troops = game.player(self.player()).troops() / 2.0;
        self.add_attack(game, troops, target);
    }

    fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player());
        p.troops() / p.max_troops() >= self.reserve_ratio()
    }

    fn has_trigger_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player());
        p.troops() / p.max_troops() >= self.trigger_ratio()
    }

    fn rand_element(&self, list: &[f64]) -> f64 {
        *self.random().borrow_mut().rand_element(list)
    }

    fn get_player_center(&self, game: &GameImpl, player: SmallId) -> Cell {
        let p = game.player(player);
        if let Some((min, max)) = p.largest_cluster_bounding_box {
            return bounding_box_center(min, max);
        }
        calculate_bounding_box_center(&game.map, p.border_tiles().iter())
            .expect("a player with a border has a center")
    }
}

/// Alliance request handling of the builds before `AllianceBehavior` (v0.25 to v0.27).
pub trait BotBehaviorAlliances: BotBehaviorShared {
    /// `shouldAcceptAllianceRequest(player, request)`.
    fn should_accept_alliance_request(&self, game: &GameImpl, requestor: SmallId) -> bool {
        let me = game.player(self.player());
        let them = game.player(requestor);
        if me.relation(requestor) < Relation::Neutral {
            return false; // Reject if hasMalice
        }
        if them.is_traitor() {
            return false; // Reject if isTraitor
        }
        if them.num_tiles_owned() as f64 > me.num_tiles_owned() as f64 * 3.0 {
            return true; // Accept if requestorIsMuchLarger
        }
        if them.alliances().len() >= 3 {
            return false; // Reject if tooManyAlliances
        }
        true
    }

    /// `handleAllianceRequests()`.
    fn handle_alliance_requests(&mut self, game: &mut GameImpl) {
        for req in game.player(self.player()).incoming_alliance_requests() {
            let requestor = game.alliance_request(req).requestor();
            if self.should_accept_alliance_request(game, requestor) {
                game.accept_request(req);
            } else {
                game.reject_alliance_request(req);
            }
        }
    }

    /// `handleAllianceExtensionRequests()`.
    fn handle_alliance_extension_requests(&mut self, game: &mut GameImpl) {
        for alliance in game.player(self.player()).alliances().to_vec() {
            if !game.alliance(alliance).only_one_agreed_to_extend() {
                continue;
            }
            let human = game.alliance(alliance).other(self.player());
            // A nation that is merely neutral towards the ally decides by chance.
            if game.player(self.player()).player_type() == PlayerType::Nation
                && game.player(self.player()).relation(human) == Relation::Neutral
                && !self.chance(1.5)
            {
                continue;
            }
            let human_id = game.player(human).id().to_string();
            game.add_execution(Box::new(AllianceExtensionExecution::new(
                self.player(),
                human_id,
            )));
        }
    }
}
