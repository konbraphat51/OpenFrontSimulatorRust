// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/UpgradeStructureExecution.ts: upgrades a structure up to `amount` levels at init.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId};
use crate::Game::GameImpl::GameImpl;

/// One-shot execution: upgrades as many times as the player can afford, then ends.
pub struct UpgradeStructureExecution {
    player: SmallId,
    unit_id: UnitId,
    amount: u32,
}

impl UpgradeStructureExecution {
    /// `new UpgradeStructureExecution(player, unitId, amount = 1)`.
    pub fn new(player: SmallId, unit_id: UnitId, amount: Option<u32>) -> Self {
        Self {
            player,
            unit_id,
            amount: amount.unwrap_or(1),
        }
    }
}

impl Execution for UpgradeStructureExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let Some(structure) = game.unit(self.unit_id).map(|u| u.id()) else {
            return;
        };
        if game.unit_any(structure).owner_id() != self.player {
            return;
        }

        for _ in 0..self.amount {
            if !game.player(self.player).can_upgrade_unit(structure) {
                break;
            }
            game.player_mut(self.player).upgrade_unit(structure);
        }
    }

    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn is_active(&self) -> bool {
        false
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
