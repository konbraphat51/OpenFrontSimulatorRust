// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ConstructionExecution.ts: builds a structure over time, or hands units to their own executions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::CityExecution::CityExecution;
use crate::Execution::DefensePostExecution::DefensePostExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::FactoryExecution::FactoryExecution;
use crate::Execution::MIRVExecution::MirvExecution;
use crate::Execution::MissileSiloExecution::MissileSiloExecution;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::PortExecution::PortExecution;
use crate::Execution::SAMLauncherExecution::SAMLauncherExecution;
use crate::Execution::WarshipExecution::WarshipExecution;
use crate::Game::Game::{Gold, SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Places a structure under construction and finishes it after the unit type's construction duration.
pub struct ConstructionExecution {
    structure: Option<UnitId>,
    active: bool,
    ticks_until_complete: i64,
    player: SmallId,
    construction_type: UnitType,
    tile: TileRef,
    rocket_direction_up: Option<bool>,
    amount: Option<u32>,
    /// The `Construction` unit standing in for the structure (before PR 2378).
    construction: Option<UnitId>,
    /// Gold taken at the start and refunded when the structure is placed (before PR 2378).
    cost: Gold,
}

impl ConstructionExecution {
    /// `new ConstructionExecution(player, constructionType, tile, rocketDirectionUp, amount)`.
    pub fn new(
        player: SmallId,
        construction_type: UnitType,
        tile: TileRef,
        rocket_direction_up: Option<bool>,
        amount: Option<u32>,
    ) -> Self {
        Self {
            structure: None,
            active: true,
            ticks_until_complete: 0,
            player,
            construction_type,
            tile,
            rocket_direction_up,
            amount,
            construction: None,
            cost: 0,
        }
    }

    /// `tick` before PR 2378: a `Construction` unit holds the gold and the place.
    fn tick_construction_unit(&mut self, game: &mut GameImpl) {
        let Some(construction) = self.construction else {
            let info = game.config().unit_info(self.construction_type);
            let Some(duration) = info.construction_duration else {
                self.complete_construction(game);
                self.active = false;
                return;
            };
            let Some(spawn_tile) =
                game.player(self.player)
                    .can_build(self.construction_type, self.tile, None)
            else {
                self.active = false;
                return;
            };
            let construction = game.player_mut(self.player).build_unit(
                UnitType::Construction,
                spawn_tile,
                UnitParams::default(),
            );
            self.construction = Some(construction);
            self.cost = game
                .player(self.player)
                .unit_cost(self.construction_type, 0);
            game.player_mut(self.player).remove_gold(self.cost);
            game.unit_mut(construction)
                .set_construction_type(self.construction_type);
            self.ticks_until_complete = duration as i64;
            return;
        };

        if !game.unit_any(construction).is_active() {
            self.active = false;
            return;
        }
        let owner = game.unit_any(construction).owner_id();
        if self.player != owner {
            self.player = owner;
        }
        if self.ticks_until_complete == 0 {
            self.player = owner;
            game.unit_mut(construction).delete(Some(false), None);
            // Refund the cost so the player has the gold to build the structure.
            game.player_mut(self.player).add_gold(self.cost);
            self.complete_construction(game);
            self.active = false;
            return;
        }
        self.ticks_until_complete -= 1;
    }

    fn complete_construction(&mut self, game: &mut GameImpl) {
        if !game.has(Change::NoConstructionUnit2378) && Self::is_structure(self.construction_type) {
            self.complete_structure_unitless(game);
            return;
        }
        if let Some(structure) = self.structure {
            game.unit_mut(structure).set_under_construction(false);
        }
        let player = self.player;
        match self.construction_type {
            UnitType::AtomBomb | UnitType::HydrogenBomb => {
                let count = self.amount.unwrap_or(1);
                for _ in 0..count {
                    // NukeExecution staggers same-tick launches per silo itself.
                    game.add_execution(Box::new(NukeExecution::new(
                        self.construction_type,
                        player,
                        self.tile,
                        None,
                        -1.0,
                        0.0,
                        self.rocket_direction_up,
                    )));
                }
            }
            UnitType::MIRV => game.add_execution(Box::new(MirvExecution::new(player, self.tile))),
            UnitType::Warship => {
                game.add_execution(Box::new(WarshipExecution::new(player, self.tile)))
            }
            UnitType::Port => game.add_execution(Box::new(PortExecution::new(self.structure()))),
            UnitType::MissileSilo => {
                game.add_execution(Box::new(MissileSiloExecution::new(self.structure())))
            }
            UnitType::DefensePost => {
                game.add_execution(Box::new(DefensePostExecution::new(self.structure())))
            }
            UnitType::SAMLauncher => game.add_execution(Box::new(SAMLauncherExecution::new(
                player,
                None,
                Some(self.structure()),
            ))),
            UnitType::City => game.add_execution(Box::new(CityExecution::new(self.structure()))),
            UnitType::Factory => {
                game.add_execution(Box::new(FactoryExecution::new(self.structure())))
            }
            _ => {}
        }
    }

    /// `completeConstruction` before PR 2378: structures build themselves from owner and tile.
    fn complete_structure_unitless(&mut self, game: &mut GameImpl) {
        let player = self.player;
        match self.construction_type {
            UnitType::Port => {
                game.add_execution(Box::new(PortExecution::from_tile(player, self.tile)))
            }
            UnitType::MissileSilo => {
                game.add_execution(Box::new(MissileSiloExecution::from_tile(player, self.tile)))
            }
            UnitType::DefensePost => {
                game.add_execution(Box::new(DefensePostExecution::from_tile(player, self.tile)))
            }
            UnitType::SAMLauncher => game.add_execution(Box::new(SAMLauncherExecution::new(
                player,
                Some(self.tile),
                None,
            ))),
            UnitType::City => {
                game.add_execution(Box::new(CityExecution::from_tile(player, self.tile)))
            }
            UnitType::Factory => {
                game.add_execution(Box::new(FactoryExecution::from_tile(player, self.tile)))
            }
            _ => {}
        }
    }

    fn structure(&self) -> UnitId {
        self.structure.expect("structure was built")
    }

    fn is_structure(unit_type: UnitType) -> bool {
        matches!(
            unit_type,
            UnitType::Port
                | UnitType::MissileSilo
                | UnitType::DefensePost
                | UnitType::SAMLauncher
                | UnitType::City
                | UnitType::Factory
        )
    }
}

impl Execution for ConstructionExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if game.config().is_unit_disabled(self.construction_type) {
            self.active = false;
            return;
        }

        if !game.is_valid_ref(self.tile) {
            self.active = false;
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.has(Change::NoConstructionUnit2378) {
            self.tick_construction_unit(game);
            return;
        }
        let Some(structure) = self.structure else {
            let info = game.config().unit_info(self.construction_type);
            // For non-structure units (nukes/warship), charge once and delegate to specialized executions.
            if !Self::is_structure(self.construction_type) {
                // Defer validation and gold deduction to the specific execution
                self.complete_construction(game);
                self.active = false;
                return;
            }

            // Structures: build real unit and mark under construction
            let Some(spawn_tile) =
                game.player(self.player)
                    .can_build(self.construction_type, self.tile, None)
            else {
                self.active = false;
                return;
            };
            let structure = game.player_mut(self.player).build_unit(
                self.construction_type,
                spawn_tile,
                UnitParams::default(),
            );
            self.structure = Some(structure);
            let duration = info.construction_duration.unwrap_or(0);
            if duration > 0 {
                game.unit_mut(structure).set_under_construction(true);
                self.ticks_until_complete = duration as i64;
                return;
            }
            // No construction time
            self.complete_construction(game);
            self.active = false;
            return;
        };

        if !game.unit_any(structure).is_active() {
            self.active = false;
            return;
        }

        let owner = game.unit_any(structure).owner_id();
        if self.player != owner {
            self.player = owner;
        }

        if self.ticks_until_complete == 0 {
            self.player = game.unit_any(structure).owner_id();
            self.complete_construction(game);
            self.active = false;
            return;
        }
        self.ticks_until_complete -= 1;
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
