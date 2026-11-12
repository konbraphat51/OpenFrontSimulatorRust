// SPDX-License-Identifier: AGPL-3.0-only
//! Placeholder for executions that are not ported yet: never silently does nothing.

use crate::Execution::Execution::Execution;
use crate::Game::Game::Tick;
use crate::Game::GameImpl::GameImpl;

/// An execution that panics with `not ported: <name>` instead of simulating anything.
///
/// [`NotPorted::at_creation`] panics immediately (use it where upstream creates the execution
/// from an intent or at runner init). [`NotPorted::at_init`] is for executions that are created
/// early but only become active later (for example `TribeExecution`, dormant during the spawn
/// phase): it panics when the game first initialises it.
pub struct NotPorted {
    name: &'static str,
    active_during_spawn_phase: bool,
}

impl NotPorted {
    /// Panics right away with `not ported: <name>`.
    pub fn at_creation(name: &'static str) -> Box<dyn Execution> {
        panic!("not ported: {name}");
    }

    /// An execution that panics with `not ported: <name>` in `init`.
    pub fn at_init(name: &'static str, active_during_spawn_phase: bool) -> Box<dyn Execution> {
        Box::new(Self {
            name,
            active_during_spawn_phase,
        })
    }
}

impl Execution for NotPorted {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: {}", self.name);
    }
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {
        panic!("not ported: {}", self.name);
    }
    fn is_active(&self) -> bool {
        true
    }
    fn active_during_spawn_phase(&self) -> bool {
        self.active_during_spawn_phase
    }
}
