// SPDX-License-Identifier: AGPL-3.0-only
//! Conversion of the record's `GameConfig` (ofsim-record) into the engine's `GameConfig`.

use crate::Configuration::Config::{
    DoomsdayClockSetting, GameConfig, HostCheats, NationsSetting, OvertimeSetting,
};
use crate::Game::Game::{
    Difficulty, DoomsdayClockSpeed, GameMapSize, GameMapType, GameMode, GameType, RankedType,
    TeamCountConfig, UnitType,
};
use anyhow::{Result, anyhow, bail};
use ofsim_record::Config as RecordConfig;

fn difficulty(d: &RecordConfig::Difficulty) -> Result<Difficulty> {
    Difficulty::from_str(d.as_str()).ok_or_else(|| anyhow!("unknown difficulty {}", d.as_str()))
}

/// Converts a record config; unknown enum values are errors (a build this engine does not know).
pub fn game_config_from_record(c: &RecordConfig::GameConfig) -> Result<GameConfig> {
    let game_map =
        GameMapType::from_str(&c.game_map).ok_or_else(|| anyhow!("unknown map {}", c.game_map))?;
    let ranked_type = match &c.ranked_type {
        None => None,
        Some(r) => Some(
            RankedType::from_str(r.as_str())
                .ok_or_else(|| anyhow!("unknown ranked type {}", r.as_str()))?,
        ),
    };
    let nations = match &c.nations {
        _ if c.disable_nations == Some(true) => NationsSetting::Disabled,
        RecordConfig::NationsConfig::Count(n) => NationsSetting::Count(*n),
        RecordConfig::NationsConfig::Mode(m) if m == "disabled" => NationsSetting::Disabled,
        RecordConfig::NationsConfig::Mode(m) if m == "default" => NationsSetting::Default,
        RecordConfig::NationsConfig::Mode(m) => bail!("unknown nations mode {m}"),
    };
    let player_teams = match &c.player_teams {
        None => None,
        Some(RecordConfig::TeamCountConfig::Count(n)) => Some(TeamCountConfig::Count(*n)),
        Some(RecordConfig::TeamCountConfig::Preset(p)) => Some(match p.as_str() {
            "Duos" => TeamCountConfig::Duos,
            "Trios" => TeamCountConfig::Trios,
            "Quads" => TeamCountConfig::Quads,
            "Humans Vs Nations" => TeamCountConfig::HumansVsNations,
            other => bail!("unknown team preset {other}"),
        }),
    };
    let disabled_units = match &c.disabled_units {
        None => None,
        Some(list) => Some(
            list.iter()
                .map(|u| UnitType::from_str(u).ok_or_else(|| anyhow!("unknown unit {u}")))
                .collect::<Result<Vec<_>>>()?,
        ),
    };
    let doomsday_clock = match &c.doomsday_clock {
        None => None,
        Some(d) => Some(DoomsdayClockSetting {
            enabled: d.enabled,
            speed: match &d.speed {
                None => None,
                Some(s) => Some(
                    DoomsdayClockSpeed::from_str(s)
                        .ok_or_else(|| anyhow!("unknown doomsday speed {s}"))?,
                ),
            },
        }),
    };
    Ok(GameConfig {
        game_map,
        difficulty: difficulty(&c.difficulty)?,
        donate_gold: c.donate_gold,
        donate_troops: c.donate_troops,
        game_type: GameType::from_str(c.game_type.as_str())
            .ok_or_else(|| anyhow!("unknown game type {}", c.game_type.as_str()))?,
        game_mode: GameMode::from_str(c.game_mode.as_str())
            .ok_or_else(|| anyhow!("unknown game mode {}", c.game_mode.as_str()))?,
        ranked_type,
        game_map_size: GameMapSize::from_str(c.game_map_size.as_str())
            .ok_or_else(|| anyhow!("unknown map size {}", c.game_map_size.as_str()))?,
        doomsday_clock,
        overtime: c.overtime.as_ref().map(|o| OvertimeSetting {
            enabled: o.enabled,
            start_minutes: o.start_minutes.map(|m| m as f64),
        }),
        nations,
        bots: c.bots,
        infinite_gold: c.infinite_gold,
        infinite_troops: c.infinite_troops,
        instant_build: c.instant_build,
        disable_nav_mesh: c.disable_nav_mesh,
        disable_alliances: c.disable_alliances,
        tutorial: c.tutorial,
        water_nukes: c.water_nukes,
        random_spawn: c.random_spawn,
        max_players: c.max_players,
        max_timer_value: c.max_timer_value,
        custom_alliance_duration: c.custom_alliance_duration,
        start_delay: c.start_delay,
        spawn_immunity_duration: c.spawn_immunity_duration,
        disabled_units,
        player_teams,
        gold_multiplier: c.gold_multiplier,
        starting_gold: c.starting_gold,
        host_cheats: c.host_cheats.as_ref().map(|h| HostCheats {
            infinite_gold: h.infinite_gold,
            infinite_troops: h.infinite_troops,
            gold_multiplier: h.gold_multiplier,
            starting_gold: h.starting_gold,
        }),
    })
}
