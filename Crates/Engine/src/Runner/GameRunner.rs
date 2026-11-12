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
use crate::Core::Obscenity;
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

/// `getClanTagOriginalCase(name)`: the first `[TAG]` of 2 to 5 letters or digits, as written.
fn clan_tag_original_case(name: &str) -> Option<String> {
    let bytes: Vec<char> = name.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == '[' {
            let mut j = i + 1;
            while j < bytes.len() && bytes[j].is_ascii_alphanumeric() {
                j += 1;
            }
            let len = j - i - 1;
            if (2..=5).contains(&len) && j < bytes.len() && bytes[j] == ']' {
                return Some(bytes[i + 1..j].iter().collect::<String>());
            }
        }
        i += 1;
    }
    None
}

/// `getClanTag(name)` of builds before v0.31: the first `[TAG]`, upper-cased.
fn clan_tag_from_name(name: &str) -> Option<String> {
    clan_tag_original_case(name).map(|t| t.to_uppercase())
}

/// Replacement names of profane usernames (`shadowNames`).
const SHADOW_NAMES: [&str; 7] = [
    "NicePeopleOnly",
    "BeKindPlz",
    "LearningManners",
    "StayClassy",
    "BeNicer",
    "NeedHugs",
    "MakeFriends",
];

/// JS `String.prototype.trim` white space: Unicode White_Space and the byte order mark.
fn js_trim(s: &str) -> &str {
    s.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}')
}

/// `censorNameWithClanTag(username)`: profane names are replaced, a clan tag is upper-cased unless profane.
fn censor_name_with_clan_tag(username: &str) -> String {
    let clan_tag = clan_tag_original_case(username);
    let name_without_clan = match &clan_tag {
        Some(tag) => js_trim(&username.replacen(&format!("[{tag}]"), "", 1)).to_string(),
        None => username.to_string(),
    };
    let clan_tag_is_profane = clan_tag.as_ref().is_some_and(|t| Obscenity::has_match(t));
    let censored = if Obscenity::has_match(&name_without_clan) {
        SHADOW_NAMES[simple_hash(&name_without_clan) as usize % SHADOW_NAMES.len()].to_string()
    } else {
        name_without_clan
    };
    match clan_tag {
        Some(tag) if !clan_tag_is_profane => format!("[{}] {}", tag.to_uppercase(), censored),
        _ => censored,
    }
}

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
    let mut random = PseudoRandom::for_rules(config.rules(), simple_hash(&start.game_id) as f64);

    let humans: Vec<PlayerInfo> = start
        .players
        .iter()
        .map(|p| {
            // Builds before PR 3122 censor every name in the game runner (the tracer passes no client id).
            let name = if rules.has(Change::ServerUsernameCensoring3122) {
                p.username.clone()
            } else {
                censor_name_with_clan_tag(&p.username)
            };
            PlayerInfo::new(
                name.clone(),
                PlayerType::Human,
                Some(p.client_id.clone()),
                random.next_id(),
                p.is_lobby_creator.unwrap_or(false),
                if rules.has(Change::ClanTagField31) {
                    p.clan_tag.clone()
                } else {
                    clan_tag_from_name(&name)
                },
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
        rules.has(Change::NationNamesExpanded31),
        rules.has(Change::NationCreationConfigurable30),
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
        if self.game.config().game_config().game_type != GameType::Singleplayer
            && self.game.config().rules().has(Change::SpawnPhaseTimer3199)
        {
            self.game
                .add_execution(Box::new(SpawnTimerExecution::new()));
        }
        let nations_last = !self.game.config().rules().has(Change::NationSpawnRetry4117);
        if !nations_last && self.game.config().spawn_nations() {
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
        if nations_last && self.game.config().spawn_nations() {
            for e in self.executor.nation_executions(&self.game) {
                self.game.add_execution(e);
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
