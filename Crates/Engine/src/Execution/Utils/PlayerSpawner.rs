// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/PlayerSpawner.ts: random-spawn executions for every human.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::SpawnExecution::SpawnExecution;
use crate::Game::Game::PlayerType;
use crate::Game::GameImpl::GameImpl;

/// `spawnPlayers()`: one `SpawnExecution` per human, in player order.
pub fn spawn_players(game: &GameImpl, game_id: &str) -> Vec<SpawnExecution> {
    let mut players = Vec::new();
    for p in game.all_players() {
        let player = game.player(p);
        if player.player_type() != PlayerType::Human {
            continue;
        }
        players.push(SpawnExecution::new(
            game_id,
            player.info().clone(),
            None,
            false,
            game.config().rules(),
        ));
    }
    players
}

/// `PlayerSpawner.spawnPlayers()` of v0.28.4: a random free land tile for every human, at least 30 apart.
pub fn spawn_players_v28(game: &GameImpl, game_id: &str) -> Vec<SpawnExecution> {
    const MAX_SPAWN_TRIES: u32 = 10_000;
    const MIN_SPAWN_DISTANCE: u32 = 30;
    let mut random = PseudoRandom::for_rules(game.config().rules(), simple_hash(game_id) as f64);
    let mut players: Vec<SpawnExecution> = Vec::new();
    for p in game.all_players() {
        let player = game.player(p);
        if player.player_type() != PlayerType::Human {
            continue;
        }
        let mut found = None;
        let mut tries = 0;
        while tries < MAX_SPAWN_TRIES {
            tries += 1;
            let x = random.next_int(0.0, game.width() as f64);
            let y = random.next_int(0.0, game.height() as f64);
            let tile = game.tile_ref(x as i32, y as i32);
            if !game.is_land(tile) || game.has_owner(tile) || game.map.is_border(tile) {
                continue;
            }
            let too_close = players.iter().any(|s| {
                s.tile
                    .is_some_and(|t| game.manhattan_dist(t, tile) < MIN_SPAWN_DISTANCE)
            });
            if too_close {
                continue;
            }
            found = Some(tile);
            break;
        }
        // The upstream TODO: a player without a spot is skipped
        let Some(tile) = found else { continue };
        players.push(SpawnExecution::new(
            game_id,
            player.info().clone(),
            Some(tile),
            false,
            game.config().rules(),
        ));
    }
    players
}
