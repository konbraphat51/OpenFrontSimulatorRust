// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts and src/core/configuration/DefaultConfig.ts (the baseline `Config` class; part: core).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Methods that took a `Player`/`PlayerView` upstream take the few fields they read instead
//! (player type, lobby-creator flag, tiles owned, troops, city levels). The `userSettings`,
//! `listed` and spectator accessors are client-only and not ported. Throwing paths panic with the
//! upstream message; the runner reports them as tick errors.

use crate::Builds::Rules::{Change, Rules};
use crate::Compat::HostMath::{self, PowKind};
use crate::Compat::JsNumber::{max as js_max, min as js_min};
use crate::Core::{DetMath, Util};
use crate::Game::Game::{
    Difficulty, DoomsdayClockSpeed, GameMapSize, GameMapType, GameMode, GameType, Gold, PlayerType,
    RankedType, SamLauncherState, TeamCountConfig, TerrainType, Tick, UnitInfo, UnitType,
};

mod Combat;
mod Economy;
mod Nukes;
mod Population;
mod Settings;
mod Units;

/// Host cheats of a private lobby.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HostCheats {
    /// Lobby creator has infinite gold.
    pub infinite_gold: Option<bool>,
    /// Lobby creator has infinite troops.
    pub infinite_troops: Option<bool>,
    /// Lobby creator's gold multiplier (0 or absent = unused).
    pub gold_multiplier: Option<f64>,
    /// Extra starting gold for the lobby creator.
    pub starting_gold: Option<u64>,
}

/// Wire form of the doomsday clock setting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct DoomsdayClockSetting {
    /// Enabled flag.
    pub enabled: Option<bool>,
    /// Speed preset.
    pub speed: Option<DoomsdayClockSpeed>,
}

/// Wire form of the overtime setting.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct OvertimeSetting {
    /// Enabled flag.
    pub enabled: Option<bool>,
    /// Minutes before the bar starts dropping.
    pub start_minutes: Option<f64>,
}

/// `nations` field of the game config.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NationsSetting {
    /// Exactly this many nations.
    Count(u32),
    /// Map default.
    Default,
    /// No nations.
    Disabled,
}

/// The simulation-relevant fields of upstream `GameConfig`.
#[derive(Clone, Debug, PartialEq)]
pub struct GameConfig {
    /// Map.
    pub game_map: GameMapType,
    /// Nation/bot difficulty.
    pub difficulty: Difficulty,
    /// Donations of gold allowed.
    pub donate_gold: bool,
    /// Donations of troops allowed.
    pub donate_troops: bool,
    /// Game type.
    pub game_type: GameType,
    /// Game mode.
    pub game_mode: GameMode,
    /// Ranked kind.
    pub ranked_type: Option<RankedType>,
    /// Map size.
    pub game_map_size: GameMapSize,
    /// Doomsday clock setting.
    pub doomsday_clock: Option<DoomsdayClockSetting>,
    /// Overtime setting.
    pub overtime: Option<OvertimeSetting>,
    /// Nation count setting.
    pub nations: NationsSetting,
    /// Number of bots.
    pub bots: u32,
    /// Infinite gold for humans.
    pub infinite_gold: bool,
    /// Infinite troops for humans.
    pub infinite_troops: bool,
    /// Instant construction.
    pub instant_build: bool,
    /// Navigation mesh disabled.
    pub disable_nav_mesh: Option<bool>,
    /// Legacy alliance switch.
    pub disable_alliances: Option<bool>,
    /// The onboarding tutorial.
    pub tutorial: Option<bool>,
    /// Water nukes.
    pub water_nukes: Option<bool>,
    /// Random spawn.
    pub random_spawn: bool,
    /// Maximum players.
    pub max_players: Option<u32>,
    /// Game timer in minutes.
    pub max_timer_value: Option<u32>,
    /// Alliance duration in minutes (0 disables alliances).
    pub custom_alliance_duration: Option<u32>,
    /// Start delay in seconds.
    pub start_delay: Option<u32>,
    /// Spawn immunity in ticks.
    pub spawn_immunity_duration: Option<u32>,
    /// Disabled unit types.
    pub disabled_units: Option<Vec<UnitType>>,
    /// Team setting.
    pub player_teams: Option<TeamCountConfig>,
    /// Global gold multiplier.
    pub gold_multiplier: Option<f64>,
    /// Starting gold.
    pub starting_gold: Option<u64>,
    /// Host cheats.
    pub host_cheats: Option<HostCheats>,
}

