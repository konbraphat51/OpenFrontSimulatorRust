// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the `Execution` interface of src/core/game/Game.ts.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Game::Game::Tick;
use crate::Game::GameImpl::GameImpl;

/// A unit of simulation work that lives in the game's execution list.
///
/// `GameImpl::execute_next_tick` takes the list out while ticking it, so an execution may freely
/// borrow the game mutably; executions added meanwhile go to the game's uninitialised list.
pub trait Execution {
    /// Called once, on the first tick the execution is allowed to run.
    fn init(&mut self, game: &mut GameImpl, ticks: Tick);
    /// Called every tick while active.
    fn tick(&mut self, game: &mut GameImpl, ticks: Tick);
    /// True while the execution wants to be ticked; inactive executions are dropped.
    fn is_active(&self) -> bool;
    /// Whether the execution runs during the spawn phase.
    fn active_during_spawn_phase(&self) -> bool;
}
