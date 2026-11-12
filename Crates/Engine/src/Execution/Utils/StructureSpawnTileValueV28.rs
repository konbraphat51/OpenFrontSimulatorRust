// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/structureSpawnTileValue.ts of v0.28.4: how a fake human scores a structure site.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::OrderedSet::OrderedSet;
use crate::Execution::Util::{closest_tile, closest_two_tiles};
use crate::Game::Game::{Relation, SmallId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// The scoring function of one structure type for one player.
pub type TileValue = Box<dyn Fn(&GameImpl, TileRef) -> f64>;

/// `structureSpawnTileValue(mg, player, type)`; panics for types without a value function, like upstream.
pub fn structure_spawn_tile_value(
    game: &GameImpl,
    player: SmallId,
    unit_type: UnitType,
) -> TileValue {
    let border_tiles: Vec<TileRef> = game.player(player).border_tiles().iter().collect();
    let other_units: Vec<TileRef> = game
        .player(player)
        .units_of(unit_type)
        .into_iter()
        .map(|u| game.unit_any(u).tile())
        .collect();
    // Prefer spacing structures out of atom bomb range
    let border_spacing = game.config().nuke_magnitudes(UnitType::AtomBomb).outer;
    let structure_spacing = border_spacing * 2.0;

    // `otherTiles` without the evaluated tile, in unit order (a JS `Set`: duplicates collapse)
    let others_without = move |tile: TileRef| -> Vec<TileRef> {
        let mut set: OrderedSet<TileRef> = OrderedSet::new();
        for &t in &other_units {
            set.insert(t);
        }
        set.remove(&tile);
        set.iter().copied().collect()
    };

    match unit_type {
        UnitType::City | UnitType::Factory | UnitType::MissileSilo => Box::new(move |mg, tile| {
            let mut w = 0.0;
            // Prefer higher elevations
            w += mg.map.magnitude(tile) as f64;
            // Prefer to be away from the border
            let (_, closest_border_dist) =
                closest_tile(&mg.map, border_tiles.iter().copied(), tile);
            w += f64::min(closest_border_dist, border_spacing);
            // Prefer to be away from other structures of the same type
            if let Some((closest_other, _)) =
                closest_two_tiles(&mg.map, others_without(tile), [tile])
            {
                let d = mg.map.manhattan_dist(closest_other, tile) as f64;
                w += f64::min(d, structure_spacing);
            }
            w
        }),
        UnitType::Port => Box::new(move |mg, tile| {
            let mut w = 0.0;
            // Prefer to be away from other structures of the same type
            let (_, closest_other_dist) = closest_tile(&mg.map, others_without(tile), tile);
            w += f64::min(closest_other_dist, structure_spacing);
            w
        }),
        UnitType::DefensePost => Box::new(move |mg, tile| {
            let mut w = 0.0;
            // Prefer higher elevations
            w += mg.map.magnitude(tile) as f64;
            let (closest, closest_border_dist) =
                closest_tile(&mg.map, border_tiles.iter().copied(), tile);
            if let Some(closest) = closest {
                // Prefer to be borderSpacing tiles from the border
                w += f64::max(
                    0.0,
                    border_spacing - (border_spacing - closest_border_dist).abs(),
                );
                // Prefer adjacent players who are hostile
                let mut neighbors: OrderedSet<SmallId> = OrderedSet::new();
                for t in mg.map.neighbors(closest) {
                    if !mg.map.is_land(t) {
                        continue;
                    }
                    let id = mg.map.owner_id(t);
                    if id == player {
                        continue;
                    }
                    if id == 0 {
                        continue;
                    }
                    neighbors.insert(id);
                }
                for &neighbor in neighbors.iter() {
                    w += border_spacing
                        * (Relation::Friendly as i32 as f64
                            - mg.player(player).relation(neighbor) as i32 as f64);
                }
            }
            // Prefer to be away from other structures of the same type
            if let Some((closest_other, _)) =
                closest_two_tiles(&mg.map, others_without(tile), [tile])
            {
                let d = mg.map.manhattan_dist(closest_other, tile) as f64;
                w += f64::min(d, structure_spacing);
            }
            w
        }),
        UnitType::SAMLauncher => {
            let mut protect_tiles: OrderedSet<TileRef> = OrderedSet::new();
            for unit in game.player(player).units().iter().copied() {
                let u = game.unit_any(unit);
                if matches!(
                    u.unit_type(),
                    UnitType::City | UnitType::Factory | UnitType::MissileSilo | UnitType::Port
                ) {
                    protect_tiles.insert(u.tile());
                }
            }
            let protect: Vec<TileRef> = protect_tiles.iter().copied().collect();
            let range = game.config().default_sam_range();
            let range_squared = range * range;
            Box::new(move |mg, tile| {
                let mut w = 0.0;
                // Prefer higher elevations
                w += mg.map.magnitude(tile) as f64;
                // Prefer to be away from the border
                if let Some((closest_border, _)) =
                    closest_two_tiles(&mg.map, border_tiles.iter().copied(), [tile])
                {
                    let d = mg.map.manhattan_dist(closest_border, tile) as f64;
                    w += f64::min(d, border_spacing);
                }
                // Prefer to be away from other structures of the same type
                if let Some((closest_other, _)) =
                    closest_two_tiles(&mg.map, others_without(tile), [tile])
                {
                    let d = mg.map.manhattan_dist(closest_other, tile) as f64;
                    w += f64::min(d, structure_spacing);
                }
                // Prefer to be in range of other structures
                for &maybe_protected in &protect {
                    let distance_squared =
                        mg.map.euclidean_dist_squared(tile, maybe_protected) as f64;
                    if distance_squared > range_squared {
                        continue;
                    }
                    w += structure_spacing;
                }
                w
            })
        }
        other => panic!("Value function not implemented for {}", other.as_str()),
    }
}
