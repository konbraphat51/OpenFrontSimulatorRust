// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/DeleteUnitExecution.ts: marks a unit for deletion, deletes it later.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// Voluntary deletion of one of the player's units on their own land.
pub struct DeleteUnitExecution {
    active: bool,
    unit: Option<UnitId>,
    player: SmallId,
    unit_id: UnitId,
}

impl DeleteUnitExecution {
    /// `new DeleteUnitExecution(player, unitId)`.
    pub fn new(player: SmallId, unit_id: UnitId) -> Self {
        Self {
            active: true,
            unit: None,
            player,
            unit_id,
        }
    }
}

impl Execution for DeleteUnitExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !self.active {
            return;
        }
        let Some(unit) = game.unit(self.unit_id) else {
            self.active = false;
            return;
        };
        if unit.owner_id() != self.player {
            self.active = false;
            return;
        }
        if !unit.is_active() {
            self.active = false;
            return;
        }
        self.unit = Some(self.unit_id);
        let tile = unit.tile();
        let tile_owner = game.owner_id(tile);
        if tile_owner == 0 || tile_owner != self.player {
            self.active = false;
            return;
        }
        if !game.is_land(tile) {
            self.active = false;
            return;
        }
        if game.in_spawn_phase() {
            self.active = false;
            return;
        }
        if !game.player(self.player).can_delete_unit() {
            self.active = false;
            return;
        }
        game.player_mut(self.player).record_delete_unit();
        game.unit_mut(self.unit_id).mark_for_deletion();
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        let Some(unit) = self.unit else {
            return;
        };
        if !self.active {
            return;
        }
        if !game.unit_any(unit).is_active() {
            self.active = false;
            return;
        }
        if game.unit_any(unit).is_overdue_deletion(ticks) {
            game.unit_mut(unit).delete(Some(false), None);
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
