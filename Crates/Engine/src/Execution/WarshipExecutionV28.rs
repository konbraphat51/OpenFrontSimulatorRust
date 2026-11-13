// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/WarshipExecution.ts as of v0.28.4: the v0.30 warship on the `MiniAStar` stepper.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::WarshipExecution::WarshipExecution;
use super::WarshipExecutionV30::{find_target_unit, shoot_target};
use crate::Builds::Rules::Change;
use crate::Game::Game::UnitType;
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Legacy28::PathFinding::StepResult;

fn step(
    exec: &mut WarshipExecution,
    game: &GameImpl,
    from: TileRef,
    to: TileRef,
    dist: u32,
) -> StepResult {
    exec.legacy_pathfinder
        .as_mut()
        .expect("set in init")
        .next_tile(&game.map, game.water_manager.mini_map(), from, to, dist)
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
        match step(exec, game, ship_tile, target_tile, 5) {
            StepResult::Completed(_) => {
                let owner = exec.owner(game);
                game.player_mut(owner).capture_unit(target);
                game.unit_mut(ship).set_target_unit(None);
                let tile = game.unit_any(ship).tile();
                game.unit_mut(ship).move_to(tile);
                return;
            }
            StepResult::NextTile(node) => game.unit_mut(ship).move_to(node),
            StepResult::Pending | StepResult::PathNotFound => {}
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
    let target_tile = game.unit_any(ship).target_tile().expect("set above");
    match step(exec, game, ship_tile, target_tile, 1) {
        StepResult::Completed(node) => {
            game.unit_mut(ship).set_target_tile(None);
            game.unit_mut(ship).move_to(node);
        }
        StepResult::NextTile(node) => game.unit_mut(ship).move_to(node),
        StepResult::Pending => {}
        StepResult::PathNotFound => game.unit_mut(ship).set_target_tile(None),
    }
}

/// `tick()` of the v0.28.4 warship.
pub fn tick_v28(exec: &mut WarshipExecution, game: &mut GameImpl) {
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
    if game.has(Change::WarshipDeleteDisconnected26) && game.player(owner).is_disconnected() {
        game.unit_mut(ship).delete(None, None);
        return;
    }
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
