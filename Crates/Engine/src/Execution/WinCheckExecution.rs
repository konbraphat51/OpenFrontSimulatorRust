// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/WinCheckExecution.ts: declares the winner of FFA and team games.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Execution::Execution::Execution;
use crate::Game::Game::{GameMode, PlayerType, RankedType, Team, Tick};
use crate::Game::GameImpl::{GameImpl, Winner};

const HARD_TIME_LIMIT_SECONDS: f64 = 170.0 * 60.0;

/// Checks every 10th tick whether the game has been won.
pub struct WinCheckExecution {
    active: bool,
    checked_ranked_spawns: bool,
}

impl WinCheckExecution {
    /// Creates the execution.
    pub fn new() -> Self {
        Self {
            active: true,
            checked_ranked_spawns: false,
        }
    }

    fn check_ranked_2v2_cancelled(&mut self, game: &mut GameImpl) -> bool {
        if self.checked_ranked_spawns {
            return false;
        }
        self.checked_ranked_spawns = true;
        if game.config().game_config().ranked_type != Some(RankedType::TwoVTwo) {
            return false;
        }
        let spawned = game
            .all_players()
            .into_iter()
            .filter(|&p| {
                let p = game.player(p);
                p.player_type() == PlayerType::Human && p.has_spawned()
            })
            .count();
        let expected = game.config().game_config().max_players.unwrap_or(0) as usize;
        if spawned >= expected {
            return false;
        }
        game.set_winner(None);
        self.active = false;
        true
    }

    fn check_winner_ffa(&mut self, game: &mut GameImpl) {
        let mut sorted = game.players();
        // Stable sort, descending by tiles owned.
        sorted.sort_by(|&a, &b| {
            game.player(b)
                .num_tiles_owned()
                .cmp(&game.player(a).num_tiles_owned())
        });
        if sorted.is_empty() {
            return;
        }
        if game.config().game_config().ranked_type == Some(RankedType::OneVOne) {
            let humans: Vec<_> = sorted
                .iter()
                .copied()
                .filter(|&p| {
                    let p = game.player(p);
                    p.player_type() == PlayerType::Human && !p.is_disconnected()
                })
                .collect();
            if humans.len() == 1 {
                game.set_winner(Some(Winner::Player(humans[0])));
                self.active = false;
                return;
            }
        }
        let max = sorted[0];
        if self.has_won(game, game.player(max).num_tiles_owned() as f64) {
            game.set_winner(Some(Winner::Player(max)));
            self.active = false;
        }
    }

    fn has_won(&self, game: &GameImpl, tiles_owned: f64) -> bool {
        let time_elapsed = game.elapsed_game_seconds();
        if let Some(max_timer) = game.config().game_config().max_timer_value
            && time_elapsed >= max_timer as f64 * 60.0
        {
            return true;
        }
        if time_elapsed >= HARD_TIME_LIMIT_SECONDS {
            return true;
        }
        let tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
        tiles_owned * 100.0
            > tiles_without_fallout * game.config().percentage_tiles_owned_to_win(time_elapsed)
    }

    fn check_winner_team(&mut self, game: &mut GameImpl) {
        if game.config().game_config().ranked_type == Some(RankedType::TwoVTwo) {
            let mut teams_remaining: Vec<Team> = Vec::new();
            for p in game.players() {
                let player = game.player(p);
                if player.player_type() != PlayerType::Human || player.is_disconnected() {
                    continue;
                }
                if let Some(team) = player.team()
                    && !teams_remaining.contains(team)
                {
                    teams_remaining.push(team.clone());
                }
            }
            if teams_remaining.len() == 1 {
                game.set_winner(Some(Winner::Team(teams_remaining[0].clone())));
                self.active = false;
                return;
            }
        }
        let mut team_to_tiles: Vec<(Team, f64)> = Vec::new();
        for p in game.players() {
            let player = game.player(p);
            let Some(team) = player.team() else { continue };
            let tiles = player.num_tiles_owned() as f64;
            match team_to_tiles.iter_mut().find(|(t, _)| t == team) {
                Some(entry) => entry.1 += tiles,
                None => team_to_tiles.push((team.clone(), tiles)),
            }
        }
        // Stable sort, descending by tiles.
        team_to_tiles.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        if team_to_tiles.is_empty() {
            return;
        }
        let max = &team_to_tiles[0];
        if self.has_won(game, max.1) {
            if max.0 == "Bot" {
                return;
            }
            game.set_winner(Some(Winner::Team(max.0.clone())));
            self.active = false;
        }
    }
}

impl Default for WinCheckExecution {
    fn default() -> Self {
        Self::new()
    }
}

impl Execution for WinCheckExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        if !ticks.is_multiple_of(10) {
            return;
        }
        if self.check_ranked_2v2_cancelled(game) {
            return;
        }
        if game.config().game_config().game_mode == GameMode::FFA {
            self.check_winner_ffa(game);
        } else {
            self.check_winner_team(game);
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
