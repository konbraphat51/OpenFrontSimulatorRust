// SPDX-License-Identifier: AGPL-3.0-only
//! The slice of `Game`, `Player` and `TerraNullius` that upstream pathfinding reads, as a trait `GameImpl` implements.

use super::Types::Source;
use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Small id of a player (0 is terra nullius).
pub type SmallId = u16;

/// What spatial queries need from the game; `SmallId` 0 stands for `TerraNullius`.
pub trait PathGameView {
    /// The game map (`game.map()`); `isShore`, `isLand`, `isWater` and `ownerID` read it.
    fn map(&self) -> &GameMapImpl;

    /// `game.getWaterComponent(tile)`: the mini map water component next to a tile.
    fn water_component(&self, tile: TileRef) -> Option<u32>;

    /// `player.borderTiles()` in set order.
    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef));

    /// `player.tileChangeVersion()`: changes whenever the player's tiles change.
    fn tile_change_version(&self, player: SmallId) -> u32;

    /// `PathFinding.Water(game).findPath(from, to)` through the shared water chain (see `PathFinder::find_water_path`).
    fn find_water_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>>;
}
