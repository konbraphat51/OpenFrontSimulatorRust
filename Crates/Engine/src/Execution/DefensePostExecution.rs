// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DefensePostExecution.ts: a defense post (its ship targeting is disabled upstream).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream never assigns `target` (the targeting block is commented out), so `shoot` and the shell
//! bookkeeping are unreachable and not ported; the execution only tracks the post's lifetime.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// Lives while the post does; does nothing while it is under construction.
pub struct DefensePostExecution {
    active: bool,
    post: UnitId,
    /// Owner and tile of a structure that is built on the first tick (before PR 2378).
    pending: Option<(SmallId, TileRef)>,
}

impl DefensePostExecution {
    /// `new DefensePostExecution(post)`.
    pub fn new(post: UnitId) -> Self {
        Self {
            active: true,
            post,
            pending: None,
        }
    }

    /// `new DefensePostExecution(player, tile)`: the structure is built on the first tick.
    pub fn from_tile(player: SmallId, tile: TileRef) -> Self {
        let mut e = Self::new(0);
        e.pending = Some((player, tile));
        e
    }
}

impl Execution for DefensePostExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if let Some((player, tile)) = self.pending.take() {
            let Some(spawn) = game
                .player(player)
                .can_build(UnitType::DefensePost, tile, None)
            else {
                self.active = false;
                return;
            };
            self.post = game.player_mut(player).build_unit(
                UnitType::DefensePost,
                spawn,
                UnitParams::default(),
            );
        }
        if !game.unit_any(self.post).is_active() {
            self.active = false;
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
