// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/MoveWarshipExecution.ts: sets a new patrol tile for the selected warships.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// One-shot execution that moves warships to a patrol tile in their own water component.
pub struct MoveWarshipExecution {
    owner: SmallId,
    unit_ids: Vec<i64>,
    position: TileRef,
}

impl MoveWarshipExecution {
    /// `new MoveWarshipExecution(owner, unitIds, position)`.
    pub fn new(owner: SmallId, unit_ids: Vec<i64>, position: TileRef) -> Self {
        Self {
            owner,
            unit_ids,
            position,
        }
    }
}

impl Execution for MoveWarshipExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.is_valid_ref(self.position) {
            return;
        }
        // Water component of the new patrol tile for the connectivity check
        let component = game
            .water_manager
            .get_water_component(&game.map, self.position);
        let warships = game.player(self.owner).units_of(UnitType::Warship);
        // Each warship is moved once, in first-seen id order
        let mut seen: Vec<i64> = Vec::new();
        for &id in &self.unit_ids {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let Some(&warship) = warships.iter().find(|&&u| u as i64 == id) else {
                continue;
            };
            let unit = game.unit_any(warship);
            if !unit.is_active() {
                continue;
            }
            // A null component never matches (upstream passes it through `!`)
            let same_component = component.is_some_and(|c| {
                game.water_manager
                    .has_water_component(&game.map, unit.tile(), c)
            });
            if !same_component {
                continue;
            }
            let mut unit = game.unit_mut(warship);
            unit.update_warship_state(false, None, Some(Some(self.position)), None);
            unit.set_target_tile(None);
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
