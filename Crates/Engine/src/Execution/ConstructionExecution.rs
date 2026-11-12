// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ConstructionExecution.ts: builds a structure over its construction time.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A3): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Not-ported stand-in of `ConstructionExecution`.
pub struct ConstructionExecution;

impl ConstructionExecution {
    /// `new ConstructionExecution(player, constructionType, tile, rocketDirectionUp?, amount?)`.
    #[allow(unused_variables, clippy::too_many_arguments)]
    pub fn new(
        player: SmallId,
        construction_type: UnitType,
        tile: TileRef,
        rocket_direction_up: Option<bool>,
        amount: Option<f64>,
    ) -> Self {
        panic!("not ported: ConstructionExecution")
    }
}

impl Execution for ConstructionExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: ConstructionExecution")
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: ConstructionExecution")
    }
    fn is_active(&self) -> bool {
        panic!("not ported: ConstructionExecution")
    }
    fn active_during_spawn_phase(&self) -> bool {
        panic!("not ported: ConstructionExecution")
    }
}
