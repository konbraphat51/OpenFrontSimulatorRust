// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/TutorialCollapse.ts and TutorialSilo.ts: the onboarding tutorial's scripted nations.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::MissileSiloExecution::MissileSiloExecution;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Game::Game::{PlayerType, SmallId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// A tutorial nation the player is attacking falls once it is down to this many tiles.
const TUTORIAL_COLLAPSE_TILES: u32 = 250;

/// The x of the strait splitting the Tutorial map.
const STRAIT_X: u32 = 142;

/// How far the empire's warship wanders from its dock, in tiles.
const PATROL_RADIUS: i64 = 6;

fn first_human(game: &GameImpl) -> Option<SmallId> {
    game.players()
        .into_iter()
        .find(|&p| game.player(p).player_type() == PlayerType::Human)
}

/// `tutorialCollapse(game, nation)`: the player conquers the rest of a nation it is attacking once it is small.
pub fn tutorial_collapse(game: &mut GameImpl, nation: SmallId) {
    if game.player(nation).num_tiles_owned() >= TUTORIAL_COLLAPSE_TILES as usize {
        return;
    }
    let Some(human) = first_human(game) else {
        return;
    };
    let attacked = game
        .player(nation)
        .incoming_attacks()
        .into_iter()
        .any(|a| game.attack(a).attacker() == human);
    if !attacked {
        return;
    }
    game.conquer_player(human, nation);
    let tiles: Vec<TileRef> = game.player(nation).tiles().iter().collect();
    for tile in tiles {
        game.player_mut(human).conquer(tile);
    }
}

fn on_players_island(game: &GameImpl, nation: SmallId) -> bool {
    game.player(nation)
        .spawn_tile()
        .is_some_and(|spawn| game.x(spawn) < STRAIT_X)
}

/// Gives the nation what it lacks to afford the unit.
fn top_up_gold(game: &mut GameImpl, nation: SmallId, t: UnitType) {
    let cost = game.player(nation).unit_cost(t, 0);
    let gold = game.player(nation).gold();
    if gold < cost {
        game.player_mut(nation).add_gold(cost - gold);
    }
}

/// `tutorialSilo(game, nation, fire)`: the nation on the player's island builds a silo and nukes the player.
pub fn tutorial_silo(game: &mut GameImpl, nation: SmallId, mut fire: bool) {
    if !on_players_island(game, nation) {
        return;
    }
    let Some(human) = first_human(game) else {
        return;
    };
    let mut silo = game
        .player(nation)
        .units_of(UnitType::MissileSilo)
        .first()
        .copied();
    if silo.is_none() {
        // Built once: a captured silo is not rebuilt.
        if game.player(nation).units_constructed(UnitType::MissileSilo) > 0 {
            return;
        }
        if !game.player(nation).shares_border_with(human) {
            return;
        }
        let Some(tile) = silo_tile(game, nation) else {
            return;
        };
        top_up_gold(game, nation, UnitType::MissileSilo);
        let built =
            game.player_mut(nation)
                .build_unit(UnitType::MissileSilo, tile, UnitParams::default());
        silo = Some(built);
        game.add_execution(Box::new(MissileSiloExecution::new(built)));
        // The first nuke goes as soon as it is up.
        fire = true;
    }
    // The empire's harbor goes up with the silo, or once it reaches the shore.
    for p in game.players() {
        if game.player(p).player_type() == PlayerType::Nation
            && !on_players_island(game, p)
            && game.player(p).units_constructed(UnitType::Port) == 0
        {
            build_harbor(game, p, human);
        }
    }

    let silo = silo.expect("set above");
    if !fire || game.unit_any(silo).is_in_cooldown() {
        return;
    }
    let Some(target) = nuke_target(game, nation, human) else {
        return;
    };
    top_up_gold(game, nation, UnitType::AtomBomb);
    if game
        .player(nation)
        .can_build(UnitType::AtomBomb, target, None)
        .is_none()
    {
        return;
    }
    game.add_execution(Box::new(NukeExecution::new(
        UnitType::AtomBomb,
        nation,
        target,
        None,
        -1.0,
        0.0,
        Some(true),
    )));
}

/// A port on the nation's shore farthest from the player, and a warship beside it.
fn build_harbor(game: &mut GameImpl, nation: SmallId, human: SmallId) {
    let Some(from) = game.player(human).spawn_tile() else {
        return;
    };
    let mut port: Option<TileRef> = None;
    let mut port_dist: i64 = -1;
    for t in game.player(nation).border_tiles().iter() {
        let d = game.map.euclidean_dist_squared(t, from) as i64;
        if d <= port_dist || !game.map.is_shore(t) {
            continue;
        }
        port = Some(t);
        port_dist = d;
    }
    let Some(port) = port else {
        return;
    };
    let Some(dock) = game
        .map
        .neighbors(port)
        .into_iter()
        .find(|&n| game.map.is_ocean(n))
    else {
        return;
    };
    top_up_gold(game, nation, UnitType::Port);
    game.player_mut(nation)
        .build_unit(UnitType::Port, port, UnitParams::default());
    top_up_gold(game, nation, UnitType::Warship);
    game.player_mut(nation).build_unit(
        UnitType::Warship,
        dock,
        UnitParams {
            patrol_tile: Some(dock),
            ..UnitParams::default()
        },
    );
}

