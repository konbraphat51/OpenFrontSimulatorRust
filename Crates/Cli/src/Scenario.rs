// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim scenario`: the Rust side of `Reference/ScenarioDump.ts`, a seeded territory fuzz used for
//! differential tests of the territory bookkeeping and `PlayerExecution`.

use crate::Trace::trace_line;
use anyhow::Result;
use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Runner::GameRunner::create_game_runner;
use ofsim_record::Records::{GameRecord, Turn};
use ofsim_record::StartInfo::GameStartInfo;
use std::io::Write;
use std::path::Path;

struct Lcg(u32);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(1664525).wrapping_add(1013904223);
        self.0 >> 8
    }
}

/// Runs the scenario and writes the trace to `out`.
pub fn scenario(
    record_path: &Path,
    maps: &Path,
    out: &Path,
    warm: u32,
    rounds: u32,
    steps: u32,
    focus: u32,
    free_only: bool,
    enclaves: u32,
    radius: i32,
) -> Result<()> {
    let record = GameRecord::load_decompressed(record_path)?;
    let rules = Rules::for_commit(&record.git_commit)?;
    let start = GameStartInfo::from_record(&record);
    let mut runner = create_game_runner(&start, maps, rules)?;
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut next_turn = 0u32;
    let mut run_tick = |runner: &mut ofsim_engine::Runner::GameRunner::GameRunner,
                        file: &mut std::io::BufWriter<std::fs::File>|
     -> Result<()> {
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
        file.write_all(trace_line(&runner.game, tick, None).as_bytes())?;
        Ok(())
    };
    for _ in 0..warm {
        run_tick(&mut runner, &mut file)?;
    }
    {
        // Enclave phase: the first alive player walls in the next `enclaves` players.
        let game = &mut runner.game;
        let alive = game.players();
        let h = alive[0];
        for k in 1..=enclaves as usize {
            if k >= alive.len() {
                break;
            }
            let t = alive[k];
            let Some(spawn) = game.player(t).spawn_tile() else {
                continue;
            };
            let (cx, cy) = (game.x(spawn) as i32, game.y(spawn) as i32);
            for dx in -radius..=radius {
                for dy in -(radius - dx.abs())..=(radius - dx.abs()) {
                    let (x, y) = (cx + dx, cy + dy);
                    if !game.is_valid_coord(x, y) {
                        continue;
                    }
                    let tile = game.tile_ref(x, y);
                    if game.is_land(tile)
                        && !game.map.is_impassable(tile)
                        && game.owner_id(tile) == 0
                    {
                        game.player_mut(h).conquer(tile);
                    }
                }
            }
            let border: Vec<u32> = game.player(t).border_tiles().iter().collect();
            if let Some(&b) = border.first() {
                game.player_mut(h).conquer(b);
            }
        }
        for _ in 0..5 {
            run_tick(&mut runner, &mut file)?;
        }
    }
    let mut rng = Lcg(12345);
    for _ in 0..rounds {
        for _ in 0..steps {
            let game = &mut runner.game;
            let alive = game.players();
            if alive.is_empty() {
                break;
            }
            let pool = if focus > 0 {
                alive.len().min(focus as usize)
            } else {
                alive.len()
            };
            let p = alive[rng.next() as usize % pool];
            let border: Vec<u32> = game.player(p).border_tiles().iter().collect();
            let b = border
                .get(rng.next() as usize % border.len().max(1))
                .copied();
            let ns = b.map(|b| game.map.neighbors(b)).unwrap_or_default();
            let n = ns.get(rng.next() as usize % ns.len().max(1)).copied();
            if let Some(n) = n
                && game.is_land(n)
                && !game.map.is_impassable(n)
                && game.owner_id(n) != p
                && (!free_only || game.owner_id(n) == 0)
            {
                game.player_mut(p).conquer(n);
            }
            if rng.next() % 40 == 0 {
                let tiles: Vec<u32> = game.player(p).tiles().iter().collect();
                if let Some(&t) = tiles.get(rng.next() as usize % tiles.len().max(1)) {
                    game.player_mut(p).relinquish(t);
                }
            }
        }
        run_tick(&mut runner, &mut file)?;
    }
    file.flush()?;
    Ok(())
}
