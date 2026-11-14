// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/BotExecution.ts of v0.26.0-beta1/2: the AI of a tribe (bot) player on `BotBehavior`.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Rules;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::Utils::BotBehaviorShared::BotBehaviorAlliances;
use crate::Execution::Utils::BotBehaviorV26Beta::BotBehaviorV26Beta;
use crate::Game::Game::{SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use std::cell::RefCell;
use std::rc::Rc;

/// Periodically expands, retaliates and accepts alliances for one tribe.
pub struct BotExecutionV26Beta {
    active: bool,
    random: SharedRandom,
    neighbors_terra_nullius: bool,
    behavior: Option<BotBehaviorV26Beta>,
    attack_rate: f64,
    attack_tick: f64,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    bot: SmallId,
}

impl BotExecutionV26Beta {
    /// `new BotExecution(bot)`; `bot_id` is the player's string id (the random seed).
    pub fn new(bot: SmallId, bot_id: &str, rules: &Rules) -> Self {
        let mut random = PseudoRandom::for_rules(rules, simple_hash(bot_id) as f64);
        let attack_rate = random.next_int(40.0, 80.0);
        let attack_tick = random.next_int(0.0, attack_rate);
        let trigger_ratio = random.next_int(60.0, 90.0) / 100.0;
        let reserve_ratio = random.next_int(20.0, 30.0) / 100.0;
        let expand_ratio = random.next_int(10.0, 20.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            neighbors_terra_nullius: true,
            behavior: None,
            attack_rate,
            attack_tick,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            bot,
        }
    }

    fn maybe_attack(&mut self, game: &mut GameImpl) {
        let behavior = self.behavior.as_mut().expect("not initialized");
        if let Some(to_attack) = behavior.get_neighbor_traitor_to_attack(game) {
            let odds = if game.player(self.bot).is_friendly(to_attack, false) {
                6.0
            } else {
                3.0
            };
            if self.random.borrow_mut().chance(odds) {
                // Check and break the alliance before attacking if needed
                if let Some(alliance) = game.player(self.bot).alliance_with(to_attack) {
                    game.player_mut(self.bot).break_alliance(alliance);
                }
                behavior.send_attack(game, to_attack);
                return;
            }
        }

        if self.neighbors_terra_nullius {
            if game.player(self.bot).shares_border_with(0) {
                behavior.send_attack(game, 0);
                return;
            }
            self.neighbors_terra_nullius = false;
        }

        behavior.forget_old_enemies(game);
        let Some(enemy) = behavior.select_random_enemy(game) else {
            return;
        };
        if !game.player(self.bot).shares_border_with(enemy) {
            return;
        }
        behavior.send_attack(game, enemy);
    }
}

impl Execution for BotExecutionV26Beta {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        if ticks as f64 % self.attack_rate != self.attack_tick {
            return;
        }
        if !game.player(self.bot).is_alive() {
            self.active = false;
            return;
        }
        if self.behavior.is_none() {
            let mut behavior = BotBehaviorV26Beta::new(
                self.random.clone(),
                self.bot,
                self.trigger_ratio,
                self.reserve_ratio,
                self.expand_ratio,
            );
            // Send an attack on the first tick
            behavior.send_attack(game, 0);
            self.behavior = Some(behavior);
            return;
        }
        if let Some(behavior) = self.behavior.as_mut() {
            behavior.handle_alliance_requests(game);
            behavior.handle_alliance_extension_requests(game);
        }
        self.maybe_attack(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
