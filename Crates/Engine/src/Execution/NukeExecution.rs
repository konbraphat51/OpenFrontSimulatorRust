// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/NukeExecution.ts: launches and flies an atom or hydrogen bomb.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//! STUB(owner=A4): replaced at merge.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Not-ported stand-in of `NukeExecution`.
pub struct NukeExecution;

impl NukeExecution {
    /// `new NukeExecution(nukeType, player, dst, src?, speed = -1, waitTicks = 0, rocketDirectionUp = true)`.
    #[allow(unused_variables, clippy::too_many_arguments)]
    pub fn new(
        nuke_type: UnitType,
        player: SmallId,
        dst: TileRef,
        src: Option<TileRef>,
        speed: f64,
        wait_ticks: f64,
        rocket_direction_up: bool,
    ) -> Self {
        panic!("not ported: NukeExecution")
    }
}

impl Execution for NukeExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: NukeExecution")
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: NukeExecution")
    }
    fn is_active(&self) -> bool {
        panic!("not ported: NukeExecution")
    }
    fn active_during_spawn_phase(&self) -> bool {
        panic!("not ported: NukeExecution")
    }
}
