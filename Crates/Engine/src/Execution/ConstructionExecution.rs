// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ConstructionExecution.ts: builds a structure over its construction time.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A3): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Execution::NotPorted::inert_mode;
use crate::Game::Game::{SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Not-ported stand-in of `ConstructionExecution`.
pub struct ConstructionExecution;

impl ConstructionExecution {
    /// `new ConstructionExecution(player, constructionType, tile, rocketDirectionUp?, amount?)`.
    pub fn new(
        _player: SmallId,
        _construction_type: UnitType,
        _tile: TileRef,
        _rocket_direction_up: Option<bool>,
        _amount: Option<f64>,
    ) -> Self {
        if !inert_mode() {
            panic!("not ported: ConstructionExecution");
        }
        Self
    }
}

impl Execution for ConstructionExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        false
    }
    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