/// Resolved doomsday clock tunables (`DOOMSDAY_CLOCK_DEFAULTS` merged with the setting).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DoomsdayClockConfig {
    /// Clock active.
    pub enabled: bool,
    /// Speed preset.
    pub speed: DoomsdayClockSpeed,
    /// Warning cooldown before decay, seconds.
    pub warn_seconds: f64,
    /// Initial drain percent.
    pub drain_start_percent: f64,
    /// Maximum drain percent.
    pub drain_max_percent: f64,
    /// Seconds to ramp drain linearly to the maximum.
    pub drain_ramp_seconds: f64,
    /// Final troop floor, percent of max.
    pub drain_floor_percent: f64,
    /// Initial troop floor, percent of max.
    pub floor_start_percent: f64,
    /// Seconds for the floor to decay to its final value.
    pub floor_decay_seconds: f64,
    /// Seconds from the skull to territory death (0 disables rot).
    pub rot_death_seconds: f64,
    /// Seconds of grainy opening of the rot.
    pub rot_grain_seconds: f64,
    /// Percent of territory speckled in the opening.
    pub rot_speckle_percent: f64,
    /// Warship drain start percent.
    pub warship_drain_start_percent: f64,
    /// Warship drain maximum percent.
    pub warship_drain_max_percent: f64,
    /// Warship drain curve exponent.
    pub warship_drain_curve_exponent: f64,
}

/// Resolved overtime tunables.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OvertimeConfig {
    /// Overtime active.
    pub enabled: bool,
    /// Minutes before the bar starts dropping.
    pub start_minutes: f64,
    /// Percentage points the bar drops per minute.
    pub drop_percent_per_minute: f64,
}

const DOOMSDAY_DEFAULTS: DoomsdayClockConfig = DoomsdayClockConfig {
    enabled: false,
    speed: DoomsdayClockSpeed::Normal,
    warn_seconds: 30.0,
    drain_start_percent: 2.0,
    drain_max_percent: 5.0,
    drain_ramp_seconds: 90.0,
    drain_floor_percent: 5.0,
    floor_start_percent: 40.0,
    floor_decay_seconds: 90.0,
    rot_death_seconds: 150.0,
    rot_grain_seconds: 10.0,
    rot_speckle_percent: 15.0,
    warship_drain_start_percent: 1.0,
    warship_drain_max_percent: 50.0,
    warship_drain_curve_exponent: 8.0,
};

const OVERTIME_DEFAULTS: OvertimeConfig = OvertimeConfig {
    enabled: false,
    start_minutes: 30.0,
    drop_percent_per_minute: 2.0,
};

/// Share of the land a side must hold to win.
const PERCENT_TILES_OWNED_TO_WIN: f64 = 80.0;
const DEFAULT_SPAWN_IMMUNITY_TICKS: u32 = 5 * 10;
/// SAM launcher construction duration in ticks (non-instant-build).
pub const SAM_CONSTRUCTION_TICKS: u32 = 30 * 10;

const LARGE_TERRITORY_MIDPOINT: f64 = 300_000.0;
const LARGE_TERRITORY_STEEPNESS: f64 = 2.5;
const LARGE_ATTACKER_DEPTH: f64 = 0.7;
const LARGE_DEFENDER_DEPTH: f64 = 0.3;
const BOT_DEFENDER_LOSS_MULT: f64 = 0.7;
const TERRA_NULLIUS_COST_SCALE: f64 = 2000.0;
const TERRA_NULLIUS_MIN_COST: f64 = 5.0;
const TERRA_NULLIUS_MAX_COST: f64 = 100.0;
const ATTACKER_LOSS_BASE: f64 = 0.463;
const ATTACKER_LOSS_PER_DENSITY: f64 = 0.0039;
const SPEED_COST_DIVISOR: f64 = 8.55;
/// Speed divisor before PR 5425.
const SPEED_COST_DIVISOR_OLD: f64 = 7.77;
const LARGE_ATTACKER_SPEED_DEPTH: f64 = 0.73;
const DEFENSE_DEBUFF_MIDPOINT: f64 = 150_000.0;
const DEFENSE_DEBUFF_DECAY_RATE: f64 = std::f64::consts::LN_2 / 50000.0;

/// Attacker side of `attackLogic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackLogicAttacker {
    /// Attacker kind.
    pub player_type: PlayerType,
    /// Tiles the attacker owns.
    pub num_tiles: f64,
}

/// Defender side of `attackLogic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackLogicDefender {
    /// Defender kind.
    pub player_type: PlayerType,
    /// Tiles the defender owns.
    pub num_tiles: f64,
    /// Defender troops.
    pub troops: f64,
    /// Defender is a traitor.
    pub is_traitor: bool,
    /// Defender is disconnected and on the attacker's team.
    pub is_disconnected_teammate: bool,
}

/// Input of `attackLogic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackLogicInput {
    /// Terrain of the tile.
    pub terrain: TerrainType,
    /// Troops committed by the attack.
    pub attack_troops: f64,
    /// Attacker.
    pub attacker: AttackLogicAttacker,
    /// Defender, `None` when attacking terra nullius.
    pub defender: Option<AttackLogicDefender>,
    /// A defender defense post covers the tile.
    pub defender_has_defense_post: bool,
    /// Fraction of land tiles with fallout, when the tile has fallout.
    pub fallout_ratio: Option<f64>,
    /// Tiles on the attack front this tick.
    pub border_size: f64,
}

