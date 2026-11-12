// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TribeExecution.ts: the AI of a tribe (bot) player.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::{Change, Rules};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::DeleteUnitExecution::DeleteUnitExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::Utils::AiAttackBehavior::AiAttackBehavior;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// Periodically expands, retaliates and accepts alliances for one tribe.
pub struct TribeExecution {
    active: bool,
    random: Rc<RefCell<PseudoRandom>>,
    neighbors_terra_nullius: bool,
    attack_behavior: Option<AiAttackBehavior>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    tribe: SmallId,
}

impl TribeExecution {
    /// `new TribeExecution(tribe)`; `tribe_id` is the player's string id (the random seed).
    pub fn new(tribe: SmallId, tribe_id: &str, rules: &Rules) -> Self {
        let mut random = PseudoRandom::for_rules(rules, simple_hash(tribe_id) as f64);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(50.0, 60.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 40.0) / 100.0;
        let expand_ratio = random.next_int(10.0, 20.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            neighbors_terra_nullius: true,
            attack_behavior: None,
            attack_rate,
            attack_tick,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            tribe,
        }
    }

    fn accept_all_alliance_requests(&self, game: &mut GameImpl) {
        for req in game.player(self.tribe).incoming_alliance_requests() {
            game.accept_request(req);
        }
        // Extension requests: only a human ally can ask to renew.
        for alliance in game.player(self.tribe).alliances().to_vec() {
            if !game.alliance(alliance).only_one_agreed_to_extend() {
                continue;
            }
            let human = game.alliance(alliance).other(self.tribe);
            let human_id = game.player(human).id().to_string();
            game.add_execution(Box::new(AllianceExtensionExecution::new(
                self.tribe, human_id,
            )));
        }
    }

    fn delete_next_structure(&self, game: &mut GameImpl) {
        if !game.config().rules().has(Change::BotStructureFarming2895) {
            return;
        }
        if !game.player(self.tribe).can_delete_unit() {
            return;
        }
        if AiAttackBehavior::legacy31(game) {
            // Before v0.31 every structure is queued for deletion at once.
            for unit in game.player(self.tribe).units().to_vec() {
                if game.unit_any(unit).unit_type().is_structure() {
                    game.add_execution(Box::new(DeleteUnitExecution::new(self.tribe, unit)));
                }
            }
            return;
        }
        for unit in game.player(self.tribe).units().to_vec() {
            let u = game.unit_any(unit);
            if !u.unit_type().is_structure() {
                continue;
            }
            if u.is_marked_for_deletion() {
                continue;
            }
            game.add_execution(Box::new(DeleteUnitExecution::new(self.tribe, unit)));
            return;
        }
    }

    fn maybe_attack(&mut self, game: &mut GameImpl) {
        let behavior = self.attack_behavior.as_mut().expect("not initialized");
        if let Some(to_attack) = behavior.get_neighbor_traitor_to_attack(
            game,
            game.config().rules().has(Change::NationAiSplit6087),
        ) {
            let odds = if game.player(self.tribe).is_friendly(to_attack, false) {
                6.0
            } else {
                3.0
            };
            if self.random.borrow_mut().chance(odds) {
                // Break the alliance before attacking if needed.
                let alliance = game.player(self.tribe).alliance_with(to_attack);
                if let Some(alliance) = alliance {
                    game.player_mut(self.tribe).break_alliance(alliance);
                }
                let sent = behavior.send_attack(game, to_attack, false);
                if AiAttackBehavior::sent(game, sent) {
                    return;
                }
            }
        }

        if self.neighbors_terra_nullius {
            let borders_free_land = if game.config().rules().has(Change::BotTerraNulliusLand2896) {
                game.player(self.tribe).nearby().contains(&0)
            } else {
                game.player(self.tribe).shares_border_with(0)
            };
            if borders_free_land {
                let sent = behavior.send_attack(game, 0, false);
                if AiAttackBehavior::sent(game, sent) {
                    return;
                }
            } else {
                self.neighbors_terra_nullius = false;
            }
        }

        behavior.attack_random_target(game);
    }
}

impl Execution for TribeExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        if ticks as f64 % self.attack_rate != self.attack_tick {
            return;
        }
        if !game.player(self.tribe).is_alive() {
            // `removeOnDeath` is called from the tribe's PlayerExecution.
            self.active = false;
            return;
        }
        if self.attack_behavior.is_none() {
            let mut behavior = AiAttackBehavior::new(
                self.random.clone(),
                self.tribe,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
                None,
                None,
            );
            // Send an attack on the first tick.
            behavior.send_attack(game, 0, false);
            self.attack_behavior = Some(behavior);
            return;
        }
        // The tutorial's tribe only grows into free land and accepts no alliance.
        if game.config().rules().has(Change::TutorialMode6204) && game.config().tutorial() {
            self.attack_behavior
                .as_mut()
                .expect("set above")
                .expand_into_free_land(game);
            return;
        }
        self.accept_all_alliance_requests(game);
        self.delete_next_structure(game);
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
