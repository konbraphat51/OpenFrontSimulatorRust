// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TransportShipUtils.ts (STUB until `SpatialQuery` lands with p2-path).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::Game::SmallId;
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use crate::Execution::NotPorted::inert_mode;

/// `canBuildTransportShip`.
pub fn can_build_transport_ship(
    _game: &GameImpl,
    _player: SmallId,
    _tile: TileRef,
) -> Option<TileRef> {
    if inert_mode() {
        return None;
    }
    panic!("not ported: TransportShipUtils::can_build_transport_ship");
}

/// `targetTransportTile`.
pub fn target_transport_tile(
    _game: &GameImpl,
    _attacker: SmallId,
    _tile: TileRef,
) -> Option<TileRef> {
    if inert_mode() {
        return None;
    }
    panic!("not ported: TransportShipUtils::target_transport_tile");
}

/// `bestShoreDeploymentSource`.
pub fn best_shore_deployment_source(
    _game: &GameImpl,
    _player: SmallId,
    _dst: TileRef,
) -> Option<TileRef> {
    panic!("not ported: TransportShipUtils::best_shore_deployment_source");
}
