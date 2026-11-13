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
use crate::Game::NationCreation::{create_nations_for_game, create_nations_v28};
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

/// `censorNameWithClanTag(username)`: profane names are replaced, a non-profane clan tag is kept in its original case.
fn censor_name_with_clan_tag(username: &str) -> String {
    let clan_tag = clan_tag_original_case(username);
    let name_without_clan = match &clan_tag {
        Some(tag) => js_trim(&username.replacen(&format!("[{tag}]"), "", 1)).to_string(),
        None => username.to_string(),
    };
    let clan_tag_is_profane = clan_tag.as_ref().is_some_and(|t| Obscenity::has_match(t));
    let name_is_profane = Obscenity::has_match(&name_without_clan);
    let censored = if name_is_profane {
        SHADOW_NAMES[simple_hash(&name_without_clan) as usize % SHADOW_NAMES.len()].to_string()
    } else {
        name_without_clan
    };
    match clan_tag {
        // An unobjectionable username is returned as typed (original case, spacing).
        Some(_) if !clan_tag_is_profane && !name_is_profane => username.to_string(),
        Some(tag) if !clan_tag_is_profane => format!("[{tag}] {censored}"),
        _ => censored,
    }
}

/// `sanitize(name)` of `Util.ts`: keeps letters, numbers, white space, emoji and `[`, `]`, `_`.
fn js_sanitize(name: &str) -> String {
    fn emoji(c: u32) -> bool {
        const RANGES: &[(u32, u32)] = &[
            (0x23, 0x23),
            (0x2a, 0x2a),
            (0x30, 0x39),
            (0xa9, 0xa9),
            (0xae, 0xae),
            (0x200d, 0x200d),
            (0x203c, 0x203c),
            (0x2049, 0x2049),
            (0x20e3, 0x20e3),
            (0x2122, 0x2122),
            (0x2139, 0x2139),
            (0x2194, 0x2199),
            (0x21a9, 0x21aa),
            (0x231a, 0x231b),
            (0x2328, 0x2328),
            (0x23cf, 0x23cf),
            (0x23e9, 0x23f3),
            (0x23f8, 0x23fa),
            (0x24c2, 0x24c2),
            (0x25aa, 0x25ab),
            (0x25b6, 0x25b6),
            (0x25c0, 0x25c0),
            (0x25fb, 0x25fe),
            (0x2600, 0x2604),
            (0x260e, 0x260e),
            (0x2611, 0x2611),
            (0x2614, 0x2615),
            (0x2618, 0x2618),
            (0x261d, 0x261d),
            (0x2620, 0x2620),
            (0x2622, 0x2623),
            (0x2626, 0x2626),
            (0x262a, 0x262a),
            (0x262e, 0x262f),
            (0x2638, 0x263a),
            (0x2640, 0x2640),
            (0x2642, 0x2642),
            (0x2648, 0x2653),
            (0x265f, 0x2660),
            (0x2663, 0x2663),
            (0x2665, 0x2666),
            (0x2668, 0x2668),
            (0x267b, 0x267b),
            (0x267e, 0x267f),
            (0x2692, 0x2697),
            (0x2699, 0x2699),
            (0x269b, 0x269c),
            (0x26a0, 0x26a1),
            (0x26a7, 0x26a7),
            (0x26aa, 0x26ab),
            (0x26b0, 0x26b1),
            (0x26bd, 0x26be),
            (0x26c4, 0x26c5),
            (0x26c8, 0x26c8),
            (0x26ce, 0x26cf),
            (0x26d1, 0x26d1),
            (0x26d3, 0x26d4),
            (0x26e9, 0x26ea),
            (0x26f0, 0x26f5),
            (0x26f7, 0x26fa),
            (0x26fd, 0x26fd),
            (0x2702, 0x2702),
            (0x2705, 0x2705),
            (0x2708, 0x270d),
            (0x270f, 0x270f),
            (0x2712, 0x2712),
            (0x2714, 0x2714),
            (0x2716, 0x2716),
            (0x271d, 0x271d),
            (0x2721, 0x2721),
            (0x2728, 0x2728),
            (0x2733, 0x2734),
            (0x2744, 0x2744),
            (0x2747, 0x2747),
            (0x274c, 0x274c),
            (0x274e, 0x274e),
            (0x2753, 0x2755),
            (0x2757, 0x2757),
            (0x2763, 0x2764),
            (0x2795, 0x2797),
            (0x27a1, 0x27a1),
            (0x27b0, 0x27b0),
            (0x27bf, 0x27bf),
            (0x2934, 0x2935),
            (0x2b05, 0x2b07),
            (0x2b1b, 0x2b1c),
            (0x2b50, 0x2b50),
            (0x2b55, 0x2b55),
            (0x3030, 0x3030),
            (0x303d, 0x303d),
            (0x3297, 0x3297),
            (0x3299, 0x3299),
            (0xfe0f, 0xfe0f),
            (0x1f004, 0x1f004),
            (0x1f0cf, 0x1f0cf),
            (0x1f170, 0x1f171),
            (0x1f17e, 0x1f17f),
            (0x1f18e, 0x1f18e),
            (0x1f191, 0x1f19a),
            (0x1f1e6, 0x1f1ff),
            (0x1f201, 0x1f202),
            (0x1f21a, 0x1f21a),
            (0x1f22f, 0x1f22f),
            (0x1f232, 0x1f23a),
            (0x1f250, 0x1f251),
            (0x1f300, 0x1f321),
            (0x1f324, 0x1f393),
            (0x1f396, 0x1f397),
            (0x1f399, 0x1f39b),
            (0x1f39e, 0x1f3f0),
            (0x1f3f3, 0x1f3f5),
            (0x1f3f7, 0x1f4fd),
            (0x1f4ff, 0x1f53d),
            (0x1f549, 0x1f54e),
            (0x1f550, 0x1f567),
            (0x1f56f, 0x1f570),
            (0x1f573, 0x1f57a),
            (0x1f587, 0x1f587),
            (0x1f58a, 0x1f58d),
            (0x1f590, 0x1f590),
            (0x1f595, 0x1f596),
            (0x1f5a4, 0x1f5a5),
            (0x1f5a8, 0x1f5a8),
            (0x1f5b1, 0x1f5b2),
            (0x1f5bc, 0x1f5bc),
            (0x1f5c2, 0x1f5c4),
            (0x1f5d1, 0x1f5d3),
            (0x1f5dc, 0x1f5de),
            (0x1f5e1, 0x1f5e1),
            (0x1f5e3, 0x1f5e3),
            (0x1f5e8, 0x1f5e8),
            (0x1f5ef, 0x1f5ef),
            (0x1f5f3, 0x1f5f3),
            (0x1f5fa, 0x1f64f),
            (0x1f680, 0x1f6c5),
            (0x1f6cb, 0x1f6d2),
            (0x1f6d5, 0x1f6d7),
            (0x1f6dc, 0x1f6e5),
            (0x1f6e9, 0x1f6e9),
            (0x1f6eb, 0x1f6ec),
            (0x1f6f0, 0x1f6f0),
            (0x1f6f3, 0x1f6fc),
            (0x1f7e0, 0x1f7eb),
            (0x1f7f0, 0x1f7f0),
            (0x1f90c, 0x1f93a),
            (0x1f93c, 0x1f945),
            (0x1f947, 0x1f9ff),
            (0x1fa70, 0x1fa7c),
            (0x1fa80, 0x1fa88),
            (0x1fa90, 0x1fabd),
            (0x1fabf, 0x1fac5),
            (0x1face, 0x1fadb),
            (0x1fae0, 0x1fae8),
            (0x1faf0, 0x1faf8),
            (0xe0020, 0xe007f),
        ];
        RANGES.iter().any(|&(lo, hi)| (lo..=hi).contains(&c))
    }
    name.chars()
        .filter(|&c| {
            c.is_alphabetic()
                || c.is_numeric()
                || c.is_whitespace()
                || c == '\u{feff}'
                || matches!(c, '[' | ']' | '_')
                || emoji(c as u32)
        })
        .collect()
}

