// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ShellExecution.ts: a shell flying from a warship or defense post to its target.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::JsNumber::round;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Game::Game::UnitParams;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::PathFinder::AirStepperEnv;
use crate::Pathfinding::PathFinderAir::AirPathFinder;
use crate::Pathfinding::PathFinderStepper::PathFinderStepper;
use crate::Pathfinding::Types::PathResult;

/// Moves a shell three tiles per tick towards its target and damages it on arrival.
pub struct ShellExecution {
    active: bool,
    path_finder: Option<AirPathFinder>,
    stepper: PathFinderStepper<TileRef>,
    shell: Option<UnitId>,
    destroy_at_tick: i64,
    random: Option<PseudoRandom>,
    spawn: TileRef,
    owner: SmallId,
    owner_unit: UnitId,
    target: UnitId,
}

impl ShellExecution {
    /// `new ShellExecution(spawn, owner, ownerUnit, target)`.
    pub fn new(spawn: TileRef, owner: SmallId, owner_unit: UnitId, target: UnitId) -> Self {
        Self {
            active: true,
            path_finder: None,
            stepper: PathFinderStepper::new(),
            shell: None,
            destroy_at_tick: -1,
            random: None,
            spawn,
            owner,
            owner_unit,
            target,
        }
    }

    fn effect_on_target(&mut self, game: &GameImpl) -> f64 {
        let base_damage = game
            .config()
            .unit_info(UnitType::Shell)
            .damage
            .unwrap_or(250.0);

        let roll = self
            .random
            .as_mut()
            .expect("initialised")
            .next_int(1.0, 6.0);
        let mut damage_multiplier = (roll - 1.0) * 25.0 + 200.0;

        // Veteran warships hit harder: scale the (integer) multiplier by the firing unit's veterancy.
        let veterancy = game.unit_any(self.owner_unit).veterancy();
        if veterancy > 0.0 {
            let bonus_percent = game.config().warship_veterancy_shell_damage_bonus();
            damage_multiplier =
                ((damage_multiplier * (100.0 + veterancy * bonus_percent)) / 100.0).floor();
        }

        round((base_damage / 250.0) * damage_multiplier)
    }
}

impl Execution for ShellExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.path_finder = Some(AirPathFinder::new_for(game.ticks(), game.config().rules()));
        self.random = Some(PseudoRandom::for_rules(
            game.config().rules(),
            game.ticks() as f64,
        ));
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let shell = match self.shell {
            Some(shell) => shell,
            None => {
                let shell = game.player_mut(self.owner).build_unit(
                    UnitType::Shell,
                    self.spawn,
                    UnitParams::default(),
                );
                self.shell = Some(shell);
                shell
            }
        };
        if !game.unit_any(shell).is_active() {
            self.active = false;
            return;
        }
        let ticks = game.ticks() as i64;
        let target = game.unit_any(self.target);
        if !target.is_active()
            || target.owner_id() == game.unit_any(shell).owner_id()
            || (self.destroy_at_tick != -1 && ticks >= self.destroy_at_tick)
        {
            game.unit_mut(shell).delete(Some(false), None);
            self.active = false;
            return;
        }

        if self.destroy_at_tick == -1 && !game.unit_any(self.owner_unit).is_active() {
            self.destroy_at_tick = ticks + game.config().shell_lifetime() as i64;
        }

        for _ in 0..3 {
            let from = game.unit_any(shell).tile();
            let to = game.unit_any(self.target).tile();
            let finder = self.path_finder.as_ref().expect("initialised");
            let result = self.stepper.next(
                &mut AirStepperEnv {
                    finder,
                    map: &game.map,
                },
                from,
                to,
                None,
            );
            match result {
                PathResult::Complete(_) => {
                    self.active = false;
                    let target_type = game.unit_any(self.target).unit_type();
                    let target_was_active = game.unit_any(self.target).is_active();
                    let effect = self.effect_on_target(game);
                    game.unit_mut(self.target)
                        .modify_health(-effect, Some(self.owner));
                    // Award veterancy to the firing warship when this shell lands the killing blow.
                    if target_was_active
                        && !game.unit_any(self.target).is_active()
                        && game.unit_any(self.owner_unit).is_active()
                        && game.unit_any(self.owner_unit).unit_type() == UnitType::Warship
                    {
                        game.unit_mut(self.owner_unit).record_kill(target_type);
                    }
                    game.unit_mut(shell).set_reached_target();
                    game.unit_mut(shell).delete(Some(false), None);
                    return;
                }
                PathResult::Next(node) => game.unit_mut(shell).move_to(node),
                PathResult::NotFound => {}
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
