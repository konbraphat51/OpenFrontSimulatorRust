// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TransportShipUtils.ts of v0.28.4: shore selection by Manhattan distance and `MiniAStar`.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Game::{SmallId, UnitType};
use super::GameImpl::GameImpl;
use super::GameMap::{GameMapImpl, TileRef, and_fn, manhattan_dist_fn};
use crate::Pathfinding::Legacy28::MiniAStar::MiniAStar;
use crate::Pathfinding::Legacy28::SerialAStar::PathFindResultType;

fn is_lake(map: &GameMapImpl, tile: TileRef) -> bool {
    !map.is_land(tile) && !map.is_ocean(tile)
}

/// `canBuildTransportShip`: the shore tile a ship would start from, or `None`.
pub fn can_build_transport_ship(
    game: &GameImpl,
    player: SmallId,
    tile: TileRef,
) -> Option<TileRef> {
    if game.player(player).unit_count(UnitType::TransportShip)
        >= game.config().boat_max_number() as i64
    {
        return None;
    }
    let dst = target_transport_tile(game, tile)?;
    let other = game.owner_id(tile);
    if other == player {
        return None;
    }
    if other != 0 && game.player(player).is_friendly(other, false) {
        return None;
    }

    let map = &game.map;
    if map.is_ocean_shore(dst) {
        let my_player_borders_ocean = game
            .player(player)
            .border_tiles()
            .iter()
            .any(|bt| map.is_ocean_shore(bt));
        let other_player_borders_ocean = if !game.has_owner(tile) {
            true
        } else {
            game.player(other)
                .border_tiles()
                .iter()
                .any(|bt| map.is_ocean_shore(bt))
        };
        return if my_player_borders_ocean && other_player_borders_ocean {
            transport_ship_spawn(game, player, dst)
        } else {
            None
        };
    }

    // Boating in a lake: search from the target for a border tile owned by the player
    let tiles = map.bfs(
        dst,
        and_fn(
            manhattan_dist_fn(dst, 300.0, false),
            Box::new(|gm: &GameMapImpl, t| is_lake(gm, t) || gm.is_shore(t)),
        ),
    );
    let mut sorted: Vec<TileRef> = tiles.iter().copied().collect();
    sorted.sort_by_key(|&t| map.manhattan_dist(dst, t));
    for t in sorted {
        if game.owner_id(t) == player {
            return transport_ship_spawn(game, player, t);
        }
    }
    None
}

fn transport_ship_spawn(game: &GameImpl, player: SmallId, target: TileRef) -> Option<TileRef> {
    if !game.map.is_shore(target) {
        return None;
    }
    closest_shore_from_player(game, player, target)
}

/// `targetTransportTile`: the closest shore of the tile's owner (or of free land within 50 tiles).
pub fn target_transport_tile(game: &GameImpl, tile: TileRef) -> Option<TileRef> {
    let dst = game.owner_id(tile);
    if dst != 0 {
        closest_shore_from_player(game, dst, tile)
    } else {
        closest_shore_tn(&game.map, tile, 50.0)
    }
}

/// `closestShoreFromPlayer`: the player's border shore tile closest to `target` (first one wins ties).
pub fn closest_shore_from_player(
    game: &GameImpl,
    player: SmallId,
    target: TileRef,
) -> Option<TileRef> {
    let map = &game.map;
    let mut closest: Option<TileRef> = None;
    for t in game.player(player).border_tiles().iter() {
        if !map.is_shore(t) {
            continue;
        }
        closest = match closest {
            None => Some(t),
            Some(c) => Some(
                if map.manhattan_dist(target, t) < map.manhattan_dist(target, c) {
                    t
                } else {
                    c
                },
            ),
        };
    }
    closest
}

/// `bestShoreDeploymentSource`: the attacker's shore tile from which a path to the target's shore starts.
pub fn best_shore_deployment_source(
    game: &GameImpl,
    player: SmallId,
    target: TileRef,
) -> Option<TileRef> {
    let t = target_transport_tile(game, target)?;
    let candidates = candidate_shore_tiles(game, player, t);
    if candidates.is_empty() {
        return None;
    }
    let map = &game.map;
    let mini = game.water_manager.mini_map();
    let mut a_star = MiniAStar::new(
        map,
        mini,
        &candidates,
        true,
        t,
        1_000_000.0,
        1.0,
        true,
        0.0,
        true,
    );
    if a_star.compute(mini) != PathFindResultType::Completed {
        return None;
    }
    let path = a_star.reconstruct_path(map, mini);
    let potential = *path.first()?;
    // Mini A* downscales the map, so the neighbours of the tile hold the real deployment point
    map.neighbors(potential)
        .into_iter()
        .find(|&n| map.is_shore(n) && game.owner_id(n) == player)
}

/// `candidateShoreTiles`: the closest, the extreme and a sample of the player's border shore tiles.
pub fn candidate_shore_tiles(game: &GameImpl, player: SmallId, target: TileRef) -> Vec<TileRef> {
    let map = &game.map;
    let mut closest_manhattan_distance = f64::INFINITY;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    );
    let mut best_by_manhattan: Option<TileRef> = None;
    let (mut ex_min_x, mut ex_min_y, mut ex_max_x, mut ex_max_y): (
        Option<TileRef>,
        Option<TileRef>,
        Option<TileRef>,
        Option<TileRef>,
    ) = (None, None, None, None);

    let border_shore_tiles: Vec<TileRef> = game
        .player(player)
        .border_tiles()
        .iter()
        .filter(|&t| map.is_shore(t))
        .collect();

    for &tile in &border_shore_tiles {
        let distance = map.manhattan_dist(tile, target) as f64;
        let (cx, cy) = (map.x(tile) as f64, map.y(tile) as f64);
        if distance < closest_manhattan_distance {
            closest_manhattan_distance = distance;
            best_by_manhattan = Some(tile);
        }
        if cx < min_x {
            min_x = cx;
            ex_min_x = Some(tile);
        } else if cy < min_y {
            min_y = cy;
            ex_min_y = Some(tile);
        } else if cx > max_x {
            max_x = cx;
            ex_max_x = Some(tile);
        } else if cy > max_y {
            max_y = cy;
            ex_max_y = Some(tile);
        }
    }

    // At most about 50 sampled tiles
    let sampling_interval = usize::max(10, border_shore_tiles.len().div_ceil(50));
    let sampled = border_shore_tiles
        .iter()
        .enumerate()
        .filter(|(i, _)| i % sampling_interval == 0)
        .map(|(_, &t)| t);

    // `.filter(Boolean)` drops null and tile 0
    [best_by_manhattan, ex_min_x, ex_min_y, ex_max_x, ex_max_y]
        .into_iter()
        .flatten()
        .chain(sampled)
        .filter(|&t| t != 0)
        .collect()
}

fn closest_shore_tn(map: &GameMapImpl, tile: TileRef, search_dist: f64) -> Option<TileRef> {
    let tn = map.bfs(
        tile,
        and_fn(
            Box::new(|gm: &GameMapImpl, t| !gm.has_owner(t)),
            manhattan_dist_fn(tile, search_dist, false),
        ),
    );
    let mut shores: Vec<TileRef> = tn.iter().copied().filter(|&t| map.is_shore(t)).collect();
    shores.sort_by_key(|&t| map.manhattan_dist(tile, t));
    shores.first().copied()
}