/// Output of `attackLogic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AttackLogicResult {
    /// Troops the attacker loses.
    pub attacker_troop_loss: f64,
    /// Troops the defender loses.
    pub defender_troop_loss: f64,
    /// Share of the tick's conquest budget the tile consumes.
    pub tick_fraction: f64,
}

/// Output of the pre-rework `attackLogic`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LegacyAttackResult {
    /// Troops the attacker loses.
    pub attacker_troop_loss: f64,
    /// Troops the defender loses.
    pub defender_troop_loss: f64,
    /// Budget (in tiles per tick units) the tile consumes.
    pub tiles_per_tick_used: f64,
}

/// Inner and outer blast radius of a nuke.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NukeMagnitude {
    /// Inner radius.
    pub inner: f64,
    /// Outer radius.
    pub outer: f64,
}

/// Relation of the train's destination owner to the train's owner (`trainGold`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrainRelation {
    /// Own station.
    Self_,
    /// Teammate.
    Team,
    /// Ally.
    Ally,
    /// Anyone else.
    Other,
}

/// Player fields `maxTroops` and `troopIncreaseRate` read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TroopInput {
    /// Player kind.
    pub player_type: PlayerType,
    /// Lobby creator (host cheats apply).
    pub is_lobby_creator: bool,
    /// `numTilesOwned()`.
    pub num_tiles_owned: f64,
    /// Sum of levels of finished cities.
    pub city_level_sum: f64,
    /// Current troops (`troopIncreaseRate` only).
    pub troops: f64,
}

/// What the unit cost formulas read about the buying player.
pub struct CostContext<'a> {
    /// Player kind.
    pub player_type: PlayerType,
    /// Lobby creator (host cheats apply).
    pub is_lobby_creator: bool,
    /// `min(unitsOwned(t), unitsConstructed(t))` for a unit type.
    pub owned_and_constructed: &'a dyn Fn(UnitType) -> i64,
    /// Extra units priced in (bulk purchases).
    pub extra_units: i64,
    /// `stats().numMirvsLaunched()`.
    pub mirvs_launched: i64,
}

/// `Math.pow` of pre-DetMath builds; `OFSIM_POW=fdlibm|platform` overrides it for experiments.
fn default_pow_kind() -> PowKind {
    match std::env::var("OFSIM_POW").as_deref() {
        Ok("fdlibm") => PowKind::Fdlibm,
        _ => PowKind::Platform,
    }
}

/// Build configuration: tunables and formulas of one upstream build.
#[derive(Clone, Debug)]
pub struct Config {
    game_config: GameConfig,
    is_replay: bool,
    rules: Rules,
    pow_kind: PowKind,
}

impl Config {
    /// Creates the configuration of a game for a build.
    pub fn new(game_config: GameConfig, is_replay: bool, rules: Rules) -> Self {
        Self {
            game_config,
            is_replay,
            rules,
            pow_kind: default_pow_kind(),
        }
    }

    /// The game config.
    pub fn game_config(&self) -> &GameConfig {
        &self.game_config
    }

    /// The build rules.
    pub fn rules(&self) -> &Rules {
        &self.rules
    }

    /// Whether the build contains the upstream change `change`.
    pub fn has(&self, change: Change) -> bool {
        self.rules.has(change)
    }

    /// Whether this run is a replay.
    pub fn is_replay(&self) -> bool {
        self.is_replay
    }

    /// `Math.pow` implementation of pre-DetMath builds (see `HostMath`).
    pub fn pow_kind(&self) -> PowKind {
        self.pow_kind
    }

    fn exp(&self, x: f64) -> f64 {
        if self.rules.has(Change::DetMath5191) {
            DetMath::exp(x)
        } else {
            HostMath::exp(x)
        }
    }

    fn log(&self, x: f64) -> f64 {
        if self.rules.has(Change::DetMath5191) {
            DetMath::log(x)
        } else {
            HostMath::log(x)
        }
    }

    /// `Math.pow(x, y)` of the build.
    pub fn pow(&self, x: f64, y: f64) -> f64 {
        if self.rules.has(Change::DetMath5191) {
            DetMath::pow(x, y)
        } else {
            HostMath::pow(self.pow_kind, x, y)
        }
    }

    fn pow2(&self, n: f64) -> f64 {
        if self.rules.has(Change::DetMath5191) {
            DetMath::pow2(n)
        } else {
            HostMath::pow(self.pow_kind, 2.0, n)
        }
    }

