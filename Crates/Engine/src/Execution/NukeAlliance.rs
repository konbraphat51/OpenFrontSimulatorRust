// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the nuke blast helpers of src/core/execution/Util.ts: weighted blast counts and alliance-break checks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::OrderedMap::OrderedMap;
use crate::Configuration::Config::NukeMagnitude;
use crate::Game::Game::{STRUCTURES, SmallId};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// `computeNukeBlastCounts`: weighted tile count per player in the blast zone, first-seen order.
pub fn compute_nuke_blast_counts(
    game: &GameImpl,
    target_tile: TileRef,
    magnitude: NukeMagnitude,
) -> OrderedMap<SmallId, f64> {
    let inner2 = magnitude.inner * magnitude.inner;
    let mut counts: OrderedMap<SmallId, f64> = OrderedMap::new();
    game.map.circle_search_visit(
        target_tile,
        magnitude.outer as i32,
        |tile, d2| {
            let owner = game.map.owner_id(tile);
            if owner > 0 {
                let weight = if d2 as f64 <= inner2 { 1.0 } else { 0.5 };
                let prev = counts.get(&owner).copied().unwrap_or(0.0);
                counts.insert(owner, prev + weight);
            }
            false
        },
        |_| {},
    );
    counts
}

/// `wouldNukeBreakAlliance`: an allied structure is in range or an ally's weighted tiles exceed `threshold`.
pub fn would_nuke_break_alliance(
    game: &GameImpl,
    target_tile: TileRef,
    magnitude: NukeMagnitude,
    ally_small_ids: Option<&[SmallId]>,
    threshold: f64,
) -> bool {
    let Some(allies) = ally_small_ids else {
        return false;
    };
    if allies.is_empty() {
        return false;
    }
    if game.any_unit_nearby(
        target_tile,
        magnitude.outer,
        STRUCTURES,
        |unit| unit.owner != 0 && allies.contains(&unit.owner),
        None,
        false,
    ) {
        return true;
    }
    let inner2 = magnitude.inner * magnitude.inner;
    let mut ally_tile_counts: OrderedMap<SmallId, f64> = OrderedMap::new();
    let mut result = false;
    game.map.circle_search_visit(
        target_tile,
        magnitude.outer as i32,
        |tile, d2| {
            if result {
                return false;
            }
            let owner = game.map.owner_id(tile);
            if owner > 0 && allies.contains(&owner) {
                let weight = if d2 as f64 <= inner2 { 1.0 } else { 0.5 };
                let new_count = ally_tile_counts.get(&owner).copied().unwrap_or(0.0) + weight;
                ally_tile_counts.insert(owner, new_count);
                if new_count > threshold {
                    result = true;
                }
            }
            false
        },
        |_| {},
    );
    result
}

/// `listNukeBreakAlliance`: every player over the weighted threshold or with a structure in range.
pub fn list_nuke_break_alliance(
    game: &GameImpl,
    target_tile: TileRef,
    magnitude: NukeMagnitude,
    threshold: f64,
) -> Vec<SmallId> {
    let mut players: Vec<SmallId> = Vec::new();
    let counts = compute_nuke_blast_counts(game, target_tile, magnitude);
    for (&player, &weight) in counts.iter() {
        if weight > threshold && !players.contains(&player) {
            players.push(player);
        }
    }
    for (id, _) in game.nearby_units(target_tile, magnitude.outer, STRUCTURES, None, false) {
        let owner = game.unit_any(id).owner;
        if !players.contains(&owner) {
            players.push(owner);
        }
    }
    players
}
