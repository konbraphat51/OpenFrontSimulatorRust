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

/// Debug switch: with `OFSIM_INERT` set, not-ported executions do nothing instead of panicking.
/// Only for differential tests against the reference tracer run with `--inert 1`, which patches
/// the same executions out of the upstream engine; never use it to claim a record verifies.
pub(crate) fn inert_mode() -> bool {
    std::env::var_os("OFSIM_INERT").is_some()
}

/// An execution that does nothing (the debug `OFSIM_INERT` stand-in of a not-ported execution).
struct Inert {
    active_during_spawn_phase: bool,
    active: bool,
}

impl Execution for Inert {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}
    fn is_active(&self) -> bool {
        self.active
    }
    fn active_during_spawn_phase(&self) -> bool {
        self.active_during_spawn_phase
    }
}

impl NotPorted {
    /// Panics right away with `not ported: <name>`.
    pub fn at_creation(name: &'static str) -> Box<dyn Execution> {
        if inert_mode() {
            // Like the reference's patched `createExec`: an inactive no-op.
            return Box::new(Inert {
                active_during_spawn_phase: false,
                active: false,
            });
        }
        panic!("not ported: {name}");
    }

    /// An execution that panics with `not ported: <name>` in `init`.
    pub fn at_init(name: &'static str, active_during_spawn_phase: bool) -> Box<dyn Execution> {
        if inert_mode() {
            return Box::new(Inert {
                active_during_spawn_phase,
                active: true,
            });
        }
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