    fn sigmoid(&self, value: f64, decay_rate: f64, midpoint: f64) -> f64 {
        1.0 / (1.0 + self.exp(-decay_rate * (value - midpoint)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn hex(v: &Value) -> f64 {
        f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
    }

    fn fl(v: &Value) -> f64 {
        hex(&v["f"])
    }

    fn game_config(j: &Value) -> GameConfig {
        let opt_bool = |k: &str| j[k].as_bool();
        let host_cheats = j.get("hostCheats").map(|h| HostCheats {
            infinite_gold: h["infiniteGold"].as_bool(),
            infinite_troops: h["infiniteTroops"].as_bool(),
            gold_multiplier: h["goldMultiplier"].as_f64(),
            starting_gold: h["startingGold"].as_u64(),
        });
        GameConfig {
            game_map: GameMapType::from_str(j["gameMap"].as_str().unwrap()).unwrap(),
            difficulty: Difficulty::from_str(j["difficulty"].as_str().unwrap()).unwrap(),
            donate_gold: j["donateGold"].as_bool().unwrap(),
            donate_troops: j["donateTroops"].as_bool().unwrap(),
            game_type: GameType::from_str(j["gameType"].as_str().unwrap()).unwrap(),
            game_mode: GameMode::from_str(j["gameMode"].as_str().unwrap()).unwrap(),
            ranked_type: None,
            game_map_size: GameMapSize::from_str(j["gameMapSize"].as_str().unwrap()).unwrap(),
            doomsday_clock: j.get("doomsdayClock").map(|d| DoomsdayClockSetting {
                enabled: d["enabled"].as_bool(),
                speed: d["speed"].as_str().and_then(DoomsdayClockSpeed::from_str),
            }),
            overtime: j.get("overtime").map(|o| OvertimeSetting {
                enabled: o["enabled"].as_bool(),
                start_minutes: o["startMinutes"].as_f64(),
            }),
            nations: match &j["nations"] {
                Value::Number(n) => NationsSetting::Count(n.as_u64().unwrap() as u32),
                Value::String(s) if s == "disabled" => NationsSetting::Disabled,
                _ => NationsSetting::Default,
            },
            bots: j["bots"].as_u64().unwrap() as u32,
            infinite_gold: j["infiniteGold"].as_bool().unwrap(),
            infinite_troops: j["infiniteTroops"].as_bool().unwrap(),
            instant_build: j["instantBuild"].as_bool().unwrap(),
            disable_nav_mesh: opt_bool("disableNavMesh"),
            disable_alliances: opt_bool("disableAlliances"),
            tutorial: opt_bool("tutorial"),
            water_nukes: opt_bool("waterNukes"),
            random_spawn: j["randomSpawn"].as_bool().unwrap(),
            max_players: None,
            max_timer_value: None,
            custom_alliance_duration: j["customAllianceDuration"].as_u64().map(|x| x as u32),
            start_delay: None,
            spawn_immunity_duration: j["spawnImmunityDuration"].as_u64().map(|x| x as u32),
            disabled_units: j["disabledUnits"].as_array().map(|a| {
                a.iter()
                    .map(|u| UnitType::from_str(u.as_str().unwrap()).unwrap())
                    .collect()
            }),
            player_teams: match &j["playerTeams"] {
                Value::Number(n) => Some(TeamCountConfig::Count(n.as_u64().unwrap() as u32)),
                Value::String(s) => Some(match s.as_str() {
                    "Duos" => TeamCountConfig::Duos,
                    "Trios" => TeamCountConfig::Trios,
                    "Quads" => TeamCountConfig::Quads,
                    _ => TeamCountConfig::HumansVsNations,
                }),
                _ => None,
            },
            gold_multiplier: j["goldMultiplier"].as_f64(),
            starting_gold: j["startingGold"].as_u64(),
            host_cheats,
        }
    }

    fn ptype(v: &Value) -> PlayerType {
        PlayerType::from_str(v.as_str().unwrap()).unwrap()
    }

    fn utype(v: &Value) -> UnitType {
        UnitType::from_str(v.as_str().unwrap()).unwrap()
    }

    fn check_f(got: f64, want: &Value, ctx: &str) {
        let w = fl(want);
        assert!(
            got.to_bits() == w.to_bits() || (got.is_nan() && w.is_nan()),
            "{ctx}: got {got:e} want {w:e}"
        );
    }

    fn check_i(got: i64, want: &Value, ctx: &str) {
        assert_eq!(got.to_string(), want["i"].as_str().unwrap(), "{ctx}");
    }

    #[test]
    fn matches_upstream_goldens() {
        let g: Value =
            serde_json::from_str(include_str!("../../Tests/Golden/Config.json")).unwrap();
        let configs: Vec<Config> = g["cfgs"]
            .as_array()
            .unwrap()
            .iter()
            .map(|j| Config::new(game_config(j), false, Rules::baseline()))
            .collect();
        let mut seen = std::collections::BTreeSet::new();
        for row in g["out"].as_array().unwrap() {
            let c = &configs[row["cfg"].as_u64().unwrap() as usize];
            let name = row["fn"].as_str().unwrap();
            let a = row["args"].as_array().unwrap();
            let want = &row["out"];
            let ctx = format!("cfg {} {name} {a:?}", row["cfg"]);
            seen.insert(name.to_string());
            let nuke = |u: UnitType| {
                matches!(
                    u,
                    UnitType::AtomBomb
                        | UnitType::HydrogenBomb
                        | UnitType::MIRV
                        | UnitType::MIRVWarhead
                )
            };
            match name {
                "traitorDefenseDebuff" => check_f(c.traitor_defense_debuff(), want, &ctx),
                "traitorSpeedDebuff" => check_f(c.traitor_speed_debuff(), want, &ctx),
                "traitorDuration" => check_f(c.traitor_duration() as f64, want, &ctx),
                "teamLandShareWinThresholdTenths" => {
                    check_f(c.team_land_share_win_threshold_tenths() as f64, want, &ctx)
                }
                "spawnImmunityDuration" => check_f(c.spawn_immunity_duration() as f64, want, &ctx),
                "nationSpawnImmunityDuration" => {
                    check_f(c.nation_spawn_immunity_duration() as f64, want, &ctx)
                }
                "cityTroopIncrease" => check_f(c.city_troop_increase(), want, &ctx),
                "msPerTick" => check_f(c.ms_per_tick() as f64, want, &ctx),
                "SAMCooldown" => check_f(c.sam_cooldown() as f64, want, &ctx),
                "SiloCooldown" => check_f(c.silo_cooldown() as f64, want, &ctx),
                "defensePostRange" => check_f(c.defense_post_range(), want, &ctx),
                "defensePostDefenseBonus" => check_f(c.defense_post_defense_bonus(), want, &ctx),
                "defensePostSpeedBonus" => check_f(c.defense_post_speed_bonus(), want, &ctx),
                "spawnNations" => assert_eq!(c.spawn_nations(), want.as_bool().unwrap(), "{ctx}"),
                "bots" => check_f(c.bots() as f64, want, &ctx),
                "instantBuild" => assert_eq!(c.instant_build(), want.as_bool().unwrap(), "{ctx}"),
                "disableNavMesh" => {
                    assert_eq!(c.disable_nav_mesh(), want.as_bool().unwrap(), "{ctx}")
                }
                "disableAlliances" => {
                    assert_eq!(c.disable_alliances(), want.as_bool().unwrap(), "{ctx}")
                }
                "waterNukes" => assert_eq!(c.water_nukes(), want.as_bool().unwrap(), "{ctx}"),
                "isRandomSpawn" => {
                    assert_eq!(c.is_random_spawn(), want.as_bool().unwrap(), "{ctx}")
                }
                "infiniteGold" => assert_eq!(c.infinite_gold(), want.as_bool().unwrap(), "{ctx}"),
                "donateGold" => assert_eq!(c.donate_gold(), want.as_bool().unwrap(), "{ctx}"),
                "infiniteTroops" => {
                    assert_eq!(c.infinite_troops(), want.as_bool().unwrap(), "{ctx}")
                }
                "donateTroops" => assert_eq!(c.donate_troops(), want.as_bool().unwrap(), "{ctx}"),
                "goldMultiplier" => check_f(c.gold_multiplier(), want, &ctx),
                "trainStationMinRange" => check_f(c.train_station_min_range(), want, &ctx),
                "trainStationMaxRange" => check_f(c.train_station_max_range(), want, &ctx),
                "railroadMaxSize" => check_f(c.railroad_max_size(), want, &ctx),
                "donateCooldown" => check_f(c.donate_cooldown() as f64, want, &ctx),
                "embargoAllCooldown" => check_f(c.embargo_all_cooldown() as f64, want, &ctx),
                "deletionMarkDuration" => check_f(c.deletion_mark_duration() as f64, want, &ctx),
                "deleteUnitCooldown" => check_f(c.delete_unit_cooldown() as f64, want, &ctx),
                "emojiMessageDuration" => check_f(c.emoji_message_duration() as f64, want, &ctx),
                "emojiMessageCooldown" => check_f(c.emoji_message_cooldown() as f64, want, &ctx),
                "quickChatCooldown" => check_f(c.quick_chat_cooldown() as f64, want, &ctx),
                "targetDuration" => check_f(c.target_duration() as f64, want, &ctx),
                "targetCooldown" => check_f(c.target_cooldown() as f64, want, &ctx),
                "allianceRequestDuration" => {
                    check_f(c.alliance_request_duration() as f64, want, &ctx)
                }
                "allianceRequestCooldown" => {
                    check_f(c.alliance_request_cooldown() as f64, want, &ctx)
                }
                "allianceDuration" => check_f(c.alliance_duration() as f64, want, &ctx),
                "temporaryEmbargoDuration" => {
                    check_f(c.temporary_embargo_duration() as f64, want, &ctx)
                }
                "minDistanceBetweenPlayers" => {
                    check_f(c.min_distance_between_players(), want, &ctx)
                }
                "armyLimitWarningThreshold" => {
                    check_f(c.army_limit_warning_threshold(), want, &ctx)
                }
                "boatMaxNumber" => check_f(c.boat_max_number() as f64, want, &ctx),
                "numSpawnPhaseTurns" => check_f(c.num_spawn_phase_turns() as f64, want, &ctx),
                "numBots" => check_f(c.num_bots() as f64, want, &ctx),
                "warshipShellLifetime" => check_f(c.warship_shell_lifetime(), want, &ctx),
                "radiusPortSpawn" => check_f(c.radius_port_spawn(), want, &ctx),
                "tradeShipShortRangeDebuff" => {
                    check_f(c.trade_ship_short_range_debuff(), want, &ctx)
                }
                "mirvNormalizeTargetTicks" => check_f(c.mirv_normalize_target_ticks(), want, &ctx),
                "nukeAllianceBreakThreshold" => {
                    check_f(c.nuke_alliance_break_threshold(), want, &ctx)
                }
                "defaultNukeTargetableRange" => {
                    check_f(c.default_nuke_targetable_range(), want, &ctx)
                }
                "defaultSamRange" => check_f(c.default_sam_range(), want, &ctx),
                "maxSamRange" => check_f(c.max_sam_range(), want, &ctx),
                "samUpgradeDuration" => check_f(c.sam_upgrade_duration(), want, &ctx),
                "defaultSamMissileSpeed" => check_f(c.default_sam_missile_speed(), want, &ctx),
                "structureMinDist" => check_f(c.structure_min_dist(), want, &ctx),
                "shellLifetime" => check_f(c.shell_lifetime(), want, &ctx),
                "warshipPatrolRange" => check_f(c.warship_patrol_range(), want, &ctx),
                "warshipTargettingRange" => check_f(c.warship_targetting_range(), want, &ctx),
                "warshipShellAttackRate" => check_f(c.warship_shell_attack_rate(), want, &ctx),
                "warshipDockingRange" => check_f(c.warship_docking_range(), want, &ctx),
                "warshipPortHealingBonusPerLevel" => {
                    check_f(c.warship_port_healing_bonus_per_level(), want, &ctx)
                }
                "warshipRetreatHealthPercent" => {
                    check_f(c.warship_retreat_health_percent(), want, &ctx)
                }
                "warshipPassiveHealing" => check_f(c.warship_passive_healing(), want, &ctx),
                "warshipPassiveHealingRange" => {
                    check_f(c.warship_passive_healing_range(), want, &ctx)
                }
                "warshipPortSwitchThreshold" => {
                    check_f(c.warship_port_switch_threshold(), want, &ctx)
                }
                "warshipMaxVeterancy" => check_f(c.warship_max_veterancy(), want, &ctx),
                "warshipVeterancyHealthBonus" => {
                    check_f(c.warship_veterancy_health_bonus(), want, &ctx)
                }
                "warshipVeterancyShellDamageBonus" => {
                    check_f(c.warship_veterancy_shell_damage_bonus(), want, &ctx)
                }
                "warshipVeterancyTransportKills" => {
                    check_f(c.warship_veterancy_transport_kills(), want, &ctx)
                }
                "warshipVeterancyTradeCaptures" => {
                    check_f(c.warship_veterancy_trade_captures(), want, &ctx)
                }
                "defensePostShellAttackRate" => {
                    check_f(c.defense_post_shell_attack_rate(), want, &ctx)
                }
                "safeFromPiratesCooldownMax" => {
                    check_f(c.safe_from_pirates_cooldown_max(), want, &ctx)
                }
                "defensePostTargettingRange" => {
                    check_f(c.defense_post_targetting_range(), want, &ctx)
                }
                "allianceExtensionPromptOffset" => {
                    check_f(c.alliance_extension_prompt_offset() as f64, want, &ctx)
                }
                "hasExtendedSpawnImmunity" => assert_eq!(
                    c.has_extended_spawn_immunity(),
                    want.as_bool().unwrap(),
                    "{ctx}"
                ),
                "playerTeams" => {
                    let got = match c.player_teams() {
                        TeamCountConfig::Count(n) => Value::from(n),
                        TeamCountConfig::Duos => Value::from("Duos"),
                        TeamCountConfig::Trios => Value::from("Trios"),
                        TeamCountConfig::Quads => Value::from("Quads"),
                        TeamCountConfig::HumansVsNations => Value::from("Humans Vs Nations"),
                    };
                    assert_eq!(&got, want, "{ctx}");
                }
                "doomsdayClockConfig" => {
                    let d = c.doomsday_clock_config();
                    assert_eq!(d.enabled, want["enabled"].as_bool().unwrap(), "{ctx}");
                    assert_eq!(d.speed.as_str(), want["speed"].as_str().unwrap(), "{ctx}");
                    for (k, v) in [
                        ("warnSeconds", d.warn_seconds),
                        ("drainStartPercent", d.drain_start_percent),
                        ("drainMaxPercent", d.drain_max_percent),
                        ("drainRampSeconds", d.drain_ramp_seconds),
                        ("drainFloorPercent", d.drain_floor_percent),
                        ("floorStartPercent", d.floor_start_percent),
                        ("floorDecaySeconds", d.floor_decay_seconds),
                        ("rotDeathSeconds", d.rot_death_seconds),
                        ("rotGrainSeconds", d.rot_grain_seconds),
                        ("rotSpecklePercent", d.rot_speckle_percent),
                        ("warshipDrainStartPercent", d.warship_drain_start_percent),
                        ("warshipDrainMaxPercent", d.warship_drain_max_percent),
                        ("warshipDrainCurveExponent", d.warship_drain_curve_exponent),
                    ] {
                        check_f(v, &want[k], &format!("{ctx} {k}"));
                    }
                }
                "overtimeConfig" => {
                    let o = c.overtime_config();
                    assert_eq!(o.enabled, want["enabled"].as_bool().unwrap(), "{ctx}");
                    check_f(o.start_minutes, &want["startMinutes"], &ctx);
                    check_f(
                        o.drop_percent_per_minute,
                        &want["dropPercentPerMinute"],
                        &ctx,
                    );
                }
                "isUnitDisabled" => assert_eq!(
                    c.is_unit_disabled(utype(&a[0])),
                    want.as_bool().unwrap(),
                    "{ctx}"
                ),
                "unitInfo" => {
                    let i = c.unit_info(utype(&a[0]));
                    assert_eq!(i.max_health, want["maxHealth"].as_f64(), "{ctx}");
                    assert_eq!(i.damage, want["damage"].as_f64(), "{ctx}");
                    assert_eq!(
                        i.construction_duration.map(|x| x as f64),
                        want["cd"].as_f64(),
                        "{ctx}"
                    );
                    assert_eq!(i.upgradable, want["up"].as_bool(), "{ctx}");
                }
                "nukeMagnitudes" | "nukeSpeed" => {
                    let u = utype(&a[0]);
                    if !nuke(u) || (name == "nukeMagnitudes" && u == UnitType::MIRV) {
                        assert!(want["err"].is_string(), "{ctx}");
                        let r = std::panic::catch_unwind(|| {
                            let c2 = Config::new(game_config(&serde_json::from_str::<Value>(r#"{"gameMap":"World","difficulty":"Easy","donateGold":false,"donateTroops":false,"gameType":"Public","gameMode":"Team","gameMapSize":"Normal","nations":"default","bots":0,"infiniteGold":false,"infiniteTroops":false,"instantBuild":false,"randomSpawn":false}"#).unwrap()), false, Rules::baseline());
                            if name == "nukeSpeed" {
                                c2.nuke_speed(u);
                            } else {
                                c2.nuke_magnitudes(u);
                            }
                        });
                        assert!(r.is_err(), "{ctx} should panic");
                    } else if name == "nukeSpeed" {
                        check_f(c.nuke_speed(u), want, &ctx);
                    } else {
                        let m = c.nuke_magnitudes(u);
                        check_f(m.inner, &want["inner"], &ctx);
                        check_f(m.outer, &want["outer"], &ctx);
                    }
                }
                "startingGold" => check_i(
                    c.starting_gold(ptype(&a[0]), a[1].as_bool().unwrap()),
                    want,
                    &ctx,
                ),
                "startManpower" => check_f(
                    c.start_manpower(ptype(&a[0]), a[1].as_bool().unwrap()),
                    want,
                    &ctx,
                ),
                "goldAdditionRate" => check_i(
                    c.gold_addition_rate(ptype(&a[0]), a[1].as_bool().unwrap()),
                    want,
                    &ctx,
                ),
                "maxTroops" | "troopIncreaseRate" => {
                    let p = TroopInput {
                        player_type: ptype(&a[0]),
                        is_lobby_creator: a[1].as_bool().unwrap(),
                        num_tiles_owned: fl(&a[2]),
                        city_level_sum: fl(&a[3]),
                        troops: fl(&a[4]),
                    };
                    let got = if name == "maxTroops" {
                        c.max_troops(&p)
                    } else {
                        c.troop_increase_rate(&p)
                    };
                    check_f(got, want, &ctx);
                }
                "trainGold" => {
                    let rel = match a[0].as_str().unwrap() {
                        "self" => TrainRelation::Self_,
                        "team" => TrainRelation::Team,
                        "ally" => TrainRelation::Ally,
                        _ => TrainRelation::Other,
                    };
                    check_i(
                        c.train_gold(rel, hex(&a[1]["f"]), a[2].as_bool().unwrap()),
                        want,
                        &ctx,
                    );
                }
                "tradeShipGold" => check_i(
                    c.trade_ship_gold(fl(&a[0]), a[1].as_bool().unwrap()),
                    want,
                    &ctx,
                ),
                "unitCost" => {
                    let owned = a[3].as_i64().unwrap();
                    let counted = move |t: UnitType| {
                        if t == UnitType::Factory {
                            (owned - 1).max(0)
                        } else {
                            owned
                        }
                    };
                    let cx = CostContext {
                        player_type: ptype(&a[1]),
                        is_lobby_creator: a[2].as_bool().unwrap(),
                        owned_and_constructed: &counted,
                        extra_units: a[4].as_i64().unwrap(),
                        mirvs_launched: owned,
                    };
                    check_i(c.unit_cost(utype(&a[0]), &cx), want, &ctx);
                }
                "conquerGoldAmount" => check_i(
                    c.conquer_gold_amount(ptype(&a[0]), a[1].as_str().unwrap().parse().unwrap()),
                    want,
                    &ctx,
                ),
                "trainSaturation" => check_f(c.train_saturation(fl(&a[0])), want, &ctx),
                "tradeShipSaturation" => check_f(c.trade_ship_saturation(fl(&a[0])), want, &ctx),
                "trainSpawnRate" => check_f(c.train_spawn_rate(fl(&a[0]), fl(&a[1])), want, &ctx),
                "tradeShipSpawnRate" => check_f(
                    c.trade_ship_spawn_rate(fl(&a[0]), fl(&a[1]), 0.0),
                    want,
                    &ctx,
                ),
                "percentageTilesOwnedToWin" => {
                    check_f(c.percentage_tiles_owned_to_win(fl(&a[0])), want, &ctx)
                }
                "boatAttackAmount" => check_f(c.boat_attack_amount(fl(&a[0])), want, &ctx),
                "attackAmount" => check_f(c.attack_amount(ptype(&a[0]), fl(&a[1])), want, &ctx),
                "defaultDonationAmount" => {
                    check_f(c.default_donation_amount(fl(&a[0])), want, &ctx)
                }
                "proximityBonusPortsNb" => {
                    check_f(c.proximity_bonus_ports_nb(fl(&a[0])), want, &ctx)
                }
                "samRange" => check_f(c.sam_range(fl(&a[0])), want, &ctx),
                "dynamicSamRange" => {
                    let st = a[0].as_object().map(|s| SamLauncherState {
                        upgrade_start_tick: s.get("upgradeStartTick").and_then(|v| v.as_f64()),
                        start_range: s["startRange"].as_f64().unwrap(),
                        target_level: s["targetLevel"].as_f64().unwrap(),
                        duration: s["duration"].as_f64().unwrap(),
                    });
                    check_f(c.dynamic_sam_range(st.as_ref(), 1.0, fl(&a[1])), want, &ctx);
                }
                "nukeDeathFactor" => check_f(
                    c.nuke_death_factor(utype(&a[0]), fl(&a[1]), fl(&a[2]), fl(&a[3])),
                    want,
                    &ctx,
                ),
                "falloutDefenseModifier" => {
                    check_f(c.fallout_defense_modifier(fl(&a[0])), want, &ctx)
                }
                "attackLogic" => {
                    let terrain = match a[0].as_u64().unwrap() {
                        0 => TerrainType::Plains,
                        1 => TerrainType::Highland,
                        2 => TerrainType::Mountain,
                        3 => TerrainType::Ocean,
                        _ => TerrainType::Impassable,
                    };
                    let defender = a[4].as_object().map(|d| AttackLogicDefender {
                        player_type: ptype(&d["type"]),
                        num_tiles: fl(&d["numTiles"]),
                        troops: fl(&d["troops"]),
                        is_traitor: d["isTraitor"].as_bool().unwrap(),
                        is_disconnected_teammate: d["isDisconnectedTeammate"].as_bool().unwrap(),
                    });
                    let input = AttackLogicInput {
                        terrain,
                        attack_troops: fl(&a[3]),
                        attacker: AttackLogicAttacker {
                            player_type: ptype(&a[1]),
                            num_tiles: fl(&a[2]),
                        },
                        defender,
                        defender_has_defense_post: a[5].as_bool().unwrap(),
                        fallout_ratio: a[6].as_object().map(|_| fl(&a[6])),
                        border_size: fl(&a[7]),
                    };
                    if want["err"].is_string() {
                        assert!(
                            std::panic::catch_unwind(|| c.attack_logic(&input)).is_err(),
                            "{ctx}"
                        );
                    } else {
                        let r = c.attack_logic(&input);
                        let w = want.as_array().unwrap();
                        check_f(r.attacker_troop_loss, &w[0], &ctx);
                        check_f(r.defender_troop_loss, &w[1], &ctx);
                        check_f(r.tick_fraction, &w[2], &ctx);
                    }
                }
                other => panic!("golden function {other} has no Rust counterpart"),
            }
        }
        assert!(seen.len() > 100, "only {} functions covered", seen.len());
    }
}
