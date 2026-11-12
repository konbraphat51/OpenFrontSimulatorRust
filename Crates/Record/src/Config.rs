// SPDX-License-Identifier: AGPL-3.0-only
//! Port of `GameConfigSchema` (src/core/Schemas.ts): the game configuration inside a record.

use crate::wire_enum;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

wire_enum!(
    /// Upstream `GameType`.
    GameType { Singleplayer = "Singleplayer", Public = "Public", Private = "Private" }
);
wire_enum!(
    /// Upstream `GameMode`.
    GameMode { FreeForAll = "Free For All", Team = "Team" }
);
wire_enum!(
    /// Upstream `GameMapSize`.
    GameMapSize { Compact = "Compact", Normal = "Normal" }
);
wire_enum!(
    /// Upstream `Difficulty`.
    Difficulty { Easy = "Easy", Medium = "Medium", Hard = "Hard", Impossible = "Impossible" }
);
wire_enum!(
    /// Upstream `RankedType`.
    RankedType { OneVOne = "1v1", TwoVTwo = "2v2", FreeForAll = "ffa", ClanWars = "clanwars" }
);

/// `nations`: a count, or "default" / "disabled".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NationsConfig {
    /// Explicit nation count.
    Count(u32),
    /// "default" or "disabled".
    Mode(String),
}

/// `playerTeams` (`TeamCountConfig`): a team count, or "Duos" / "Trios" / "Quads" / "Humans Vs Nations".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TeamCountConfig {
    /// Explicit team count.
    Count(u32),
    /// Named preset.
    Preset(String),
}

