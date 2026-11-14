// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim trace`: writes the NDJSON trace of `Reference/README.md` for a record.

use anyhow::Result;
use ofsim_engine::Compat::JsNumber::js_number_to_string;
use ofsim_engine::Game::GameImpl::GameImpl;
use ofsim_engine::Runner::Replay::{Stop, replay};
use std::io::Write;
use std::path::Path;

/// Writes one trace line per tick to `out`; returns a short summary (stop reason).
pub fn trace(record: &Path, maps: &Path, out: &Path, until: Option<u32>) -> Result<String> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut write_error: Option<std::io::Error> = None;
    // Optional per-tick unit dumps (`OFSIM_TRACE_UNITS_AT=t1,t2`), in the shape of the reference `--full-at` units.
    let unit_ticks: Vec<u32> = std::env::var("OFSIM_TRACE_UNITS_AT")
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_default();
    let mut units_file = if unit_ticks.is_empty() {
        None
    } else {
        Some(std::io::BufWriter::new(std::fs::File::create(format!(
            "{}.full.ndjson",
            out.display()
        ))?))
    };
    // Optional tile owner dumps (`OFSIM_OWNERS_AT=t1,t2`), in the shape of the reference `--full-at` owners.
    let owner_ticks: Vec<u32> = std::env::var("OFSIM_OWNERS_AT")
        .map(|v| v.split(',').filter_map(|x| x.trim().parse().ok()).collect())
        .unwrap_or_default();
    let mut owners_file = if owner_ticks.is_empty() {
        None
    } else {
        Some(std::io::BufWriter::new(std::fs::File::create(format!(
            "{}.owners.ndjson",
            out.display()
        ))?))
    };
    let outcome = replay(record, maps, until, &mut |runner, info| {
        if let Some(f) = owners_file.as_mut()
            && owner_ticks.contains(&info.tick)
        {
            let game = &runner.game;
            let owners: Vec<String> = (0..game.width() * game.height())
                .map(|t| game.owner_id(t).to_string())
                .collect();
            let _ = writeln!(
                f,
                "{{\"tick\":{},\"owners\":[{}]}}",
                info.tick,
                owners.join(",")
            );
        }
        if let Some(f) = units_file.as_mut()
            && unit_ticks.contains(&info.tick)
        {
            let _ = f.write_all(units_line(&runner.game, info.tick).as_bytes());
        }
        let line = trace_line(&runner.game, info.tick, info.turn.hash);
        if let Err(e) = file.write_all(line.as_bytes()) {
            write_error = Some(e);
            return false;
        }
        true
    })?;
    file.flush()?;
    if let Some(e) = write_error {
        return Err(e.into());
    }
    Ok(match outcome.stop {
        Stop::Panic { tick, message } => format!(
            "stopped at tick {tick}: {message} ({} ticks written)",
            outcome.ticks_run
        ),
        other => format!("{other:?} ({} ticks written)", outcome.ticks_run),
    })
}

fn units_line(game: &GameImpl, tick: u32) -> String {
    let mut units: Vec<String> = Vec::new();
    for id in game.all_players() {
        for &u in game.player(id).units() {
            let unit = game.unit_any(u);
            units.push(format!(
                "{{\"id\":{},\"type\":\"{:?}\",\"owner\":{},\"tile\":{},\"troops\":{},\"level\":{},\"active\":{},\"hash\":{}}}",
                u,
                unit.unit_type(),
                unit.owner_id(),
                unit.tile(),
                js_number_to_string(unit.troops()),
                unit.level(),
                unit.is_active(),
                js_number_to_string(unit.hash())
            ));
        }
    }
    format!(
        "{{\"tick\":{tick},\"full\":{{\"units\":[{}]}}}}\n",
        units.join(",")
    )
}

/// One NDJSON trace line (with trailing newline) of the game state after tick `tick`.
pub fn trace_line(game: &GameImpl, tick: u32, record_hash: Option<f64>) -> String {
    let mut line = String::with_capacity(256);
    line.push_str(&format!(
        "{{\"tick\":{},\"hash\":{},\"recordHash\":{},\"players\":[",
        tick,
        js_number_to_string(game.hash()),
        record_hash.map_or("null".to_string(), js_number_to_string)
    ));
    for (i, id) in game.all_players().into_iter().enumerate() {
        let p = game.player(id);
        let mut unit_hash_sum = 0.0;
        // Debug: `OFSIM_UNIT_LOG=<smallId>` prints that player's mobile units to stderr every tick.
        let log_units = std::env::var("OFSIM_UNIT_LOG").is_ok_and(|v| v == id.to_string());
        for &u in p.units() {
            unit_hash_sum += game.unit_any(u).hash();
            if log_units && !game.unit_any(u).unit_type().is_structure() {
                eprintln!(
                    "UNIT {} {} {:?} {}",
                    tick,
                    u,
                    game.unit_any(u).unit_type(),
                    game.unit_any(u).tile()
                );
            }
        }
        if i > 0 {
            line.push(',');
        }
        line.push_str(&format!(
            "[{},{},{},\"{}\",{},{},{}]",
            id,
            js_number_to_string(p.troops()),
            p.num_tiles_owned(),
            p.gold(),
            js_number_to_string(unit_hash_sum),
            p.is_alive(),
            p.units().len()
        ));
    }
    line.push_str("]}\n");
    line
}
