// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationUtils.ts: helpers shared by the nation behaviours.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// `randTerritoryTileArray(random, mg, player, numTiles)`.
pub fn rand_territory_tile_array(
    _random: &mut PseudoRandom,
    _game: &GameImpl,
    _player: SmallId,
    _num_tiles: usize,
) -> Vec<TileRef> {
    panic!("not ported: NationUtils");
}

/// `findJuiciestTarget(game, candidates)`.
pub fn find_juiciest_target(_game: &GameImpl, _candidates: &[SmallId]) -> Option<SmallId> {
    panic!("not ported: NationUtils");
}
