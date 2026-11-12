// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/GameRunner.ts: builds a game from start info and feeds it turns.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Only the simulation path is ported (`createGameRunner`, `init`, `addTurn`, `executeNextTick`).
//! The name placement (`placeName`, `placeSpawnName`), the update packing, the callbacks and the
//! player query endpoints (`playerActions`, `playerBorderTiles`, ...) only feed the client and
//! never write simulation state. A tick that throws upstream is reported by the caller through
//! `catch_unwind`.

use super::ConfigConversion::game_config_from_record;
use super::Executor::Executor;
use crate::Builds::Rules::{Change, Rules};
use crate::Configuration::Config::Config;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::DoomsdayClockExecution::DoomsdayClockExecution;
use crate::Execution::RecomputeRailClusterExecution::RecomputeRailClusterExecution;
use crate::Execution::SpawnTimerExecution::SpawnTimerExecution;
use crate::Execution::WinCheckExecution::WinCheckExecution;
use crate::Game::Game::{GameType, PlayerInfo, PlayerType, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::GameMapImpl;
use crate::Game::NationCreation::create_nations_for_game;
use anyhow::Result;
use ofsim_map::{MapData, MapSize, TerrainMap};
use ofsim_record::Records::Turn;
use ofsim_record::StartInfo::GameStartInfo;
use std::path::Path;

fn game_map_of(t: &TerrainMap, rules: &Rules) -> Result<GameMapImpl> {
    let mut map = GameMapImpl::new(t.width, t.height, t.data.clone(), t.num_land_tiles as i64)
        .map_err(|e| anyhow::anyhow!(e))?;
    map.set_build_behavior(
        rules.has(Change::ImpassableTerrain4340),
        !rules.has(Change::NeighborOrderNswe4495),
    );
    Ok(map)
}

/// A game plus the executor that turns recorded turns into executions.
pub struct GameRunner {
    /// The game.
    pub game: GameImpl,
    executor: Executor,
    turns: Vec<Turn>,
    curr_turn: usize,
}

/// `createGameRunner`: loads the map of `maps_root`, creates players and nations, calls `init`.
pub fn create_game_runner(
    start: &GameStartInfo,
    maps_root: &Path,
    rules: Rules,
) -> Result<GameRunner> {
    let engine_config = game_config_from_record(&start.config)?;
    let config = Config::new(engine_config.clone(), false, rules.clone());
    let size = MapSize::from_wire(start.config.game_map_size.as_str())?;
    let terrain = MapData::load(maps_root, &start.config.game_map, size)?;
    let mut random = PseudoRandom::new(simple_hash(&start.game_id) as f64);

    let humans: Vec<PlayerInfo> = start
        .players
        .iter()
        .map(|p| {
            PlayerInfo::new(
                p.username.clone(),
                PlayerType::Human,
                Some(p.client_id.clone()),
                random.next_id(),
                p.is_lobby_creator.unwrap_or(false),
                p.clan_tag.clone(),
                p.friends.clone().unwrap_or_default(),
                p.team_index,
                None,
            )
        })
        .collect();

    let nations = create_nations_for_game(
        &engine_config,
        &terrain.nations,
        &terrain.additional_nations,
        humans.len(),
        &mut random,
    );

    let game = GameImpl::new(
        humans,
        nations,
        game_map_of(&terrain.game_map, &rules)?,
        game_map_of(&terrain.mini_game_map, &rules)?,
        config,
        terrain.team_game_spawn_areas.clone(),
    );
    let tribe_names = start
        .tribes
        .as_ref()
        .map(|t| t.iter().map(|x| x.name.clone()).collect())
        .unwrap_or_default();
    let mut runner = GameRunner {
        game,
        executor: Executor::new(start.game_id.clone(), tribe_names),
        turns: Vec::new(),
        curr_turn: 0,
    };
    runner.init();
    Ok(runner)
}

impl GameRunner {
    /// `init()`: the executions every game starts with.
    pub fn init(&mut self) {
        if self.game.config().game_config().game_type != GameType::Singleplayer {
            self.game
                .add_execution(Box::new(SpawnTimerExecution::new()));
        }
        if self.game.config().spawn_nations() {
            for e in self.executor.nation_executions(&self.game) {
                self.game.add_execution(e);
            }
        }
        if self.game.config().is_random_spawn() {
            for e in self.executor.spawn_players(&self.game) {
                self.game.add_execution(Box::new(e));
            }
        }
        if self.game.config().bots() > 0 {
            let n = self.game.config().bots() as usize;
            for e in self.executor.spawn_tribes(&self.game, n) {
                self.game.add_execution(Box::new(e));
            }
        }
        self.game.add_execution(Box::new(WinCheckExecution::new()));
        if self.game.config().rules().has(Change::DoomsdayClock4469)
            && self.game.config().doomsday_clock_config().enabled
        {
            self.game
                .add_execution(Box::new(DoomsdayClockExecution::new()));
        }
        if !self.game.config().is_unit_disabled(UnitType::Factory) {
            self.game
                .add_execution(Box::new(RecomputeRailClusterExecution));
        }
    }

    /// `addTurn(turn)`.
    pub fn add_turn(&mut self, turn: Turn) {
        self.turns.push(turn);
    }

    /// Number of turns queued and not yet executed.
    pub fn pending_turns(&self) -> usize {
        self.turns.len() - self.curr_turn
    }

    /// `executeNextTick()`: runs the next queued turn. Returns `None` when no turn is queued,
    /// otherwise the hash the game computed this tick (`Some(None)` when it computes none).
    pub fn execute_next_tick(&mut self) -> Option<Option<f64>> {
        if self.curr_turn >= self.turns.len() {
            return None;
        }
        let execs = self
            .executor
            .create_execs(&self.game, &self.turns[self.curr_turn]);
        for e in execs {
            self.game.add_execution(e);
        }
        self.curr_turn += 1;
        Some(self.game.execute_next_tick())
    }
}
