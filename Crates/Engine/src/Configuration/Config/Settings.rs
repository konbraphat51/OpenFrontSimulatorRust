// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts (part: settings): lobby settings, durations, cooldowns and win conditions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
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
        if self.rules.has(Change::ImmunityByConfig28) {
            return DEFAULT_SPAWN_IMMUNITY_TICKS;
        }
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

    /// Chance that a SAM launch intercepts a nuke (`samHittingChance`, builds without `SamNoRng2665`).
    pub fn sam_hitting_chance(&self) -> f64 {
        0.8
    }

    /// Chance that a SAM launch intercepts MIRV warheads (`samWarheadHittingChance`).
    pub fn sam_warhead_hitting_chance(&self) -> f64 {
        0.5
    }

    /// SAM launcher cooldown in ticks.
    pub fn sam_cooldown(&self) -> u32 {
        if self.rules.has(Change::SamCooldown90V31) {
            90
        } else if self.rules.has(Change::SamCooldown120V30) {
            120
        } else {
            75
        }
    }

    /// Missile silo cooldown in ticks.
    pub fn silo_cooldown(&self) -> u32 {
        if self.rules.has(Change::SiloCooldown90V31) {
            90
        } else {
            75
        }
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
        // The v0.28 engine has no water graph
        self.rules.has(Change::LegacyAStar28) || self.game_config.disable_nav_mesh.unwrap_or(false)
    }

    /// Alliances disabled (custom duration 0 or the legacy flag).
    pub fn disable_alliances(&self) -> bool {
        if !self.rules.has(Change::DisableAlliancesOption30) {
            return false;
        }
        self.game_config.custom_alliance_duration == Some(0)
            || self.game_config.disable_alliances.unwrap_or(false)
    }

    /// True when nations must skip alliance behaviour (builds since v0.31).
    pub fn nation_alliances_disabled(&self) -> bool {
        self.rules.has(Change::NationDisabledAlliances31) && self.disable_alliances()
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
        if !self.rules.has(Change::LobbyGoldOptions2915) {
            return 1.0;
        }
        self.game_config.gold_multiplier.unwrap_or(1.0)
    }

    /// Starting gold of a player.
    pub fn starting_gold(&self, player_type: PlayerType, is_lobby_creator: bool) -> Gold {
        if player_type == PlayerType::Bot || !self.rules.has(Change::LobbyGoldOptions2915) {
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

    /// Window `emoji_message_limit` counts over.
    pub fn emoji_message_window(&self) -> Tick {
        5 * 10
    }

    /// Emojis a human may send one recipient per window.
    pub fn emoji_message_limit(&self) -> usize {
        5
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
        if self.rules.has(Change::TeamSpawnAreas3263)
            && self.is_unit_disabled(UnitType::TransportShip)
        {
            return 0;
        }
        3
    }

    /// Length of the spawn phase in turns.
    pub fn num_spawn_phase_turns(&self) -> u32 {
        if self.game_config.game_type == GameType::Singleplayer {
            return 100;
        }
        if self.is_random_spawn() && self.rules.has(Change::RandomSpawnShortPhase3433) {
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
}
