// SPDX-License-Identifier: AGPL-3.0-only
//! The `PathGameView` and `SharedWaterView` implementations of `GameImpl` (what pathfinding and the nation water cache read).

use super::Game::{PlayerType, SmallId};
use super::GameImpl::GameImpl;
use super::GameMap::{GameMapImpl, TileRef};
use crate::Execution::Nation::SharedWaterCache::SharedWaterView;
use crate::Pathfinding::PathGameView::PathGameView;
use crate::Pathfinding::Types::Source;

impl PathGameView for GameImpl {
    fn map(&self) -> &GameMapImpl {
        &self.map
    }

    fn water_component(&self, tile: TileRef) -> Option<u32> {
        self.water_manager.get_water_component(&self.map, tile)
    }

    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
        self.player_impl(player).border_tiles().for_each(f);
    }

    fn tile_change_version(&self, player: SmallId) -> u32 {
        self.player_impl(player).tile_change_version()
    }

    fn find_water_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.water_manager.find_water_path(&self.map, from, to)
    }
}

/// Read-only game handle for `&GameImpl` call sites (`canBuild`): the water chains use interior mutability.
impl PathGameView for &GameImpl {
    fn map(&self) -> &GameMapImpl {
        &self.map
    }

    fn water_component(&self, tile: TileRef) -> Option<u32> {
        PathGameView::water_component(*self, tile)
    }

    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
        PathGameView::for_each_border_tile(*self, player, f);
    }

    fn tile_change_version(&self, player: SmallId) -> u32 {
        PathGameView::tile_change_version(*self, player)
    }

    fn find_water_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        self.water_manager.find_water_path(&self.map, from, to)
    }
}

impl SharedWaterView for GameImpl {
    fn map(&self) -> &GameMapImpl {
        &self.map
    }

    fn players(&self) -> Vec<SmallId> {
        GameImpl::players(self)
    }

    fn is_bot(&self, player: SmallId) -> bool {
        self.player_impl(player).player_type() == PlayerType::Bot
    }

    fn tile_change_version(&self, player: SmallId) -> u32 {
        self.player_impl(player).tile_change_version()
    }

    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
        self.player_impl(player).border_tiles().for_each(f);
    }

    fn can_trade(&self, player: SmallId, other: SmallId) -> bool {
        self.player(player).can_trade(other)
    }

    fn water_component(&self, tile: TileRef) -> Option<u32> {
        self.water_manager.get_water_component(&self.map, tile)
    }
}
