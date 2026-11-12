// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim trace`: writes the NDJSON trace of `Reference/README.md` for a record.

use crate::JsNumberFormat::js_number_to_string;
use crate::Replaying::{Stop, replay};
use anyhow::Result;
use std::io::Write;
use std::path::Path;

/// Writes one trace line per tick to `out`; returns a short summary (stop reason).
pub fn trace(record: &Path, maps: &Path, out: &Path, until: Option<u32>) -> Result<String> {
    let mut file = std::io::BufWriter::new(std::fs::File::create(out)?);
    let mut write_error: Option<std::io::Error> = None;
    let outcome = replay(record, maps, until, &mut |runner, info| {
        let game = &runner.game;
        let mut line = String::with_capacity(256);
        line.push_str(&format!(
            "{{\"tick\":{},\"hash\":{},\"recordHash\":{},\"players\":[",
            info.tick,
            js_number_to_string(game.hash()),
            info.turn
                .hash
                .map_or("null".to_string(), js_number_to_string)
        ));
        for (i, id) in game.all_players().into_iter().enumerate() {
            let p = game.player(id);
            let mut unit_hash_sum = 0.0;
            for &u in p.units() {
                unit_hash_sum += game.unit_any(u).hash();
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
