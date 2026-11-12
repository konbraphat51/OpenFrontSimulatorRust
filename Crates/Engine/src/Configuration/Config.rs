// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts (the baseline `Config` class).
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

    /// Defense debuff of a traitor defender.
    pub fn traitor_defense_debuff(&self) -> f64 {
        0.5
    }
    /// Speed debuff of a traitor defender.
    pub fn traitor_speed_debuff(&self) -> f64 {
        0.8
    }
    /// Ticks a player stays a traitor.
    pub fn traitor_duration(&self) -> Tick {
        30 * 10
    }
    /// Team land share (tenths) needed to win.
    pub fn team_land_share_win_threshold_tenths(&self) -> u32 {
        7
    }

    /// Doomsday clock config resolved against defaults.
    pub fn doomsday_clock_config(&self) -> DoomsdayClockConfig {
        let c = self.game_config.doomsday_clock;
        DoomsdayClockConfig {
            enabled: c
                .and_then(|c| c.enabled)
                .unwrap_or(DOOMSDAY_DEFAULTS.enabled),
            speed: c.and_then(|c| c.speed).unwrap_or(DOOMSDAY_DEFAULTS.speed),
            ..DOOMSDAY_DEFAULTS
        }
    }

    /// Overtime config resolved against defaults.
    pub fn overtime_config(&self) -> OvertimeConfig {
        let c = self.game_config.overtime;
        OvertimeConfig {
            enabled: c
                .and_then(|c| c.enabled)
                .unwrap_or(OVERTIME_DEFAULTS.enabled),
            start_minutes: c
                .and_then(|c| c.start_minutes)
                .unwrap_or(OVERTIME_DEFAULTS.start_minutes),
            drop_percent_per_minute: OVERTIME_DEFAULTS.drop_percent_per_minute,
        }
    }

    /// Spawn immunity of players, in ticks.
    pub fn spawn_immunity_duration(&self) -> Tick {
        self.game_config
            .spawn_immunity_duration
            .unwrap_or(DEFAULT_SPAWN_IMMUNITY_TICKS)
    }
    /// Spawn immunity of nations, in ticks.
    pub fn nation_spawn_immunity_duration(&self) -> Tick {
        DEFAULT_SPAWN_IMMUNITY_TICKS
    }
    /// True when spawn immunity is longer than the default.
    pub fn has_extended_spawn_immunity(&self) -> bool {
        self.spawn_immunity_duration() > DEFAULT_SPAWN_IMMUNITY_TICKS
    }

    /// Troops a city adds to the cap per level.
    pub fn city_troop_increase(&self) -> f64 {
        250_000.0
    }
    /// Defense multiplier on fallout tiles, `falloutRatio` in [0, 1].
    pub fn fallout_defense_modifier(&self, fallout_ratio: f64) -> f64 {
        5.0 - fallout_ratio * 2.0
    }
    /// Milliseconds per tick.
    pub fn ms_per_tick(&self) -> u32 {
        100
    }
    /// SAM launcher cooldown in ticks.
    pub fn sam_cooldown(&self) -> u32 {
        90
    }
    /// Missile silo cooldown in ticks.
    pub fn silo_cooldown(&self) -> u32 {
        90
    }
    /// Defense post range.
    pub fn defense_post_range(&self) -> f64 {
        30.0
    }
    /// Defense post defense bonus.
    pub fn defense_post_defense_bonus(&self) -> f64 {
        5.0
    }
    /// Defense post speed bonus.
    pub fn defense_post_speed_bonus(&self) -> f64 {
        3.0
    }

    /// Team setting, default 0 teams.
    pub fn player_teams(&self) -> TeamCountConfig {
        self.game_config
            .player_teams
            .unwrap_or(TeamCountConfig::Count(0))
    }
    /// False when nations are disabled.
    pub fn spawn_nations(&self) -> bool {
        self.game_config.nations != NationsSetting::Disabled
    }
    /// True when the unit type is disabled for this game.
    pub fn is_unit_disabled(&self, unit_type: UnitType) -> bool {
        self.game_config
            .disabled_units
            .as_ref()
            .is_some_and(|u| u.contains(&unit_type))
    }
    /// Number of bots.
    pub fn bots(&self) -> u32 {
        self.game_config.bots
    }
    /// Instant build.
    pub fn instant_build(&self) -> bool {
        self.game_config.instant_build
    }
    /// Navigation mesh disabled.
    pub fn disable_nav_mesh(&self) -> bool {
        self.game_config.disable_nav_mesh.unwrap_or(false)
    }
    /// Alliances disabled (custom duration 0 or the legacy flag).
    pub fn disable_alliances(&self) -> bool {
        self.game_config.custom_alliance_duration == Some(0)
            || self.game_config.disable_alliances.unwrap_or(false)
    }
    /// Water nukes.
    pub fn water_nukes(&self) -> bool {
        self.game_config.water_nukes.unwrap_or(false)
    }
    /// Random spawn.
    pub fn is_random_spawn(&self) -> bool {
        self.game_config.random_spawn
    }
    /// Infinite gold.
    pub fn infinite_gold(&self) -> bool {
        self.game_config.infinite_gold
    }
    /// Gold donations allowed.
    pub fn donate_gold(&self) -> bool {
        self.game_config.donate_gold
    }
    /// Infinite troops.
    pub fn infinite_troops(&self) -> bool {
        self.game_config.infinite_troops
    }
    /// Troop donations allowed.
    pub fn donate_troops(&self) -> bool {
        self.game_config.donate_troops
    }
    /// Global gold multiplier, default 1.
    pub fn gold_multiplier(&self) -> f64 {
        self.game_config.gold_multiplier.unwrap_or(1.0)
    }

    /// Starting gold of a player.
    pub fn starting_gold(&self, player_type: PlayerType, is_lobby_creator: bool) -> Gold {
        if player_type == PlayerType::Bot {
            return 0;
        }
        let base = self.game_config.starting_gold.unwrap_or(0) as i64;
        if let Some(hc) = &self.game_config.host_cheats
            && let Some(extra) = hc.starting_gold
            && extra != 0
            && is_lobby_creator
        {
            return base + extra as i64;
        }
        base
    }

    /// Global train spawn throttle for `num_train_units` trains.
    pub fn train_saturation(&self, num_train_units: f64) -> f64 {
        let boost = 1.0 + 0.5 * self.exp(-num_train_units / 30.0);
        let midpoint = if self.rules.has(Change::SaturationMidpoints5587) {
            560.0
        } else if self.rules.has(Change::TrainSaturation500Pr5462) {
            500.0
        } else {
            300.0
        };
        let damping = 1.0 - self.sigmoid(num_train_units, std::f64::consts::LN_2 / 100.0, midpoint);
        let plateau =
            0.25 * (1.0 - self.sigmoid(num_train_units, std::f64::consts::LN_2 / 150.0, 900.0));
        boost * js_max(damping, plateau)
    }

    /// Odds denominator of a train spawn.
    pub fn train_spawn_rate(&self, num_player_factories: f64, num_train_units: f64) -> f64 {
        let rate = (num_player_factories + 10.0) * 15.0;
        if !self.rules.has(Change::TradeTrainSaturation5302) {
            return rate;
        }
        js_max(1.0, (rate / self.train_saturation(num_train_units)).floor())
    }

    /// Gold of a train delivery.
    pub fn train_gold(
        &self,
        rel: TrainRelation,
        cities_visited: f64,
        is_lobby_creator: bool,
    ) -> Gold {
        let cities_visited = js_max(0.0, cities_visited - 9.0);
        let base_gold = match rel {
            TrainRelation::Ally => 35_000.0,
            TrainRelation::Team | TrainRelation::Other => 25_000.0,
            TrainRelation::Self_ => 10_000.0,
        };
        let dist_penalty = cities_visited * 5_000.0;
        let gold = js_max(5000.0, base_gold - dist_penalty);
        Util::to_int(gold * self.gold_multiplier_for(is_lobby_creator))
    }

    /// Shortest train station link.
    pub fn train_station_min_range(&self) -> f64 {
        15.0
    }
    /// Longest train station link.
    pub fn train_station_max_range(&self) -> f64 {
        110.0
    }
    /// Longest railroad.
    pub fn railroad_max_size(&self) -> f64 {
        #[allow(clippy::approx_constant)]
        let sqrt2_approx = 1.4142;
        if !self.rules.has(Change::RailroadMaxSizeDiagonal4406) {
            return self.train_station_max_range();
        }
        self.train_station_max_range() * sqrt2_approx
    }

    /// Gold of a trade ship delivery over `dist`.
    pub fn trade_ship_gold(&self, dist: f64, is_lobby_creator: bool) -> Gold {
        let debuff = self.trade_ship_short_range_debuff();
        let base_gold = 75_000.0 / (1.0 + self.exp(-0.03 * (dist - debuff))) + 50.0 * dist;
        let v = (base_gold * self.gold_multiplier_for(is_lobby_creator)).floor();
        assert!(v.is_finite(), "BigInt of a non-finite value throws");
        v as i64
    }

    /// Global trade ship spawn throttle.
    pub fn trade_ship_saturation(&self, num_trade_ships: f64) -> f64 {
        let boost = 1.0 + 0.45 * self.exp(-num_trade_ships / 120.0);
        let midpoint = if self.rules.has(Change::SaturationMidpoints5587) {
            330.0
        } else {
            230.0
        };
        let damping = 1.0 - self.sigmoid(num_trade_ships, std::f64::consts::LN_2 / 50.0, midpoint);
        let plateau =
            0.25 * (1.0 - self.sigmoid(num_trade_ships, std::f64::consts::LN_2 / 100.0, 800.0));
        boost * js_max(damping, plateau)
    }

    /// Odds denominator of a trade ship spawn.
    pub fn trade_ship_spawn_rate(
        &self,
        trade_ship_spawn_rejections: f64,
        num_trade_ships: f64,
    ) -> f64 {
        let rejection_modifier = 1.0 / (trade_ship_spawn_rejections + 1.0);
        if !self.rules.has(Change::TradeTrainSaturation5302) {
            let decay_rate = std::f64::consts::LN_2 / 50.0;
            let base_spawn_rate = 1.0 - self.sigmoid(num_trade_ships, decay_rate, 400.0);
            return ((100.0 * rejection_modifier) / base_spawn_rate).floor();
        }
        js_max(
            1.0,
            ((100.0 * rejection_modifier) / self.trade_ship_saturation(num_trade_ships)).floor(),
        )
    }

    /// Static data of a unit type (cost excluded, see [`Self::unit_cost`]).
    pub fn unit_info(&self, unit_type: UnitType) -> UnitInfo {
        let build = |ticks: u32| Some(if self.instant_build() { 0 } else { ticks });
        match unit_type {
            UnitType::Warship => UnitInfo {
                max_health: Some(1000.0),
                ..UnitInfo::default()
            },
            UnitType::Shell => UnitInfo {
                damage: Some(250.0),
                ..UnitInfo::default()
            },
            UnitType::Port => UnitInfo {
                construction_duration: build(5 * 10),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::MissileSilo => UnitInfo {
                construction_duration: build(10 * 10),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::DefensePost => UnitInfo {
                construction_duration: build(5 * 10),
                ..UnitInfo::default()
            },
            UnitType::SAMLauncher => UnitInfo {
                construction_duration: build(SAM_CONSTRUCTION_TICKS),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::City | UnitType::Factory => UnitInfo {
                construction_duration: build(2 * 10),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::TransportShip
            | UnitType::SAMMissile
            | UnitType::AtomBomb
            | UnitType::HydrogenBomb
            | UnitType::MIRV
            | UnitType::MIRVWarhead
            | UnitType::TradeShip
            | UnitType::Train => UnitInfo::default(),
        }
    }

    fn has_infinite_gold_for(&self, is_lobby_creator: bool) -> bool {
        self.infinite_gold()
            || (self
                .game_config
                .host_cheats
                .as_ref()
                .and_then(|h| h.infinite_gold)
                .unwrap_or(false)
                && is_lobby_creator)
    }

    fn has_infinite_troops_for(&self, is_lobby_creator: bool) -> bool {
        self.infinite_troops()
            || (self
                .game_config
                .host_cheats
                .as_ref()
                .and_then(|h| h.infinite_troops)
                .unwrap_or(false)
                && is_lobby_creator)
    }

    fn gold_multiplier_for(&self, is_lobby_creator: bool) -> f64 {
        let base = self.gold_multiplier();
        if let Some(m) = self
            .game_config
            .host_cheats
            .as_ref()
            .and_then(|h| h.gold_multiplier)
            && m != 0.0
            && is_lobby_creator
        {
            return m;
        }
        base
    }

    /// Gold taken when conquering a player holding `gold`.
    pub fn conquer_gold_amount(&self, captured_type: PlayerType, gold: Gold) -> Gold {
        match captured_type {
            PlayerType::Bot | PlayerType::Nation => gold,
            PlayerType::Human => gold / 2,
        }
    }

    fn cost_wrapped(
        &self,
        ctx: &CostContext,
        cost_fn: impl Fn(f64) -> f64,
        types: &[UnitType],
    ) -> Gold {
        if ctx.player_type == PlayerType::Human && self.has_infinite_gold_for(ctx.is_lobby_creator)
        {
            return 0;
        }
        let num_units: i64 = types.iter().map(|t| (ctx.owned_and_constructed)(*t)).sum();
        cost_fn((num_units + ctx.extra_units) as f64) as i64
    }

    /// Gold cost of building one more unit of `unit_type` (`UnitInfo.cost`).
    pub fn unit_cost(&self, unit_type: UnitType, ctx: &CostContext) -> Gold {
        match unit_type {
            UnitType::TransportShip
            | UnitType::Shell
            | UnitType::SAMMissile
            | UnitType::MIRVWarhead
            | UnitType::TradeShip
            | UnitType::Train => 0,
            UnitType::Warship => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, (n + 1.0) * 250_000.0),
                &[UnitType::Warship],
            ),
            UnitType::Port => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::Port, UnitType::Factory],
            ),
            UnitType::AtomBomb => self.cost_wrapped(ctx, |_| 750_000.0, &[UnitType::AtomBomb]),
            UnitType::HydrogenBomb => {
                self.cost_wrapped(ctx, |_| 5_000_000.0, &[UnitType::HydrogenBomb])
            }
            UnitType::MIRV => {
                if ctx.player_type == PlayerType::Human
                    && self.has_infinite_gold_for(ctx.is_lobby_creator)
                {
                    return 0;
                }
                25_000_000 + ctx.mirvs_launched * 15_000_000
            }
            UnitType::MissileSilo => {
                self.cost_wrapped(ctx, |_| 1_000_000.0, &[UnitType::MissileSilo])
            }
            UnitType::DefensePost => self.cost_wrapped(
                ctx,
                |n| js_min(250_000.0, (n + 1.0) * 50_000.0),
                &[UnitType::DefensePost],
            ),
            UnitType::SAMLauncher => self.cost_wrapped(
                ctx,
                |n| js_min(3_000_000.0, (n + 1.0) * 1_500_000.0),
                &[UnitType::SAMLauncher],
            ),
            UnitType::City => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::City],
            ),
            UnitType::Factory => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::Factory, UnitType::Port],
            ),
        }
    }

    /// Default donation: a third of the sender's troops, floored.
    pub fn default_donation_amount(&self, sender_troops: f64) -> f64 {
        (sender_troops / 3.0).floor()
    }
    /// Gold donation cooldown in ticks.
    pub fn donate_cooldown(&self) -> Tick {
        10 * 10
    }
    /// Embargo-all cooldown in ticks.
    pub fn embargo_all_cooldown(&self) -> Tick {
        10 * 10
    }
    /// Ticks a unit stays marked for deletion.
    pub fn deletion_mark_duration(&self) -> Tick {
        30 * 10
    }
    /// Cooldown between unit deletions.
    pub fn delete_unit_cooldown(&self) -> Tick {
        30 * 10
    }
    /// Emoji display duration.
    pub fn emoji_message_duration(&self) -> Tick {
        5 * 10
    }
    /// Emoji cooldown.
    pub fn emoji_message_cooldown(&self) -> Tick {
        5 * 10
    }
    /// Quick chat cooldown.
    pub fn quick_chat_cooldown(&self) -> Tick {
        3 * 10
    }
    /// Target marker duration.
    pub fn target_duration(&self) -> Tick {
        10 * 10
    }
    /// Target marker cooldown.
    pub fn target_cooldown(&self) -> Tick {
        15 * 10
    }
    /// Alliance request lifetime.
    pub fn alliance_request_duration(&self) -> Tick {
        20 * 10
    }
    /// Cooldown between alliance requests.
    pub fn alliance_request_cooldown(&self) -> Tick {
        30 * 10
    }
    /// Alliance length in ticks.
    pub fn alliance_duration(&self) -> Tick {
        match self.game_config.custom_alliance_duration {
            Some(m) if m > 0 => m * 60 * 10,
            _ => 300 * 10,
        }
    }
    /// Temporary embargo length.
    pub fn temporary_embargo_duration(&self) -> Tick {
        300 * 10
    }
    /// Minimum spawn distance between players.
    pub fn min_distance_between_players(&self) -> f64 {
        30.0
    }

    /// Share of land (percent) a side needs to win after `elapsed_game_seconds`.
    pub fn percentage_tiles_owned_to_win(&self, elapsed_game_seconds: f64) -> f64 {
        let base = if !self.rules.has(Change::WinThresholdSame5223)
            && self.game_config.game_mode == GameMode::Team
        {
            95.0
        } else {
            PERCENT_TILES_OWNED_TO_WIN
        };
        let sd = self.overtime_config();
        if !sd.enabled || !self.rules.has(Change::Overtime5062) {
            return base;
        }
        let seconds_past_start = elapsed_game_seconds.floor() - sd.start_minutes * 60.0;
        if seconds_past_start <= 0.0 {
            return base;
        }
        js_max(
            0.0,
            base - ((seconds_past_start * sd.drop_percent_per_minute) / 60.0).floor(),
        )
    }
    /// Fraction of max troops at which the HUD warns.
    pub fn army_limit_warning_threshold(&self) -> f64 {
        0.8
    }
    /// Maximum simultaneous transport boats.
    pub fn boat_max_number(&self) -> u32 {
        if self.is_unit_disabled(UnitType::TransportShip) {
            return 0;
        }
        3
    }
    /// Length of the spawn phase in turns.
    pub fn num_spawn_phase_turns(&self) -> u32 {
        if self.game_config.game_type == GameType::Singleplayer {
            return 100;
        }
        if self.is_random_spawn() {
            return 150;
        }
        if self.rules.has(Change::SpawnPhase20s5347) {
            200
        } else {
            300
        }
    }
    /// Number of bots.
    pub fn num_bots(&self) -> u32 {
        self.bots()
    }

    fn large_territory_bonus(&self, num_tiles: f64, depth: f64) -> f64 {
        1.0 - depth
            * self.sigmoid(
                self.log(num_tiles),
                LARGE_TERRITORY_STEEPNESS,
                self.log(LARGE_TERRITORY_MIDPOINT),
            )
    }

    /// Per-tile attack outcome (`attackLogic`).
    pub fn attack_logic(&self, input: &AttackLogicInput) -> AttackLogicResult {
        let attack_troops = input.attack_troops;
        let attacker = input.attacker;
        let (mut mag, mut tile_cost) = match input.terrain {
            TerrainType::Plains => (80.0, 16.5),
            TerrainType::Highland => (100.0, 20.0),
            TerrainType::Mountain => (120.0, 25.0),
            TerrainType::Impassable => panic!("impassable terrain cannot be attacked"),
            TerrainType::Ocean => panic!("terrain type {} not supported", TerrainType::Ocean as u8),
        };

        if input.defender.is_some() && input.defender_has_defense_post {
            mag *= self.defense_post_defense_bonus();
            tile_cost *= self.defense_post_speed_bonus();
        }
        if let Some(ratio) = input.fallout_ratio {
            let fallout = self.fallout_defense_modifier(ratio);
            mag *= fallout;
            tile_cost *= fallout;
        }

        let Some(defender) = input.defender else {
            let tick_budget = input.border_size * 2.0;
            return AttackLogicResult {
                attacker_troop_loss: mag
                    / if attacker.player_type == PlayerType::Bot {
                        10.0
                    } else {
                        5.0
                    },
                defender_troop_loss: 0.0,
                tick_fraction: Util::within(
                    (TERRA_NULLIUS_COST_SCALE * tile_cost) / attack_troops,
                    TERRA_NULLIUS_MIN_COST,
                    TERRA_NULLIUS_MAX_COST,
                ) / tick_budget,
            };
        };

        if defender.is_disconnected_teammate {
            mag = 0.0;
        }
        if (attacker.player_type == PlayerType::Human || attacker.player_type == PlayerType::Nation)
            && defender.player_type == PlayerType::Bot
        {
            mag *= BOT_DEFENDER_LOSS_MULT;
        }

        let large_attacker_bonus =
            self.large_territory_bonus(attacker.num_tiles, LARGE_ATTACKER_DEPTH);
        let large_defender_bonus =
            self.large_territory_bonus(defender.num_tiles, LARGE_DEFENDER_DEPTH);

        let traitor_loss_mod = if defender.is_traitor {
            self.traitor_defense_debuff()
        } else {
            1.0
        };
        let traitor_cost_mod = if defender.is_traitor {
            self.traitor_speed_debuff()
        } else {
            1.0
        };

        let defender_troop_loss = defender.troops / defender.num_tiles;

        let troop_ratio = defender.troops / attack_troops;
        let attacker_troop_loss = mag
            * traitor_loss_mod
            * Util::within(troop_ratio, 0.6, 2.0)
            * (ATTACKER_LOSS_BASE * large_attacker_bonus * large_defender_bonus
                + ATTACKER_LOSS_PER_DENSITY * defender_troop_loss);

        let speed_cost = (Util::within(troop_ratio, 0.82, 7.5)
            * Util::within(troop_ratio / 20.0, 1.0, 50.0))
            / if self.rules.has(Change::AttackSpeedDivisor5425) {
                SPEED_COST_DIVISOR
            } else {
                SPEED_COST_DIVISOR_OLD
            };
        let large_attacker_speed_bonus =
            self.large_territory_bonus(attacker.num_tiles, LARGE_ATTACKER_SPEED_DEPTH);
        AttackLogicResult {
            attacker_troop_loss,
            defender_troop_loss,
            tick_fraction: (speed_cost
                * tile_cost
                * large_attacker_speed_bonus
                * large_defender_bonus
                * traitor_cost_mod)
                / input.border_size,
        }
    }

    /// Pre-PR-5173 `attackLogic(gm, attackTroops, attacker, defender, tile)`; `border_size` is unused.
    pub fn attack_logic_legacy(&self, input: &AttackLogicInput) -> LegacyAttackResult {
        let attack_troops = input.attack_troops;
        let attacker = input.attacker;
        let (mut mag, mut speed) = match input.terrain {
            TerrainType::Plains => (80.0, 16.5),
            TerrainType::Highland => (100.0, 20.0),
            TerrainType::Mountain => (120.0, 25.0),
            TerrainType::Impassable => panic!("impassable terrain cannot be attacked"),
            TerrainType::Ocean => panic!("terrain type {} not supported", TerrainType::Ocean as u8),
        };
        if input.defender.is_some() && input.defender_has_defense_post {
            mag *= self.defense_post_defense_bonus();
            speed *= self.defense_post_speed_bonus();
        }
        if let Some(ratio) = input.fallout_ratio {
            mag *= self.fallout_defense_modifier(ratio);
            speed *= self.fallout_defense_modifier(ratio);
        }
        if let Some(defender) = input.defender {
            if defender.is_disconnected_teammate {
                mag = 0.0;
            }
            if (attacker.player_type == PlayerType::Human
                || attacker.player_type == PlayerType::Nation)
                && defender.player_type == PlayerType::Bot
            {
                mag *= 0.7;
            }
        }
        let Some(defender) = input.defender else {
            return LegacyAttackResult {
                attacker_troop_loss: if attacker.player_type == PlayerType::Bot {
                    mag / 10.0
                } else {
                    mag / 5.0
                },
                defender_troop_loss: 0.0,
                tiles_per_tick_used: Util::within(
                    (2000.0 * js_max(10.0, speed)) / attack_troops,
                    5.0,
                    100.0,
                ),
            };
        };
        let defense_sig = 1.0
            - self.sigmoid(
                defender.num_tiles,
                DEFENSE_DEBUFF_DECAY_RATE,
                DEFENSE_DEBUFF_MIDPOINT,
            );
        let large_defender_speed_debuff = 0.7 + 0.3 * defense_sig;
        let large_defender_attack_debuff = 0.7 + 0.3 * defense_sig;

        let mut large_attack_bonus = 1.0;
        if attacker.num_tiles > 100_000.0 {
            large_attack_bonus = self.pow((100_000.0 / attacker.num_tiles).sqrt(), 0.7);
        }
        let mut large_attacker_speed_bonus = 1.0;
        if attacker.num_tiles > 100_000.0 {
            large_attacker_speed_bonus = self.pow(100_000.0 / attacker.num_tiles, 0.6);
        }

        let defender_troop_loss = defender.troops / defender.num_tiles;
        let traitor_mod = if defender.is_traitor {
            self.traitor_defense_debuff()
        } else {
            1.0
        };
        let current_attacker_loss = Util::within(defender.troops / attack_troops, 0.6, 2.0)
            * mag
            * 0.8
            * large_defender_attack_debuff
            * large_attack_bonus
            * traitor_mod;
        let alt_attacker_loss = 1.3 * defender_troop_loss * (mag / 100.0) * traitor_mod;
        let attacker_troop_loss = 0.6 * current_attacker_loss + 0.4 * alt_attacker_loss;
        LegacyAttackResult {
            attacker_troop_loss,
            defender_troop_loss,
            tiles_per_tick_used: Util::within(defender.troops / (5.0 * attack_troops), 0.2, 1.5)
                * speed
                * large_defender_speed_debuff
                * large_attacker_speed_bonus
                * if defender.is_traitor {
                    self.traitor_speed_debuff()
                } else {
                    1.0
                },
        }
    }

    /// Pre-PR-5172 `attackTilesPerTick`; `defender_troops` is `None` for terra nullius.
    pub fn attack_tiles_per_tick_legacy(
        &self,
        attack_troops: f64,
        defender_troops: Option<f64>,
        num_adjacent_tiles_with_enemy: f64,
    ) -> f64 {
        match defender_troops {
            Some(troops) => {
                Util::within(((5.0 * attack_troops) / troops) * 2.0, 0.01, 0.5)
                    * num_adjacent_tiles_with_enemy
                    * 3.0
            }
            None => num_adjacent_tiles_with_enemy * 2.0,
        }
    }

    /// Troops sent by a boat attack: a fifth of the attacker's troops, floored.
    pub fn boat_attack_amount(&self, attacker_troops: f64) -> f64 {
        (attacker_troops / 5.0).floor()
    }
    /// Warship shell lifetime in ticks.
    pub fn warship_shell_lifetime(&self) -> f64 {
        20.0
    }
    /// Radius around a port where boats spawn.
    pub fn radius_port_spawn(&self) -> f64 {
        20.0
    }
    /// Range below which trade routes are punished.
    pub fn trade_ship_short_range_debuff(&self) -> f64 {
        300.0
    }
    /// Number of nearest ports considered for the proximity bonus.
    pub fn proximity_bonus_ports_nb(&self, total_ports: f64) -> f64 {
        Util::within(total_ports / 3.0, 4.0, total_ports)
    }
    /// Troops committed by an attack order at 100 percent.
    pub fn attack_amount(&self, attacker_type: PlayerType, attacker_troops: f64) -> f64 {
        if attacker_type == PlayerType::Bot {
            attacker_troops / 20.0
        } else {
            attacker_troops / 5.0
        }
    }

    /// Starting troops of a player.
    pub fn start_manpower(&self, player_type: PlayerType, is_lobby_creator: bool) -> f64 {
        if player_type == PlayerType::Bot {
            return 10_000.0;
        }
        if player_type == PlayerType::Nation {
            return match self.game_config.difficulty {
                Difficulty::Easy => 12_500.0,
                Difficulty::Medium => 18_750.0,
                Difficulty::Hard => 25_000.0,
                Difficulty::Impossible => 31_250.0,
            };
        }
        if self.has_infinite_troops_for(is_lobby_creator) {
            1_000_000.0
        } else {
            25_000.0
        }
    }

    /// Troop cap of a player.
    pub fn max_troops(&self, p: &TroopInput) -> f64 {
        let max_troops = if p.player_type == PlayerType::Human
            && self.has_infinite_troops_for(p.is_lobby_creator)
        {
            1_000_000_000.0
        } else {
            2.0 * (self.pow(p.num_tiles_owned, 0.6) * 1000.0 + 50000.0)
                + p.city_level_sum * self.city_troop_increase()
        };

        if p.player_type == PlayerType::Bot {
            return max_troops / 3.0;
        }
        if p.player_type == PlayerType::Human {
            return max_troops;
        }
        match self.game_config.difficulty {
            Difficulty::Easy => max_troops * 0.5,
            Difficulty::Medium => max_troops * 0.75,
            Difficulty::Hard => max_troops * 1.0,
            Difficulty::Impossible => max_troops * 1.25,
        }
    }

    /// Troops gained this tick.
    pub fn troop_increase_rate(&self, p: &TroopInput) -> f64 {
        let max = self.max_troops(p);
        let mut to_add = 10.0 + self.pow(p.troops, 0.73) / 4.0;
        let ratio = 1.0 - p.troops / max;
        to_add *= ratio;
        if p.player_type == PlayerType::Bot {
            to_add *= 0.5;
        }
        if p.player_type == PlayerType::Nation {
            match self.game_config.difficulty {
                Difficulty::Easy => to_add *= 0.9,
                Difficulty::Medium => to_add *= 0.95,
                Difficulty::Hard => to_add *= 1.0,
                Difficulty::Impossible => to_add *= 1.05,
            }
        }
        js_min(p.troops + to_add, max) - p.troops
    }

    /// Gold gained per tick.
    pub fn gold_addition_rate(&self, player_type: PlayerType, is_lobby_creator: bool) -> Gold {
        let multiplier = self.gold_multiplier_for(is_lobby_creator);
        let base_rate = if player_type == PlayerType::Bot {
            50.0
        } else {
            100.0
        };
        (base_rate * multiplier).floor() as i64
    }

    /// Blast radii of a nuke type; panics for other units like upstream throws.
    pub fn nuke_magnitudes(&self, unit_type: UnitType) -> NukeMagnitude {
        match unit_type {
            UnitType::MIRVWarhead => NukeMagnitude {
                inner: 12.0,
                outer: 18.0,
            },
            UnitType::AtomBomb => NukeMagnitude {
                inner: 12.0,
                outer: 30.0,
            },
            UnitType::HydrogenBomb => NukeMagnitude {
                inner: 80.0,
                outer: 100.0,
            },
            other => panic!("Unknown nuke type: {}", other.as_str()),
        }
    }
    /// Damage to an alliance that triggers breaking it.
    pub fn nuke_alliance_break_threshold(&self) -> f64 {
        100.0
    }
    /// Nuke speed; panics for other units like upstream throws.
    pub fn nuke_speed(&self, unit_type: UnitType) -> f64 {
        match unit_type {
            UnitType::AtomBomb | UnitType::HydrogenBomb => 10.0,
            UnitType::MIRV | UnitType::MIRVWarhead
                if !self.rules.has(Change::MirvWarheadsStandard4706) =>
            {
                if unit_type == UnitType::MIRV {
                    10.0
                } else {
                    15.0
                }
            }
            UnitType::MIRV => 15.0,
            UnitType::MIRVWarhead => 22.0,
            other => panic!("Unknown nuke type: {}", other.as_str()),
        }
    }
    /// MIRV target normalisation ticks.
    pub fn mirv_normalize_target_ticks(&self) -> f64 {
        14.0
    }
    /// Range within which nukes are targetable.
    pub fn default_nuke_targetable_range(&self) -> f64 {
        150.0
    }
    /// Level 0 SAM range.
    pub fn default_sam_range(&self) -> f64 {
        70.0
    }
    /// SAM range at `level`.
    pub fn sam_range(&self, level: f64) -> f64 {
        self.max_sam_range() - 480.0 / (level + 5.0)
    }
    /// SAM range limit.
    pub fn max_sam_range(&self) -> f64 {
        150.0
    }
    /// SAM upgrade duration in ticks.
    pub fn sam_upgrade_duration(&self) -> f64 {
        (self.sam_cooldown() as f64 / 2.0).floor()
    }

    /// Range of a SAM mid-upgrade.
    pub fn dynamic_sam_range(
        &self,
        state: Option<&SamLauncherState>,
        level: f64,
        current_tick: f64,
    ) -> f64 {
        let Some(state) = state else {
            return self.sam_range(level);
        };
        let Some(start_tick) = state.upgrade_start_tick else {
            return self.sam_range(level);
        };
        let duration = state.duration;
        let elapsed = current_tick - start_tick;
        if elapsed >= duration {
            return self.sam_range(state.target_level);
        }
        let target_range = self.sam_range(state.target_level);
        let diff = target_range - state.start_range;
        state.start_range + (diff * elapsed) / duration
    }

    /// SAM missile speed.
    pub fn default_sam_missile_speed(&self) -> f64 {
        12.0
    }

    /// Share of humans a nuke kills (`humans` counts soldiers, attackers and boat troops).
    pub fn nuke_death_factor(
        &self,
        nuke_type: UnitType,
        humans: f64,
        tiles_owned: f64,
        max_troops: f64,
    ) -> f64 {
        if nuke_type != UnitType::MIRVWarhead {
            return (5.0 * humans) / js_max(1.0, tiles_owned);
        }
        let target_troops = 0.03 * max_troops;
        let excess_troops = js_max(0.0, humans - target_troops);
        let scaling_factor = 500.0;
        let steepness = 2.0;
        let normalized_excess = excess_troops / max_troops;
        scaling_factor * (1.0 - self.exp(-steepness * normalized_excess))
    }

    /// Minimum distance between structures.
    pub fn structure_min_dist(&self) -> f64 {
        15.0
    }
    /// Shell lifetime.
    pub fn shell_lifetime(&self) -> f64 {
        50.0
    }
    /// Warship patrol range.
    pub fn warship_patrol_range(&self) -> f64 {
        100.0
    }
    /// Warship targeting range.
    pub fn warship_targetting_range(&self) -> f64 {
        130.0
    }
    /// Ticks between warship shells.
    pub fn warship_shell_attack_rate(&self) -> f64 {
        20.0
    }
    /// Warship docking range.
    pub fn warship_docking_range(&self) -> f64 {
        5.0
    }
    /// Port healing per level.
    pub fn warship_port_healing_bonus_per_level(&self) -> f64 {
        5.0
    }
    /// Health percent at or below which a warship retreats.
    pub fn warship_retreat_health_percent(&self) -> f64 {
        75.0
    }
    /// Passive warship healing per tick.
    pub fn warship_passive_healing(&self) -> f64 {
        1.0
    }
    /// Range of passive warship healing.
    pub fn warship_passive_healing_range(&self) -> f64 {
        150.0
    }
    /// Ratio deciding whether a warship switches ports.
    pub fn warship_port_switch_threshold(&self) -> f64 {
        0.75
    }
    /// Maximum veterancy level.
    pub fn warship_max_veterancy(&self) -> f64 {
        if !self.rules.has(Change::WarshipVeterancy4433) {
            return 0.0;
        }
        3.0
    }
    /// Max-health bonus per veterancy level, integer percent.
    pub fn warship_veterancy_health_bonus(&self) -> f64 {
        20.0
    }
    /// Shell-damage bonus per veterancy level, integer percent.
    pub fn warship_veterancy_shell_damage_bonus(&self) -> f64 {
        20.0
    }
    /// Transport kills per veterancy level.
    pub fn warship_veterancy_transport_kills(&self) -> f64 {
        10.0
    }
    /// Trade captures per veterancy level.
    pub fn warship_veterancy_trade_captures(&self) -> f64 {
        25.0
    }
    /// Ticks between defense post shells.
    pub fn defense_post_shell_attack_rate(&self) -> f64 {
        100.0
    }
    /// Maximum safe-from-pirates cooldown.
    pub fn safe_from_pirates_cooldown_max(&self) -> f64 {
        20.0
    }
    /// Defense post targeting range.
    pub fn defense_post_targetting_range(&self) -> f64 {
        75.0
    }
    /// Ticks before alliance expiry when extension is offered.
    pub fn alliance_extension_prompt_offset(&self) -> Tick {
        300
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
                "tradeShipSpawnRate" => {
                    check_f(c.trade_ship_spawn_rate(fl(&a[0]), fl(&a[1])), want, &ctx)
                }
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
