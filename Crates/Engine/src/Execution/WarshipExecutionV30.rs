// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/WarshipExecution.ts as of v0.30: a warship that patrols, shoots and hunts trade ships (no docking or repair retreat).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::WarshipExecution::WarshipExecution;
use crate::Builds::Rules::Change;
use crate::Game::Game::UnitType;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Types::PathResult;

/// `findTargetUnit()`: transport ships first, then warships, then trade ships; nearest wins.
pub(super) fn find_target_unit(
    exec: &WarshipExecution,
    game: &GameImpl,
) -> Option<crate::Game::Game::UnitId> {
    let config = game.config();
    let warship = exec.ship();
    let owner = exec.owner(game);
    let has_port = game.player(owner).unit_count(UnitType::Port) > 0;
    let patrol_tile = exec.patrol_tile(game).expect("a warship has a patrol tile");
    let patrol_range_squared = config.warship_patrol_range().powi(2);
    let ships = game.nearby_units(
        game.unit_any(warship).tile(),
        config.warship_targetting_range(),
        &[
            UnitType::TransportShip,
            UnitType::Warship,
            UnitType::TradeShip,
        ],
        None,
        false,
    );
    let mut best: Option<crate::Game::Game::UnitId> = None;
    let mut best_priority = 0;
    let mut best_dist = 0.0;
    for (unit_id, dist_squared) in ships {
        let unit = game.unit_any(unit_id);
        if unit.owner_id() == owner
            || unit_id == warship
            || if game.has(Change::WarshipLegacy28) {
                game.player(unit.owner_id())
                    .is_friendly(owner, game.has(Change::WarshipAfkFriendly2203))
            } else {
                !game.player(owner).can_attack_player(unit.owner_id(), true)
            }
            || exec.already_sent_shell.contains(&unit_id)
        {
            continue;
        }
        let unit_type = unit.unit_type();
        if unit_type == UnitType::TradeShip {
            let target_owner = unit.target_unit().map(|t| game.unit_any(t).owner_id());
            if !has_port
                || game.unit_ref(unit_id).is_safe_from_pirates()
                || target_owner == Some(owner)
                || target_owner.is_some_and(|t| game.player(t).is_friendly(owner, false))
            {
                continue;
            }
            // Warships do not chase trade ships far from the patrol tile.
            if game.map.euclidean_dist_squared(patrol_tile, unit.tile()) as f64
                > patrol_range_squared
            {
                continue;
            }
        }
        let priority = match unit_type {
            UnitType::TransportShip => 0,
            UnitType::Warship => 1,
            _ => 2,
        };
        if best.is_none()
            || priority < best_priority
            || (priority == best_priority && dist_squared < best_dist)
        {
            best = Some(unit_id);
            best_priority = priority;
            best_dist = dist_squared;
        }
    }
    best
}

pub(super) fn shoot_target(exec: &mut WarshipExecution, game: &mut GameImpl) {
    let ship = exec.ship();
    let rate = game.config().warship_shell_attack_rate();
    let ticks = game.ticks() as f64;
    if ticks - exec.last_shell_attack > rate {
        let target = game
            .unit_any(ship)
            .target_unit()
            .expect("shootTarget needs a target");
        if game.unit_any(target).unit_type() != UnitType::TransportShip {
            // Warships don't need to reload when attacking transport ships.
            exec.last_shell_attack = ticks;
        }
        let owner = exec.owner(game);
        let ship_tile = game.unit_any(ship).tile();
        game.add_execution(Box::new(
            crate::Execution::ShellExecution::ShellExecution::new(ship_tile, owner, ship, target),
        ));
        if !game.unit_ref(target).has_health() {
            // Don't send multiple shells to a target that can be one-shotted.
            exec.already_sent_shell.insert(target);
            game.unit_mut(ship).set_target_unit(None);
        }
    }
}

fn hunt_down_trade_ship(exec: &mut WarshipExecution, game: &mut GameImpl) {
    let ship = exec.ship();
    for _ in 0..2 {
        let target = game
            .unit_any(ship)
            .target_unit()
            .expect("hunt needs a target");
        let target_tile = game.unit_any(target).tile();
        let ship_tile = game.unit_any(ship).tile();
        match exec.next_step(game, ship_tile, target_tile, Some(5.0)) {
            PathResult::Complete(_) => {
                let owner = exec.owner(game);
                game.player_mut(owner).capture_unit(target);
                game.unit_mut(ship).set_target_unit(None);
                let tile = game.unit_any(ship).tile();
                game.unit_mut(ship).move_to(tile);
                return;
            }
            PathResult::Next(node) => game.unit_mut(ship).move_to(node),
            PathResult::NotFound => {}
        }
    }
}

fn patrol(exec: &mut WarshipExecution, game: &mut GameImpl) {
    let ship = exec.ship();
    if game.unit_any(ship).target_tile().is_none() {
        let tile = exec.random_tile(game, false);
        game.unit_mut(ship).set_target_tile(tile);
        if tile.is_none() {
            return;
        }
    }
    let ship_tile = game.unit_any(ship).tile();
    let target_tile: TileRef = game.unit_any(ship).target_tile().expect("set above");
    match exec.next_step(game, ship_tile, target_tile, None) {
        PathResult::Complete(node) => {
            game.unit_mut(ship).set_target_tile(None);
            game.unit_mut(ship).move_to(node);
        }
        PathResult::Next(node) => game.unit_mut(ship).move_to(node),
        PathResult::NotFound => {}
    }
}

/// `tick()` of the v0.30 warship.
pub fn tick_v30(exec: &mut WarshipExecution, game: &mut GameImpl) {
    let ship = exec.ship();
    if !game.unit_any(ship).is_active() {
        exec.set_inactive();
        return;
    }
    if game.unit_any(ship).health() <= 0.0 {
        game.unit_mut(ship).delete(None, None);
        return;
    }
    let owner = exec.owner(game);
    if game.player(owner).unit_count(UnitType::Port) > 0 {
        game.unit_mut(ship).modify_health(1.0, None);
    }
    let target = find_target_unit(exec, game);
    game.unit_mut(ship).set_target_unit(target);
    let target_type = target.map(|t| game.unit_any(t).unit_type());
    if target_type == Some(UnitType::TradeShip) {
        hunt_down_trade_ship(exec, game);
        return;
    }
    patrol(exec, game);
    if game.unit_any(ship).target_unit().is_some() {
        shoot_target(exec, game);
    }
}
