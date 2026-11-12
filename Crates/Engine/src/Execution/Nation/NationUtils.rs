// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationUtils.ts: random territory tiles of a player.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Only `randTerritoryTileArray` (used by the N2 behaviours) is here; N1 owns the rest of the file.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::calculate_bounding_box;
use crate::Game::Game::{Cell, SmallId};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// `randTerritoryTileArray(random, mg, player, numTiles)`: up to `num_tiles` random tiles of the player.
pub fn rand_territory_tile_array(
    random: &mut PseudoRandom,
    game: &GameImpl,
    player: SmallId,
    num_tiles: usize,
) -> Vec<TileRef> {
    let bounding_box = calculate_bounding_box(&game.map, game.player(player).border_tiles().iter());
    let mut tiles = Vec::new();
    for _ in 0..num_tiles {
        if let Some(tile) = rand_territory_tile(random, game, player, bounding_box) {
            tiles.push(tile);
        }
    }
    tiles
}

/// `randTerritoryTile`: rejection sampling in the bounding box, then a random tile of small territories.
fn rand_territory_tile(
    random: &mut PseudoRandom,
    game: &GameImpl,
    player: SmallId,
    bounding_box: Option<(Cell, Cell)>,
) -> Option<TileRef> {
    match bounding_box {
        Some((min, max)) => {
            for _ in 0..100 {
                let rand_x = random.next_int(min.x as f64, max.x as f64) as i32;
                let rand_y = random.next_int(min.y as f64, max.y as f64) as i32;
                if !game.is_on_map(Cell::new(rand_x, rand_y)) {
                    continue;
                }
                let rand_tile = game.tile_ref(rand_x, rand_y);
                if game.owner_id(rand_tile) == player {
                    return Some(rand_tile);
                }
            }
        }
        None => {
            // Upstream's infinite box makes every draw NaN: two draws per attempt, none on the map.
            for _ in 0..200 {
                random.next();
            }
        }
    }
    let owned = game.player(player).num_tiles_owned();
    if owned > 0 && owned <= 100 {
        let tiles: Vec<TileRef> = game.player(player).tiles().iter().collect();
        return Some(*random.rand_element(&tiles));
    }
    None
}
