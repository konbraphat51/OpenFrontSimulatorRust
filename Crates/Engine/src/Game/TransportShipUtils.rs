// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TransportShipUtils.ts: where a transport ship may land and where it may leave from.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Game::{SmallId, UnitType};
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use crate::Pathfinding::Spatial::SpatialQuery::DEFAULT_MAX_DIST;

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

    let dst = target_transport_tile(game, player, tile)?;

    let other = game.owner_id(tile);
    if other == player {
        return None;
    }
    if other != 0 && !game.player(player).can_attack_player(other, false) {
        return None;
    }

    game.spatial
        .borrow_mut()
        .closest_shore_by_water(&mut &*game, player, dst)
}

/// `targetTransportTile`: the landing shore the attacker can reach by water.
pub fn target_transport_tile(game: &GameImpl, attacker: SmallId, tile: TileRef) -> Option<TileRef> {
    // Only reachable shores count, so a shore facing a disconnected inland lake is never chosen
    game.spatial.borrow_mut().closest_reachable_shore(
        &game,
        game.owner_id(tile),
        attacker,
        tile,
        DEFAULT_MAX_DIST,
    )
}

/// `bestShoreDeploymentSource`: the attacker's shore tile closest to `dst` by water.
pub fn best_shore_deployment_source(
    game: &GameImpl,
    player: SmallId,
    dst: TileRef,
) -> Option<TileRef> {
    game.spatial
        .borrow_mut()
        .closest_shore_by_water(&mut &*game, player, dst)
}
