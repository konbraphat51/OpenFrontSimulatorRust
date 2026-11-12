// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim nuke-scenario`: the Rust side of `Reference/NukeScenarioDump.ts`, a seeded launch program
//! for differential tests of nukes, SAMs and the Doomsday Clock.

use crate::Trace::trace_line;
use anyhow::Result;
use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Compat::JsNumber::js_number_to_string;
use ofsim_engine::Execution::MIRVExecution::MirvExecution;
use ofsim_engine::Execution::NukeExecution::NukeExecution;
use ofsim_engine::Execution::SAMLauncherExecution::SAMLauncherExecution;
use ofsim_engine::Game::Game::{PlayerType, UnitParams, UnitType};
use ofsim_engine::Runner::GameRunner::{GameRunner, create_game_runner};
use ofsim_record::Config::DoomsdayClockConfig;
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

/// Launch program parameters (see `Reference/NukeScenarioDump.ts`).
pub struct NukeScenarioArgs<'a> {
    /// Game record.
    pub record: &'a Path,
    /// Map resources directory.
    pub maps: &'a Path,
    /// Output NDJSON path (`<out>.digest` is written next to it).
    pub out: &'a Path,
    /// Ticks of the record to run first.
    pub warm: u32,
    /// Number of launch rounds.
    pub rounds: u32,
    /// Ticks per round.
    pub spacing: u32,
    /// Ticks after the last round.
    pub tail: u32,
    /// Doomsday clock speed, or `None`.
    pub doomsday: Option<&'a str>,
    /// Number of launching players.
    pub attackers: usize,
    /// Manhattan radius of the grow phase around each spawn.
    pub grow: i32,
    /// One in this many players gets a SAM launcher (0 = none).
    pub sam_mod: usize,
}

struct Writers {
    trace: std::io::BufWriter<std::fs::File>,
    digest: std::io::BufWriter<std::fs::File>,
}

