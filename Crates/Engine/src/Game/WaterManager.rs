// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/WaterManager.ts.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

// STUB: replaced by p2-path
use super::Game::Tick;
use super::GameMap::{GameMapImpl, TileRef};

/// Water conversion queue and water component bookkeeping (stub).
#[derive(Debug, Default)]
pub struct WaterManager {}

impl WaterManager {
    /// Constructor with upstream's arguments (map, mini map, `disableNavMesh`).
    pub fn new(_map: &GameMapImpl, _mini_map: &GameMapImpl, _disable_nav_mesh: bool) -> Self {
        Self {}
    }

    /// `tick`: tiles whose terrain changed this tick (none in the stub).
    pub fn tick(&mut self, _ticks: Tick) -> Vec<TileRef> {
        Vec::new()
    }

    /// `queueTile`.
    pub fn queue_tile(&mut self, _tile: TileRef) {
        panic!("not ported: WaterManager::queue_tile");
    }

    /// `getWaterComponent`.
    pub fn get_water_component(&self, _tile: TileRef) -> Option<u32> {
        panic!("not ported: WaterManager::get_water_component");
    }

    /// `hasWaterComponent`.
    pub fn has_water_component(&self, _tile: TileRef, _component: u32) -> bool {
        panic!("not ported: WaterManager::has_water_component");
    }
}
