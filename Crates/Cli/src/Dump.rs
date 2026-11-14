// SPDX-License-Identifier: AGPL-3.0-only
//! `ofsim replay`: writes the files of the OpenFrontDs `Engine/ReplayDumper.ts` (tile snapshots,
//! entity snapshots, `Result.json`) so consumers can switch engines.

use anyhow::Result;
use ofsim_engine::Compat::JsNumber::js_number_to_string;
use ofsim_engine::Game::Game::PlayerType;
use ofsim_engine::Game::GameImpl::GameImpl;
use ofsim_engine::Runner::Replay::{Stop, replay_record};
use ofsim_record::Records::GameRecord;
use std::collections::HashSet;
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{SyncSender, sync_channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

/// Final verdict of a dump, serialized to `Result.json` (field order of the upstream dumper).
pub struct DumpResult {
    /// `ok`, `desync` or `error`.
    pub status: &'static str,
    /// Game id of the record.
    pub game_id: String,
    /// Commit of the record.
    pub git_commit: String,
    /// Tick count after the last completed tick.
    pub final_tick: u32,
    /// Recorded hashes compared.
    pub hashes_checked: u32,
    /// Turn of the first hash mismatch.
    pub first_mismatch_tick: Option<u32>,
    /// Map width in tiles.
    pub width: u32,
    /// Map height in tiles.
    pub height: u32,
    /// Snapshot interval.
    pub snapshot_every: u32,
    /// Ticks that have a snapshot.
    pub snapshot_ticks: Vec<u32>,
    /// Error text of a failed run.
    pub error: Option<String>,
}

impl DumpResult {
    /// `JSON.stringify` of the result, as the upstream dumper writes it.
    pub fn to_json(&self) -> String {
        let text = |s: &str| serde_json::to_string(s).unwrap();
        let ticks: Vec<String> = self.snapshot_ticks.iter().map(u32::to_string).collect();
        format!(
            "{{\"status\":\"{}\",\"gameID\":{},\"gitCommit\":{},\"finalTick\":{},\"hashesChecked\":{},\"firstMismatchTick\":{},\"width\":{},\"height\":{},\"snapshotEvery\":{},\"snapshotTicks\":[{}],\"error\":{}}}",
            self.status,
            text(&self.game_id),
            text(&self.git_commit),
            self.final_tick,
            self.hashes_checked,
            self.first_mismatch_tick
                .map_or("null".into(), |t| t.to_string()),
            self.width,
            self.height,
            self.snapshot_every,
            ticks.join(","),
            self.error.as_deref().map_or("null".into(), text),
        )
    }
}

fn json_text(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

fn json_opt<T: ToString>(v: Option<T>) -> String {
    v.map_or("null".to_string(), |x| x.to_string())
}

fn json_ids(ids: &[u16]) -> String {
    let parts: Vec<String> = ids.iter().map(u16::to_string).collect();
    format!("[{}]", parts.join(","))
}

/// One Entities.ndjson line (with newline) for the current tick; no human names.
pub fn capture_entities(game: &GameImpl) -> String {
    let mut out = String::with_capacity(4096);
    let _ = write!(
        out,
        "{{\"tick\":{},\"inSpawnPhase\":{},\"players\":[",
        game.ticks(),
        game.in_spawn_phase()
    );
    let ids = game.all_players();
    for (i, &id) in ids.iter().enumerate() {
        let p = game.player(id);
        if i > 0 {
            out.push(',');
        }
        let name = if p.player_type() == PlayerType::Human {
            "null".to_string()
        } else {
            json_text(p.name())
        };
        let embargoes: Vec<u16> = p.get_embargoes().iter().map(|e| e.target).collect();
        let _ = write!(
            out,
            "{{\"smallId\":{},\"playerId\":{},\"clientId\":{},\"playerType\":\"{}\",\"name\":{},\"team\":{},\"isAlive\":{},\"hasSpawned\":{},\"isTraitor\":{},\"isDisconnected\":{},\"tilesOwned\":{},\"troops\":{},\"gold\":\"{}\",\"betrayals\":{},\"spawnTile\":{},\"allies\":{},\"targets\":{},\"embargoes\":{}}}",
            p.small_id(),
            json_text(p.id()),
            p.client_id().map_or("null".to_string(), json_text),
            p.player_type().as_str(),
            name,
            p.team().map_or("null".to_string(), |t| json_text(t)),
            p.is_alive(),
            p.has_spawned(),
            p.is_traitor(),
            p.is_disconnected(),
            p.num_tiles_owned(),
            js_number_to_string(p.troops()),
            p.gold(),
            p.betrayals(),
            json_opt(p.spawn_tile()),
            json_ids(&p.allies()),
            json_ids(&p.targets()),
            json_ids(&embargoes),
        );
    }
    out.push_str("],\"units\":[");
    for (i, u) in game.all_units().into_iter().enumerate() {
        let unit = game.unit_ref(u);
        if i > 0 {
            out.push(',');
        }
        let health = if unit.has_health() {
            js_number_to_string(unit.health())
        } else {
            "null".to_string()
        };
        let _ = write!(
            out,
            "{{\"unitId\":{},\"unitType\":\"{}\",\"ownerSmallId\":{},\"tile\":{},\"troops\":{},\"health\":{},\"level\":{},\"isUnderConstruction\":{},\"targetTile\":{}}}",
            unit.id(),
            unit.unit_type().as_str(),
            unit.owner_id(),
            unit.tile(),
            js_number_to_string(unit.troops()),
            health,
            unit.level(),
            unit.is_under_construction(),
            json_opt(unit.target_tile()),
        );
    }
    out.push_str("],\"attacks\":[");
    let mut first = true;
    for &id in &ids {
        for &a in game.player_impl(id).outgoing_attacks() {
            let attack = game.attack(a);
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(
                out,
                "{{\"attackId\":{},\"attackerSmallId\":{},\"targetSmallId\":{},\"troops\":{},\"isRetreating\":{},\"sourceTile\":{}}}",
                json_text(attack.id()),
                attack.attacker(),
                attack.target(),
                js_number_to_string(attack.troops()),
                attack.retreating(),
                json_opt(attack.source_tile()),
            );
        }
    }
    out.push_str("],\"alliances\":[");
    let mut seen = HashSet::new();
    let mut first = true;
    for &id in &ids {
        for &a in game.player_impl(id).alliances() {
            if !seen.insert(a) {
                continue;
            }
            let alliance = game.alliance(a);
            if !first {
                out.push(',');
            }
            first = false;
            let _ = write!(
                out,
                "{{\"requestorSmallId\":{},\"recipientSmallId\":{},\"createdAt\":{},\"expiresAt\":{}}}",
                alliance.requestor(),
                alliance.recipient(),
                alliance.created_at(),
                alliance.expires_at(),
            );
        }
    }
    out.push_str("]}\n");
    out
}

/// zstd frame in the shape Node's `zlib.zstdCompressSync` writes (level 3, content size).
fn compress_tiles(bytes: &[u8]) -> Result<Vec<u8>> {
    let mut encoder = zstd::stream::Encoder::new(Vec::new(), 3)?;
    encoder.set_pledged_src_size(Some(bytes.len() as u64))?;
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

/// Tile state as the bytes of a little-endian `u16` array.
fn tile_bytes(game: &GameImpl) -> Vec<u8> {
    let buffer = game.map.tile_state_buffer();
    let mut bytes = vec![0u8; buffer.len() * 2];
    for (chunk, v) in bytes.chunks_exact_mut(2).zip(buffer) {
        chunk.copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

/// Compresses and writes tile snapshots on background threads so the engine keeps running.
struct TileWriter {
    sender: Option<SyncSender<(u32, Vec<u8>)>>,
    workers: Vec<JoinHandle<Result<()>>>,
}

impl TileWriter {
    fn new(dir: PathBuf, threads: usize) -> Self {
        let threads = threads.max(1);
        let (sender, receiver) = sync_channel::<(u32, Vec<u8>)>(threads * 2);
        let receiver = Arc::new(Mutex::new(receiver));
        let workers = (0..threads)
            .map(|_| {
                let receiver = Arc::clone(&receiver);
                let dir = dir.clone();
                std::thread::spawn(move || -> Result<()> {
                    loop {
                        let message = receiver.lock().unwrap().recv();
                        let Ok((tick, bytes)) = message else {
                            return Ok(());
                        };
                        std::fs::write(
                            dir.join(format!("T{tick:06}.u16.zst")),
                            compress_tiles(&bytes)?,
                        )?;
                    }
                })
            })
            .collect();
        Self {
            sender: Some(sender),
            workers,
        }
    }

    /// Queues one snapshot; blocks while the queue is full.
    fn submit(&self, tick: u32, bytes: Vec<u8>) -> Result<()> {
        let sender = self.sender.as_ref().expect("writer is open");
        sender
            .send((tick, bytes))
            .map_err(|_| anyhow::anyhow!("snapshot writer stopped"))
    }

    /// Waits for every queued snapshot; returns the first write error.
    fn finish(&mut self) -> Result<()> {
        self.sender = None;
        let mut first: Option<anyhow::Error> = None;
        for worker in self.workers.drain(..) {
            let outcome = worker
                .join()
                .unwrap_or_else(|_| Err(anyhow::anyhow!("snapshot writer panicked")));
            if let Err(e) = outcome
                && first.is_none()
            {
                first = Some(e);
            }
        }
        first.map_or(Ok(()), Err)
    }
}

impl Drop for TileWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// Replays `record` and writes snapshots and `Result.json` into `out`; always writes `Result.json`
/// when the directory can be created.
pub fn dump(
    record: &GameRecord,
    maps: &Path,
    out: &Path,
    every: u32,
    writer_threads: usize,
) -> Result<DumpResult> {
    std::fs::create_dir_all(out)?;
    let mut result = DumpResult {
        status: "ok",
        game_id: record.info.game_id.clone(),
        git_commit: record.git_commit.clone(),
        final_tick: 0,
        hashes_checked: 0,
        first_mismatch_tick: None,
        width: 0,
        height: 0,
        snapshot_every: every,
        snapshot_ticks: Vec::new(),
        error: None,
    };
    let run = dump_into(record, maps, out, every, writer_threads, &mut result);
    if let Err(e) = run {
        result.status = "error";
        result.error = Some(format!("{e:#}"));
    }
    std::fs::write(out.join("Result.json"), result.to_json())?;
    Ok(result)
}

fn dump_into(
    record: &GameRecord,
    maps: &Path,
    out: &Path,
    every: u32,
    writer_threads: usize,
    result: &mut DumpResult,
) -> Result<()> {
    let tiles_dir = out.join("Tiles");
    std::fs::create_dir_all(&tiles_dir)?;
    let mut tile_writer = TileWriter::new(tiles_dir, writer_threads);
    let mut entities = std::io::BufWriter::new(std::fs::File::create(out.join("Entities.ndjson"))?);
    let mut io_error: Option<anyhow::Error> = None;
    let mut first_call = true;
    let mut write_snapshot = |game: &GameImpl, result: &mut DumpResult| -> Result<()> {
        let tick = game.ticks();
        if result.snapshot_ticks.last() == Some(&tick) {
            return Ok(());
        }
        tile_writer.submit(tick, tile_bytes(game))?;
        entities.write_all(capture_entities(game).as_bytes())?;
        result.snapshot_ticks.push(tick);
        Ok(())
    };
    let last_turn = record.turns.last().map(|t| t.turn_number);
    let outcome = replay_record(record, maps, None, &mut |runner, info| {
        let game = &runner.game;
        if first_call {
            first_call = false;
            result.width = game.width();
            result.height = game.height();
        }
        result.final_tick = game.ticks();
        if let (Some(expected), Some(actual)) = (info.turn.hash, info.engine_hash) {
            result.hashes_checked += 1;
            if expected != actual {
                result.status = "desync";
                result.first_mismatch_tick = Some(info.turn.turn_number);
                if let Err(e) = write_snapshot(game, result) {
                    io_error = Some(e);
                }
                return false;
            }
        }
        let due = game.ticks().is_multiple_of(every) || Some(info.turn.turn_number) == last_turn;
        if due && let Err(e) = write_snapshot(game, result) {
            io_error = Some(e);
            return false;
        }
        true
    });
    let written = tile_writer.finish();
    let outcome = outcome?;
    if let Some(e) = io_error {
        return Err(e);
    }
    written?;
    if let Stop::Panic { tick, message } = outcome.stop {
        result.status = "error";
        result.error = Some(format!("{message} (tick {tick})"));
    }
    entities.flush()?;
    Ok(())
}
