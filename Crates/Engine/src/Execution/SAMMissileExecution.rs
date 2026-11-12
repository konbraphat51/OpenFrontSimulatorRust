// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/SAMMissileExecution.ts: a SAM missile chasing an enemy nuke.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Legacy28::PathFinding::AirPathFinder as LegacyAirPathFinder;
use crate::Pathfinding::PathFinder::AirStepperEnv;
use crate::Pathfinding::PathFinderAir::AirPathFinder;
use crate::Pathfinding::PathFinderStepper::PathFinderStepper;
use crate::Pathfinding::Types::PathResult;

/// One SAM missile in flight.
pub struct SAMMissileExecution {
    active: bool,
    stepper: PathFinderStepper<TileRef>,
    air: AirPathFinder,
    legacy_air: Option<LegacyAirPathFinder>,
    sam_missile: Option<UnitId>,
    speed: f64,
    spawn: TileRef,
    owner: SmallId,
    owner_unit: UnitId,
    target: UnitId,
    target_tile: TileRef,
}

impl SAMMissileExecution {
    /// `new SAMMissileExecution(spawn, owner, ownerUnit, target, targetTile)`.
    pub fn new(
        spawn: TileRef,
        owner: SmallId,
        owner_unit: UnitId,
        target: UnitId,
        target_tile: TileRef,
    ) -> Self {
        Self {
            active: true,
            stepper: PathFinderStepper::new(),
            air: AirPathFinder::new(0),
            legacy_air: None,
            sam_missile: None,
            speed: 0.0,
            spawn,
            owner,
            owner_unit,
            target,
            target_tile,
        }
    }
}

impl Execution for SAMMissileExecution {
    fn init(&mut self, game: &mut GameImpl, ticks: Tick) {
        self.air = AirPathFinder::new_for(game.ticks(), game.config().rules());
        if game.config().rules().has(Change::LegacyProjectiles28) {
            self.legacy_air = Some(LegacyAirPathFinder::new(PseudoRandom::for_rules(
                game.config().rules(),
                game.ticks() as f64,
            )));
        }
        self.speed = game.config().default_sam_missile_speed();
        if game.config().rules().has(Change::SamTargetingFixes4770) {
            self.tick(game, ticks);
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let missile = match self.sam_missile {
            Some(m) => m,
            None => {
                let m = game.player_mut(self.owner).build_unit(
                    UnitType::SAMMissile,
                    self.spawn,
                    UnitParams {
                        target_unit: Some(self.target),
                        ..Default::default()
                    },
                );
                self.sam_missile = Some(m);
                m
            }
        };
        if !game.unit_any(missile).is_active() {
            self.active = false;
            return;
        }
        let (target_active, target_owner, target_type) = {
            let t = game.unit_any(self.target);
            (t.is_active(), t.owner_id(), t.unit_type)
        };
        let warheads_allowed = game.config().rules().has(Change::MirvWarheadsStandard4706);
        let nukes_whitelist = [
            UnitType::AtomBomb,
            UnitType::HydrogenBomb,
            UnitType::MIRVWarhead,
        ];
        let listed = nukes_whitelist.contains(&target_type)
            && (warheads_allowed || target_type != UnitType::MIRVWarhead);
        if !target_active
            || !game.unit_any(self.owner_unit).is_active()
            || target_owner == game.unit_any(missile).owner_id()
            || !listed
        {
            if target_active {
                game.unit_mut(self.target).set_targeted_by_sam(false);
            }
            game.unit_mut(missile).delete(Some(false), None);
            self.active = false;
            return;
        }
        let mut i = 0.0;
        while i < self.speed {
            let from = game.unit_any(missile).tile();
            let result = if let Some(air) = self.legacy_air.as_mut() {
                match air.next_tile(&game.map, from, self.target_tile) {
                    None => PathResult::Complete(from),
                    Some(t) => PathResult::Next(t),
                }
            } else {
                self.stepper.next(
                    &mut AirStepperEnv {
                        finder: &self.air,
                        map: &game.map,
                    },
                    from,
                    self.target_tile,
                    None,
                )
            };
            match result {
                PathResult::Complete(_) => {
                    self.active = false;
                    game.unit_mut(self.target)
                        .delete(Some(true), Some(self.owner));
                    game.unit_mut(missile).delete(Some(false), None);
                    return;
                }
                PathResult::Next(node) => game.unit_mut(missile).move_to(node),
                PathResult::NotFound => {}
            }
            i += 1.0;
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