/// Runs the program and writes the trace and the digest.
pub fn nuke_scenario(a: &NukeScenarioArgs) -> Result<()> {
    let record = GameRecord::load_decompressed(a.record)?;
    let rules = Rules::for_commit(&record.git_commit)?;
    let mut start = GameStartInfo::from_record(&record);
    if let Some(speed) = a.doomsday {
        start.config.doomsday_clock = Some(DoomsdayClockConfig {
            enabled: Some(true),
            speed: Some(speed.to_string()),
            extra: Default::default(),
        });
    }
    let mut runner = create_game_runner(&start, a.maps, rules)?;
    let digest_path = format!("{}.digest", a.out.display());
    let mut w = Writers {
        trace: std::io::BufWriter::new(std::fs::File::create(a.out)?),
        digest: std::io::BufWriter::new(std::fs::File::create(digest_path)?),
    };
    let trace_every: u32 = std::env::var("OFSIM_TRACE_EVERY")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let mut next_turn = 0u32;
    let mut run_tick = |runner: &mut GameRunner, w: &mut Writers| -> Result<()> {
        let tick = runner.game.ticks();
        let turn = if next_turn < a.warm {
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
        if tick.is_multiple_of(trace_every) {
            w.trace
                .write_all(trace_line(&runner.game, tick, None).as_bytes())?;
        }
        let state = runner.game.map.tile_state_buffer();
        let mut h: u32 = 2166136261;
        for &s in state {
            h = (h ^ s as u32).wrapping_mul(16777619);
        }
        writeln!(
            w.digest,
            "{},{},{},{},{}",
            tick,
            runner.game.map.num_tiles_with_fallout(),
            runner.game.map.num_land_tiles(),
            h,
            js_number_to_string(runner.game.hash())
        )?;
        Ok(())
    };

    let mut rng = Lcg(987654321);
    for _ in 0..a.warm {
        run_tick(&mut runner, &mut w)?;
    }

    let alive = runner.game.players();
    {
        let game = &mut runner.game;
        for &p in &alive {
            let Some(spawn) = game.player(p).spawn_tile() else {
                continue;
            };
            let (cx, cy) = (game.x(spawn) as i32, game.y(spawn) as i32);
            for dx in -a.grow..=a.grow {
                for dy in -(a.grow - dx.abs())..=(a.grow - dx.abs()) {
                    let (x, y) = (cx + dx, cy + dy);
                    if !game.is_valid_coord(x, y) {
                        continue;
                    }
                    let tile = game.tile_ref(x, y);
                    if game.is_land(tile)
                        && !game.map.is_impassable(tile)
                        && game.owner_id(tile) == 0
                    {
                        game.player_mut(p).conquer(tile);
                    }
                }
            }
        }
    }
    let humans: Vec<_> = alive
        .iter()
        .copied()
        .filter(|&p| runner.game.player(p).player_type() == PlayerType::Human)
        .collect();
    let attackers: Vec<_> = if humans.is_empty() { &alive } else { &humans }
        .iter()
        .copied()
        .take(a.attackers)
        .collect();
    for (index, &p) in alive.iter().enumerate() {
        let game = &mut runner.game;
        game.player_mut(p).add_gold(10_i64.pow(13));
        let tiles: Vec<u32> = game.player(p).tiles().iter().collect();
        if tiles.is_empty() {
            continue;
        }
        let mut pick = || tiles[rng.next() as usize % tiles.len()];
        if a.sam_mod > 0 && index % a.sam_mod == 0 {
            let sam =
                game.player_mut(p)
                    .build_unit(UnitType::SAMLauncher, pick(), UnitParams::default());
            game.add_execution(Box::new(SAMLauncherExecution::new(p, None, Some(sam))));
            if index % (2 * a.sam_mod) == 0 {
                game.player_mut(p).upgrade_unit(sam);
            }
        }
        game.player_mut(p)
            .build_unit(UnitType::City, pick(), UnitParams::default());
        game.player_mut(p)
            .build_unit(UnitType::DefensePost, pick(), UnitParams::default());
        if attackers.contains(&p) {
            for _ in 0..3 {
                game.player_mut(p)
                    .build_unit(UnitType::MissileSilo, pick(), UnitParams::default());
            }
        }
    }
    // Diplomacy: some accepted alliances and some pending requests towards attackers.
    for k in 0..10 {
        let ai = rng.next() as usize;
        let bi = rng.next() as usize;
        let game = &mut runner.game;
        let (pa, pb) = (attackers[ai % attackers.len()], alive[bi % alive.len()]);
        if pa == pb {
            continue;
        }
        if k < 6 {
            if let Some(request) = game.player_mut(pa).create_alliance_request(pb) {
                game.accept_alliance_request(request);
            }
        } else {
            game.player_mut(pb).create_alliance_request(pa);
        }
    }
    run_tick(&mut runner, &mut w)?;

    for round in 0..a.rounds {
        if round == 10 {
            for &p in &alive {
                for sam in runner.game.player(p).units_of(UnitType::SAMLauncher) {
                    runner.game.player_mut(p).upgrade_unit(sam);
                }
            }
        }
        let ai = rng.next() as usize;
        let ti = rng.next() as usize;
        let ki = rng.next() as usize;
        let tilei = rng.next() as usize;
        let game = &mut runner.game;
        let attacker = attackers[ai % attackers.len()];
        let target = alive[ti % alive.len()];
        let tiles: Vec<u32> = game.player(target).tiles().iter().collect();
        if attacker != target && !tiles.is_empty() && game.player(attacker).is_alive() {
            let tile = tiles[tilei % tiles.len()];
            game.player_mut(attacker).add_gold(10_i64.pow(12));
            let k = ki % 6;
            if k < 3 {
                game.add_execution(Box::new(NukeExecution::new(
                    UnitType::AtomBomb,
                    attacker,
                    tile,
                    None,
                    -1.0,
                    0.0,
                    None,
                )));
            } else if k < 5 {
                game.add_execution(Box::new(NukeExecution::new(
                    UnitType::HydrogenBomb,
                    attacker,
                    tile,
                    None,
                    -1.0,
                    0.0,
                    None,
                )));
            } else {
                game.add_execution(Box::new(MirvExecution::new(attacker, tile)));
            }
        }
        for _ in 0..a.spacing {
            run_tick(&mut runner, &mut w)?;
        }
    }
    for _ in 0..a.tail {
        run_tick(&mut runner, &mut w)?;
    }
    w.trace.flush()?;
    w.digest.flush()?;
    Ok(())
}