/// `censorNameWithClanTag(username)` of v0.27 and v0.28: a good name is returned as it was written.
fn censor_name_keeping_good_names(username: &str, sanitize_first: bool) -> String {
    let sanitized = if sanitize_first {
        js_sanitize(username)
    } else {
        username.to_string()
    };
    let clan_tag = clan_tag_original_case(&sanitized);
    let name_without_clan = match &clan_tag {
        Some(tag) => js_trim(&sanitized.replacen(&format!("[{tag}]"), "", 1)).to_string(),
        None => sanitized.clone(),
    };
    let clan_tag_is_profane = clan_tag.as_ref().is_some_and(|t| Obscenity::has_match(t));
    let username_is_profane = Obscenity::has_match(&name_without_clan);
    let censored = if username_is_profane {
        SHADOW_NAMES[simple_hash(&name_without_clan) as usize % SHADOW_NAMES.len()].to_string()
    } else {
        name_without_clan
    };
    match clan_tag {
        Some(tag) if !clan_tag_is_profane => {
            if username_is_profane {
                format!("[{tag}] {censored}")
            } else {
                sanitized
            }
        }
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
    crate::Core::Util::set_legacy_emoji_table(!rules.has(Change::EmojiTableFactoryRow2522));
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
            } else if rules.has(Change::SanitizeThenFixProfane27_0) {
                // v0.27.0 and v0.27.1: a profane name (clan tag included) is replaced whole.
                let sanitized = js_sanitize(&p.username);
                if Obscenity::has_match(&sanitized) {
                    SHADOW_NAMES[simple_hash(&sanitized) as usize % SHADOW_NAMES.len()].to_string()
                } else {
                    sanitized
                }
            } else if rules.has(Change::CensorKeepsGoodNames27) {
                censor_name_keeping_good_names(
                    &p.username,
                    !rules.has(Change::NoUsernameSanitation2622),
                )
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

    let nations = if rules.has(Change::FakeHumanNations28) {
        create_nations_v28(&engine_config, &terrain.nations, &mut random)
    } else {
        create_nations_for_game(
            &engine_config,
            &terrain.nations,
            &terrain.additional_nations,
            humans.len(),
            rules.has(Change::NationNamesExpanded31),
            rules.has(Change::NationCreationConfigurable30),
            &mut random,
        )
    };

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
            && self.game.has(Change::SpawnPhaseTimer3199)
        {
            self.game
                .add_execution(Box::new(SpawnTimerExecution::new()));
        }
        let nations_last = !self.game.has(Change::NationSpawnRetry4117);
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
        if self.game.has(Change::DoomsdayClock4469)
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
