// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/SpawnExecution.ts: places a player on the map.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::PlayerExecution::PlayerExecution;
use super::TribeExecution::TribeExecution;
use super::Util::get_spawn_tiles;
use crate::Builds::Rules::{Change, Rules};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{GameType, PlayerInfo, PlayerType, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use ofsim_map::SpawnArea;

const MAX_SPAWN_TRIES: u32 = 1_000;
const RELAX_MIN_DIST_AT: u32 = 750;

struct Spawn {
    center: TileRef,
    tiles: Vec<TileRef>,
}

/// Spawns (or respawns) one player at a tile or at a random free spot.
pub struct SpawnExecution {
    random: PseudoRandom,
    active: bool,
    queued_during_spawn_phase: bool,
    player_info: PlayerInfo,
    /// The requested spawn tile, if any.
    pub tile: Option<TileRef>,
    from_intent: bool,
}

impl SpawnExecution {
    /// `new SpawnExecution(gameID, playerInfo, tile, fromIntent)`.
    pub fn new(
        game_id: &str,
        player_info: PlayerInfo,
        tile: Option<TileRef>,
        from_intent: bool,
        rules: &Rules,
    ) -> Self {
        let seed = simple_hash(&player_info.id) as f64 + simple_hash(game_id) as f64;
        Self {
            random: PseudoRandom::for_rules(rules, seed),
            active: true,
            queued_during_spawn_phase: false,
            player_info,
            tile,
            from_intent,
        }
    }

    fn get_spawn(&mut self, game: &GameImpl, center: Option<TileRef>) -> Option<Spawn> {
        if let Some(center) = center {
            let tiles = get_spawn_tiles(&game.map, center, false).unwrap_or_default();
            if tiles.is_empty() {
                return None;
            }
            return Some(Spawn { center, tiles });
        }
        let spawn_area = self.get_team_spawn_area(game);
        let mut tries = 0;
        while tries < MAX_SPAWN_TRIES {
            tries += 1;
            let center = self.rand_tile(game, spawn_area.as_ref());
            if !game.is_land(center) || game.has_owner(center) || game.map.is_border(center) {
                continue;
            }
            if tries <= RELAX_MIN_DIST_AT
                || !game.config().rules().has(Change::SpawnRelaxMinDist4921)
            {
                let min_dist = game.config().min_distance_between_players();
                let near = game
                    .all_players()
                    .into_iter()
                    .filter(|&p| game.player(p).id() != self.player_info.id)
                    .any(|p| match game.player(p).spawn_tile() {
                        None => false,
                        Some(st) => (game.manhattan_dist(st, center) as f64) < min_dist,
                    });
                if near {
                    continue;
                }
            }
            if let Some(tiles) = get_spawn_tiles(&game.map, center, true) {
                return Some(Spawn { center, tiles });
            }
        }
        None
    }

    fn rand_tile(&mut self, game: &GameImpl, area: Option<&SpawnArea>) -> TileRef {
        if let Some(a) = area {
            let x = self.random.next_int(a.x as f64, (a.x + a.width) as f64);
            let y = self.random.next_int(a.y as f64, (a.y + a.height) as f64);
            return game.tile_ref(x as i32, y as i32);
        }
        let x = self.random.next_int(0.0, game.width() as f64);
        let y = self.random.next_int(0.0, game.height() as f64);
        game.tile_ref(x as i32, y as i32)
    }

    fn get_team_spawn_area(&self, game: &GameImpl) -> Option<SpawnArea> {
        let player: SmallId = game.player_id(&self.player_info.id);
        let team = game.player(player).team()?.clone();
        game.team_spawn_area(&team)
    }
}

impl Execution for SpawnExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.queued_during_spawn_phase = game.in_spawn_phase();
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.active = false;

        if !game.config().rules().has(Change::SpawnPhaseTimer3199) && !game.in_spawn_phase() {
            return;
        }

        if let Some(t) = self.tile
            && !game.is_valid_ref(t)
        {
            return;
        }

        let player = match game.find_player(&self.player_info.id) {
            Some(p) => p,
            None => game.add_player(self.player_info.clone(), None),
        };

        if game.config().rules().has(Change::SpawnIntentGate4867) {
            if self.from_intent && !self.queued_during_spawn_phase {
                return;
            }
        } else if game.config().rules().has(Change::SpawnTeleportGuard4657)
            && !game.in_spawn_phase()
            && game.player(player).has_spawned()
        {
            return;
        }

        if game.config().is_random_spawn() && game.player(player).has_spawned() {
            return;
        }

        let prev_tiles: Vec<TileRef> = game.player(player).tiles().iter().collect();
        for &t in &prev_tiles {
            game.player_mut(player).relinquish(t);
        }
        let requested = if game.config().is_random_spawn()
            && game
                .config()
                .rules()
                .has(Change::RandomSpawnIgnoresTile4428)
        {
            None
        } else {
            self.tile
        };
        let spawn = self.get_spawn(game, requested);

        let Some(spawn) = spawn else {
            if game.config().rules().has(Change::SpawnRollback5321) {
                for &t in &prev_tiles {
                    game.player_mut(player).conquer(t);
                }
            }
            return;
        };

        for &t in &spawn.tiles {
            game.player_mut(player).conquer(t);
        }

        if !game.player(player).has_spawned() {
            game.add_execution(Box::new(PlayerExecution::new(player)));
            if game.player(player).player_type() == PlayerType::Bot {
                let tribe_id = game.player(player).id().to_string();
                game.add_execution(Box::new(TribeExecution::new(
                    player,
                    &tribe_id,
                    game.config().rules(),
                )));
            }
        }

        game.player_mut(player).set_spawn_tile(spawn.center);

        if game.config().game_config().game_type == GameType::Singleplayer
            && self.player_info.player_type == PlayerType::Human
        {
            game.end_spawn_phase();
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
