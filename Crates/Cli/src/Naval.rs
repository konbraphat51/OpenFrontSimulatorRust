// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim naval-scenario`: the Rust side of `Reference/NavalScenarioDump.ts`, a scripted port, warship,
//! trade ship and transport ship scenario used for differential tests of the water executions.

use crate::Trace::trace_line;
use anyhow::Result;
use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Execution::BoatRetreatExecution::BoatRetreatExecution;
use ofsim_engine::Execution::MoveWarshipExecution::MoveWarshipExecution;
use ofsim_engine::Execution::TradeShipExecution::TradeShipExecution;
use ofsim_engine::Execution::TransportShipExecution::TransportShipExecution;
use ofsim_engine::Execution::WarshipExecution::WarshipExecution;
use ofsim_engine::Game::Game::{SmallId, UnitId, UnitParams, UnitType};
use ofsim_engine::Game::GameImpl::GameImpl;
use ofsim_engine::Runner::GameRunner::{GameRunner, create_game_runner};
use ofsim_record::Records::{GameRecord, Turn};
use ofsim_record::StartInfo::GameStartInfo;
use std::io::Write;
use std::path::Path;

fn shore_of(game: &GameImpl, p: SmallId) -> Option<u32> {
    game.player(p)
        .border_tiles()
        .iter()
        .find(|&t| game.map.is_shore(t) && game.map.is_land(t) && game.owner_id(t) == p)
}

fn send_trade(game: &mut GameImpl, ports: &[(SmallId, UnitId)], offset: usize) {
    for i in 0..ports.len() {
        let a = ports[i];
        let b = ports[(i + 1 + offset) % ports.len()];
        if a.1 == b.1 {
            continue;
        }
        let (a_tile, b_tile) = (game.unit_any(a.1).tile(), game.unit_any(b.1).tile());
        let component = game.water_manager.get_water_component(&game.map, b_tile);
        let reachable =
            component.is_some_and(|c| game.water_manager.has_water_component(&game.map, a_tile, c));
        if !reachable {
            continue;
        }
        game.add_execution(Box::new(TradeShipExecution::new(a.0, a.1, b.1)));
    }
}

fn send_boats(game: &mut GameImpl, cast: &[SmallId]) {
    for i in 0..cast.len() {
        let other = cast[(i + 1) % cast.len()];
        if let Some(target) = shore_of(game, other) {
            game.add_execution(Box::new(TransportShipExecution::new(cast[i], target, None)));
        }
    }
}

/// Runs the scenario and writes the trace to `out`.
pub fn naval_scenario(
    record_path: &Path,
    maps: &Path,
    out: &Path,
    warm: u32,
    total_ticks: u32,
    max_players: usize,
) -> Result<()> {
    let record = GameRecord::load_decompressed(record_path)?;
    let rules = Rules::for_commit(&record.git_commit)?;
    let start = GameStartInfo::from_record(&record);
    let mut runner = create_game_runner(&start, maps, rules)?;
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut next_turn = 0u32;
    let mut run_tick = |runner: &mut GameRunner, file: &mut std::io::BufWriter<std::fs::File>| {
        let tick = runner.game.ticks();
        let turn = if next_turn < warm {
            record.turns[next_turn as usize].clone()
        } else {
            Turn {
                turn_number: next_turn,
                intents: vec![],
                hash: None,
            }
        };
        next_turn += 1;
        runner.add_turn(turn);
        runner.execute_next_tick();
        file.write_all(trace_line(&runner.game, tick, None).as_bytes())
    };
    for _ in 0..warm {
        run_tick(&mut runner, &mut file)?;
    }

    let game = &mut runner.game;
    // The first players (in id order) that own a shore tile
    let cast: Vec<SmallId> = game
        .players()
        .into_iter()
        .filter(|&p| shore_of(game, p).is_some())
        .take(max_players)
        .collect();
    let mut ports: Vec<(SmallId, UnitId)> = Vec::new();
    for &p in &cast {
        game.player_mut(p).add_gold(100_000_000);
        let Some(shore) = shore_of(game, p) else {
            continue;
        };
        let unit = game
            .player_mut(p)
            .build_unit(UnitType::Port, shore, UnitParams::default());
        ports.push((p, unit));
        if let Some(water) = game
            .map
            .neighbors(shore)
            .into_iter()
            .find(|&n| game.map.is_water(n))
        {
            game.add_execution(Box::new(WarshipExecution::new(p, water)));
        }
    }

    for s in 0..total_ticks {
        let game = &mut runner.game;
        if s == 0 {
            send_trade(game, &ports, 0);
            send_boats(game, &cast);
        }
        if s == 60 {
            // Retreat the first transport ship of every player
            for &p in &cast {
                if let Some(&boat) = game.player(p).units_of(UnitType::TransportShip).first()
                    && boat % 2 == 0
                {
                    game.add_execution(Box::new(BoatRetreatExecution::new(p, boat)));
                }
            }
        }
        if s == 150 {
            // Move every warship 30 tiles along x when that tile is water
            for &p in &cast {
                for w in game.player(p).units_of(UnitType::Warship) {
                    let tile = game.unit_any(w).tile();
                    let x = game.x(tile) as i32 + 30;
                    let y = game.y(tile) as i32;
                    if game.is_valid_coord(x, y) && game.map.is_water(game.tile_ref(x, y)) {
                        let target = game.tile_ref(x, y);
                        game.add_execution(Box::new(MoveWarshipExecution::new(
                            p,
                            vec![w as i64, w as i64],
                            target,
                        )));
                    }
                }
            }
        }
        if s == 300 && ports.len() > 1 {
            // Turn the second player's port tile into water: boats heading there must retreat
            let tile = game.unit_any(ports[1].1).tile();
            game.player_mut(ports[1].0).relinquish(tile);
            game.set_water(tile);
        }
        if s > 0 && s % 250 == 0 {
            send_trade(game, &ports, ((s / 250) % 3) as usize);
            send_boats(game, &cast);
        }
        run_tick(&mut runner, &mut file)?;
    }
    file.flush()?;
    Ok(())
}