/// `tutorialWarship(game, nation, ticks)`: moves the empire's warship around its dock, a tile every other tick.
pub fn tutorial_warship(game: &mut GameImpl, nation: SmallId, ticks: u32) {
    if !ticks.is_multiple_of(2) {
        return;
    }
    let Some(ship) = game
        .player(nation)
        .units_of(UnitType::Warship)
        .first()
        .copied()
    else {
        return;
    };
    let Some(home) = game.unit_any(ship).warship_state().patrol_tile else {
        return;
    };
    let mut target = game.unit_any(ship).target_tile();
    if target.is_none() || target == Some(game.unit_any(ship).tile()) {
        let Some(point) = patrol_point(game, home, ticks) else {
            return;
        };
        target = Some(point);
        game.unit_mut(ship).set_target_tile(Some(point));
    }
    let target = target.expect("set above");
    // The ocean neighbour nearest the target; if none is nearer, pick another.
    let ship_tile = game.unit_any(ship).tile();
    let mut next: Option<TileRef> = None;
    let mut best = game.map.euclidean_dist_squared(ship_tile, target);
    for n in game.map.neighbors(ship_tile) {
        if !game.map.is_ocean(n) {
            continue;
        }
        let d = game.map.euclidean_dist_squared(n, target);
        if d >= best {
            continue;
        }
        next = Some(n);
        best = d;
    }
    match next {
        None => game.unit_mut(ship).set_target_tile(None),
        Some(n) => game.unit_mut(ship).move_to(n),
    }
}

/// An ocean tile within `PATROL_RADIUS` of `home`, picked from the tick so every client picks the same.
fn patrol_point(game: &GameImpl, home: TileRef, ticks: u32) -> Option<TileRef> {
    let span = 2 * PATROL_RADIUS + 1;
    for i in 0..10u32 {
        let h = (ticks.wrapping_add(i) as i32).wrapping_mul(2654435761u32 as i32) as u32;
        let dx = (h as i64 % span) - PATROL_RADIUS;
        let dy = (((h >> 8) as i64) % span) - PATROL_RADIUS;
        if dx * dx + dy * dy > PATROL_RADIUS * PATROL_RADIUS {
            continue;
        }
        let x = game.x(home) as i64 + dx;
        let y = game.y(home) as i64 + dy;
        if !game.is_valid_coord(x as i32, y as i32) {
            continue;
        }
        let t = game.tile_ref(x as i32, y as i32);
        if game.map.is_ocean(t) {
            return Some(t);
        }
    }
    None
}

/// The nation's tile nearest where it spawned, its capital.
fn silo_tile(game: &GameImpl, nation: SmallId) -> Option<TileRef> {
    let from = game.player(nation).spawn_tile()?;
    let mut best: Option<TileRef> = None;
    let mut best_dist = u64::MAX;
    for t in game.player(nation).tiles().iter() {
        let d = game.map.euclidean_dist_squared(t, from);
        if d >= best_dist {
            continue;
        }
        best = Some(t);
        best_dist = d;
    }
    best
}

/// The player's tile nearest the middle of their land whose blast reaches none of the nation's.
fn nuke_target(game: &GameImpl, nation: SmallId, human: SmallId) -> Option<TileRef> {
    let outer = game.config().nuke_magnitudes(UnitType::AtomBomb).outer;
    let clear = (outer + 1.0) * (outer + 1.0);
    let border: Vec<TileRef> = game.player(nation).border_tiles().iter().collect();
    let human_tiles: Vec<TileRef> = game.player(human).tiles().iter().collect();
    let (mut sum_x, mut sum_y) = (0.0, 0.0);
    for &t in &human_tiles {
        sum_x += game.x(t) as f64;
        sum_y += game.y(t) as f64;
    }
    let n = game.player(human).num_tiles_owned() as f64;
    let (cx, cy) = (sum_x / n, sum_y / n);
    let from_middle = |t: TileRef| {
        let (dx, dy) = (game.x(t) as f64 - cx, game.y(t) as f64 - cy);
        dx * dx + dy * dy
    };
    let mut tiles = human_tiles;
    tiles.sort_by(|&a, &b| {
        from_middle(a)
            .partial_cmp(&from_middle(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    tiles.into_iter().find(|&t| {
        border
            .iter()
            .all(|&b| (game.map.euclidean_dist_squared(t, b) as f64) > clear)
    })
}