/// `doomsdayClock` option of the config.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DoomsdayClockConfig {
    /// Whether the clock is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// "slow", "normal", "fast" or "veryfast".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `overtime` option of the config.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OvertimeConfig {
    /// Whether overtime is on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    /// Minutes of game time before overtime starts.
    #[serde(
        default,
        rename = "startMinutes",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_minutes: Option<u32>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `publicGameModifiers` of the config (all informational for replays; the engine reads the resolved config fields).
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PublicGameModifiers {
    /// Compact map.
    #[serde(default, rename = "isCompact", skip_serializing_if = "Option::is_none")]
    pub is_compact: Option<bool>,
    /// Random spawn.
    #[serde(
        default,
        rename = "isRandomSpawn",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_random_spawn: Option<bool>,
    /// Crowded lobby.
    #[serde(default, rename = "isCrowded", skip_serializing_if = "Option::is_none")]
    pub is_crowded: Option<bool>,
    /// Hard nations.
    #[serde(
        default,
        rename = "isHardNations",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_hard_nations: Option<bool>,
    /// Starting gold.
    #[serde(
        default,
        rename = "startingGold",
        skip_serializing_if = "Option::is_none"
    )]
    pub starting_gold: Option<u64>,
    /// Gold multiplier.
    #[serde(
        default,
        rename = "goldMultiplier",
        skip_serializing_if = "Option::is_none"
    )]
    pub gold_multiplier: Option<f64>,
    /// Alliances disabled.
    #[serde(
        default,
        rename = "isAlliancesDisabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_alliances_disabled: Option<bool>,
    /// Ports disabled.
    #[serde(
        default,
        rename = "isPortsDisabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_ports_disabled: Option<bool>,
    /// Nukes disabled.
    #[serde(
        default,
        rename = "isNukesDisabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_nukes_disabled: Option<bool>,
    /// SAMs disabled.
    #[serde(
        default,
        rename = "isSAMsDisabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_sams_disabled: Option<bool>,
    /// Peace time.
    #[serde(
        default,
        rename = "isPeaceTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_peace_time: Option<bool>,
    /// Water nukes.
    #[serde(
        default,
        rename = "isWaterNukes",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_water_nukes: Option<bool>,
    /// Doomsday clock.
    #[serde(
        default,
        rename = "isDoomsdayClock",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_doomsday_clock: Option<bool>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// `hostCheats` of the config.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct HostCheats {
    /// Infinite gold.
    #[serde(
        default,
        rename = "infiniteGold",
        skip_serializing_if = "Option::is_none"
    )]
    pub infinite_gold: Option<bool>,
    /// Infinite troops.
    #[serde(
        default,
        rename = "infiniteTroops",
        skip_serializing_if = "Option::is_none"
    )]
    pub infinite_troops: Option<bool>,
    /// Gold multiplier (null or absent means none).
    #[serde(
        default,
        rename = "goldMultiplier",
        skip_serializing_if = "Option::is_none"
    )]
    pub gold_multiplier: Option<f64>,
    /// Starting gold (null or absent means none).
    #[serde(
        default,
        rename = "startingGold",
        skip_serializing_if = "Option::is_none"
    )]
    pub starting_gold: Option<u64>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Port of `GameConfigSchema`. Optional-and-nullable upstream fields are `Option` here: null and absent are the same.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameConfig {
    /// `GameMapType` wire name (for example "Black Sea").
    #[serde(rename = "gameMap")]
    pub game_map: String,
    /// Difficulty of nations and tribes.
    pub difficulty: Difficulty,
    /// Donations of gold enabled.
    #[serde(rename = "donateGold")]
    pub donate_gold: bool,
    /// Donations of troops enabled.
    #[serde(rename = "donateTroops")]
    pub donate_troops: bool,
    /// Singleplayer, public or private.
    #[serde(rename = "gameType")]
    pub game_type: GameType,
    /// FFA or team.
    #[serde(rename = "gameMode")]
    pub game_mode: GameMode,
    /// Ranked type, only for ranked games.
    #[serde(
        default,
        rename = "rankedType",
        skip_serializing_if = "Option::is_none"
    )]
    pub ranked_type: Option<RankedType>,
    /// Normal or compact map.
    #[serde(rename = "gameMapSize")]
    pub game_map_size: GameMapSize,
    /// Doomsday clock settings.
    #[serde(
        default,
        rename = "doomsdayClock",
        skip_serializing_if = "Option::is_none"
    )]
    pub doomsday_clock: Option<DoomsdayClockConfig>,
    /// Overtime settings.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub overtime: Option<OvertimeConfig>,
    /// Modifiers a public lobby was created with.
    #[serde(
        default,
        rename = "publicGameModifiers",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_game_modifiers: Option<PublicGameModifiers>,
    /// Nation count or mode.
    pub nations: NationsConfig,
    /// Number of tribes (bots).
    pub bots: u32,
    /// Infinite gold.
    #[serde(rename = "infiniteGold")]
    pub infinite_gold: bool,
    /// Infinite troops.
    #[serde(rename = "infiniteTroops")]
    pub infinite_troops: bool,
    /// Instant build.
    #[serde(rename = "instantBuild")]
    pub instant_build: bool,
    /// Disable the navigation mesh.
    #[serde(
        default,
        rename = "disableNavMesh",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_nav_mesh: Option<bool>,
    /// Disable alliances.
    #[serde(
        default,
        rename = "disableAlliances",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_alliances: Option<bool>,
    /// The onboarding tutorial (v0.35.7 on).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tutorial: Option<bool>,
    /// Disable clan tags.
    #[serde(
        default,
        rename = "disableClanTags",
        skip_serializing_if = "Option::is_none"
    )]
    pub disable_clan_tags: Option<bool>,
    /// Live stats reporting.
    #[serde(
        default,
        rename = "liveStatsEnabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub live_stats_enabled: Option<bool>,
    /// Anonymize names.
    #[serde(
        default,
        rename = "anonymizeNames",
        skip_serializing_if = "Option::is_none"
    )]
    pub anonymize_names: Option<bool>,
    /// ClientIDs allowed to see real names.
    #[serde(
        default,
        rename = "nameReveals",
        skip_serializing_if = "Option::is_none"
    )]
    pub name_reveals: Option<Vec<String>>,
    /// Public ids allowed to see real names.
    #[serde(
        default,
        rename = "nameRevealPublicIds",
        skip_serializing_if = "Option::is_none"
    )]
    pub name_reveal_public_ids: Option<Vec<String>>,
    /// Water nukes.
    #[serde(
        default,
        rename = "waterNukes",
        skip_serializing_if = "Option::is_none"
    )]
    pub water_nukes: Option<bool>,
    /// Random spawn.
    #[serde(rename = "randomSpawn")]
    pub random_spawn: bool,
    /// Player cap.
    #[serde(
        default,
        rename = "maxPlayers",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_players: Option<u32>,
    /// Allowlist of public ids.
    #[serde(
        default,
        rename = "allowedPublicIds",
        skip_serializing_if = "Option::is_none"
    )]
    pub allowed_public_ids: Option<Vec<String>>,
    /// Trusted accounts only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trusted: Option<bool>,
    /// Game timer in minutes.
    #[serde(
        default,
        rename = "maxTimerValue",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_timer_value: Option<u32>,
    /// Alliance duration in minutes.
    #[serde(
        default,
        rename = "customAllianceDuration",
        skip_serializing_if = "Option::is_none"
    )]
    pub custom_alliance_duration: Option<u32>,
    /// Start delay in seconds.
    #[serde(
        default,
        rename = "startDelay",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_delay: Option<u32>,
    /// Spawn immunity in ticks.
    #[serde(
        default,
        rename = "spawnImmunityDuration",
        skip_serializing_if = "Option::is_none"
    )]
    pub spawn_immunity_duration: Option<u32>,
    /// Disabled `UnitType` wire names.
    #[serde(
        default,
        rename = "disabledUnits",
        skip_serializing_if = "Option::is_none"
    )]
    pub disabled_units: Option<Vec<String>>,
    /// Team count or preset.
    #[serde(
        default,
        rename = "playerTeams",
        skip_serializing_if = "Option::is_none"
    )]
    pub player_teams: Option<TeamCountConfig>,
    /// Gold multiplier.
    #[serde(
        default,
        rename = "goldMultiplier",
        skip_serializing_if = "Option::is_none"
    )]
    pub gold_multiplier: Option<f64>,
    /// Starting gold.
    #[serde(
        default,
        rename = "startingGold",
        skip_serializing_if = "Option::is_none"
    )]
    pub starting_gold: Option<u64>,
    /// Host cheats.
    #[serde(
        default,
        rename = "hostCheats",
        skip_serializing_if = "Option::is_none"
    )]
    pub host_cheats: Option<HostCheats>,
    /// Lobby pool (stripped from start info upstream; kept raw).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pool: Option<Value>,
    /// Unknown keys, so new builds still load.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
