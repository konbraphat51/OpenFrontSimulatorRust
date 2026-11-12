// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the nuke blast helpers of src/core/execution/Util.ts: weighted blast counts and the list of players a nuke angers (`wouldNukeBreakAlliance` is in `Util`).
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

/// The players a nuke angers before PR 2912: over the weighted threshold, or owning a structure of an ally of the launcher.
pub fn list_nuke_break_ally_legacy(
    game: &GameImpl,
    launcher: SmallId,
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
        if owner != 0 && game.player(launcher).is_allied_with(owner) && !players.contains(&owner) {
            players.push(owner);
        }
    }
    players
}
