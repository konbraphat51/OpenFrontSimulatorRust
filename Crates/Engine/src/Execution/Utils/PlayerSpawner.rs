// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/PlayerSpawner.ts: random-spawn executions for every human.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

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
        ));
    }
    players
}
