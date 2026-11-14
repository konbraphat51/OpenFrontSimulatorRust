// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/DoomsdayClock.ts: threshold, drain, floor and rot math of the Doomsday Clock.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Everything is integer-valued f64 with floored divisions, like upstream. The HUD-only
//! `doomsdayClockWaveState` is not ported.

use super::Game::DoomsdayClockSpeed;

/// Speed sets the pace; team mode sets the bar.
#[derive(Clone, Copy, Debug)]
pub struct DoomsdayClockProfile {
    /// Pace preset.
    pub speed: DoomsdayClockSpeed,
    /// True when a side is a team rather than one player.
    pub team_game: bool,
    /// The 6-wave 55 percent ladder of builds before PR 4893 (otherwise 7 waves up to 35 percent).
    pub six_waves: bool,
}

struct WaveSchedule {
    grace_seconds: f64,
    ramp_seconds: Vec<f64>,
    pause_seconds: Vec<f64>,
    levels: Vec<f64>,
}

const LEVELS: [f64; 7] = [200.0, 400.0, 700.0, 1100.0, 1700.0, 2500.0, 3500.0];
const LEVELS_TEAM: [f64; 7] = [300.0, 600.0, 1000.0, 1500.0, 2100.0, 2800.0, 3500.0];

fn schedule(profile: &DoomsdayClockProfile) -> WaveSchedule {
    if profile.six_waves {
        let (ramp, last_ramp, pause): (f64, f64, f64) = match profile.speed {
            DoomsdayClockSpeed::Normal => (208.0, 210.0, 50.0),
            DoomsdayClockSpeed::Slow => (292.0, 290.0, 70.0),
            DoomsdayClockSpeed::Fast => (125.0, 125.0, 30.0),
            DoomsdayClockSpeed::VeryFast => (40.0, 40.0, 12.0),
        };
        return WaveSchedule {
            grace_seconds: 600.0,
            ramp_seconds: vec![ramp, ramp, ramp, ramp, ramp, last_ramp],
            pause_seconds: vec![pause, pause, pause, pause, pause, 0.0],
            levels: vec![400.0, 900.0, 1600.0, 2600.0, 4000.0, 5500.0],
        };
    }
    let (ramp, pause) = match profile.speed {
        DoomsdayClockSpeed::Normal => (168.0, 54.0),
        DoomsdayClockSpeed::Slow => (240.0, 70.0),
        DoomsdayClockSpeed::Fast => (102.0, 31.0),
        DoomsdayClockSpeed::VeryFast => (36.0, 8.0),
    };
    WaveSchedule {
        grace_seconds: 600.0,
        ramp_seconds: vec![ramp; 7],
        pause_seconds: vec![pause, pause, pause, pause, pause, pause, 0.0],
        levels: if profile.team_game {
            LEVELS_TEAM.to_vec()
        } else {
            LEVELS.to_vec()
        },
    }
}

/// `requiredBasisPoints`: required share of the map (basis points) at `elapsed` game seconds.
fn required_basis_points(profile: &DoomsdayClockProfile, elapsed: f64) -> f64 {
    let s = schedule(profile);
    if elapsed <= s.grace_seconds {
        return 0.0;
    }
    let mut t = elapsed - s.grace_seconds;
    let mut prev = 0.0;
    for i in 0..s.levels.len() {
        let ramp = s.ramp_seconds[i];
        let target = s.levels[i];
        if t < ramp {
            return prev + (((target - prev) * t) / ramp).floor();
        }
        t -= ramp;
        if t < s.pause_seconds[i] {
            return target;
        }
        t -= s.pause_seconds[i];
        prev = target;
    }
    s.levels[s.levels.len() - 1]
}

/// `doomsdayClockRequiredTiles`: minimum tiles one side must hold at `elapsed` game seconds.
pub fn doomsday_clock_required_tiles(
    profile: &DoomsdayClockProfile,
    land: f64,
    elapsed: f64,
) -> f64 {
    if land <= 0.0 {
        return 0.0;
    }
    ((required_basis_points(profile, elapsed) * land) / 10000.0).floor()
}

/// Upper bound (exclusive) of both rot noise fields.
pub const ROT_NOISE_SCALE: f64 = 65536.0;

const R2_X: u32 = 3242174889;
const R2_Y: u32 = 2447445413;

/// `rotSpeckleNoise`: even per-tile value in `[0, ROT_NOISE_SCALE)` placing scattered pinholes.
pub fn rot_speckle_noise(x: u32, y: u32, salt: u32) -> f64 {
    let h = x
        .wrapping_mul(R2_X)
        .wrapping_add(y.wrapping_mul(R2_Y))
        .wrapping_add(salt.wrapping_mul(0x9e3779b9));
    ((h >> 16) % 65536) as f64
}

/// `rotFrontNoise`: hashed per-tile value in `[0, ROT_NOISE_SCALE)` ordering the rot front.
pub fn rot_front_noise(tile: u32, salt: u32) -> f64 {
    let mut h = tile.wrapping_mul(0x27d4eb2d) ^ salt.wrapping_mul(0x9e3779b9);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2545f491);
    h ^= h >> 13;
    ((h >> 16) % 65536) as f64
}

/// `DoomsdayClockFloorConfig`.
#[derive(Clone, Copy, Debug)]
pub struct DoomsdayClockFloorConfig {
    /// Long-run floor, percent of max.
    pub drain_floor_percent: f64,
    /// Starting floor, percent of max.
    pub floor_start_percent: f64,
    /// Seconds for the floor to decay.
    pub floor_decay_seconds: f64,
}

/// `doomsdayClockTroopFloor`.
pub fn doomsday_clock_troop_floor(
    max_troops: f64,
    seconds_past_warn: f64,
    cfg: &DoomsdayClockFloorConfig,
) -> f64 {
    let end = cfg.drain_floor_percent;
    let span = cfg.floor_start_percent - end;
    let t = f64::max(0.0, seconds_past_warn);
    let pct = if span > 0.0 && cfg.floor_decay_seconds > 0.0 && t < cfg.floor_decay_seconds {
        cfg.floor_start_percent - ((span * t) / cfg.floor_decay_seconds).floor()
    } else {
        end
    };
    ((max_troops * pct) / 100.0).floor()
}

/// `DoomsdayClockDrainConfig`.
#[derive(Clone, Copy, Debug)]
pub struct DoomsdayClockDrainConfig {
    /// Percent of max lost per second at the start.
    pub drain_start_percent: f64,
    /// Percent of max lost per second at the end of the ramp.
    pub drain_max_percent: f64,
    /// Ramp length in seconds.
    pub drain_ramp_seconds: f64,
}

const DRAIN_CURVE_SCALE: f64 = 1_000_000.0;

fn drain_curve_fraction(t: f64, r: f64, exponent: f64) -> f64 {
    let ratio = ((t * DRAIN_CURVE_SCALE) / r).floor();
    let mut acc = DRAIN_CURVE_SCALE;
    let mut i = 0.0;
    while i < exponent {
        acc = ((acc * ratio) / DRAIN_CURVE_SCALE).floor();
        i += 1.0;
    }
    acc
}

/// `doomsdayClockDrain`: troops (or warship health) a skulled side loses this second.
pub fn doomsday_clock_drain(
    max_troops: f64,
    seconds_past_warn: f64,
    cfg: &DoomsdayClockDrainConfig,
    curve_exponent: f64,
) -> f64 {
    let t = f64::max(0.0, seconds_past_warn);
    let r = cfg.drain_ramp_seconds;
    let span = cfg.drain_max_percent - cfg.drain_start_percent;
    let mut pct = cfg.drain_max_percent;
    if r > 0.0 && t < r {
        let grown = if curve_exponent <= 1.0 {
            ((span * t) / r).floor()
        } else {
            ((span * drain_curve_fraction(t, r, curve_exponent)) / DRAIN_CURVE_SCALE).floor()
        };
        pct = cfg.drain_start_percent + grown;
    }
    f64::max(1.0, ((max_troops * pct) / 100.0).floor())
}

/// `doomsdayClockRotQuota`: tiles rot takes per second, `ceil(tilesLeft / secondsLeft)`.
pub fn doomsday_clock_rot_quota(
    tiles_left: f64,
    seconds_under: f64,
    rot_death_seconds: f64,
) -> f64 {
    if tiles_left <= 0.0 || rot_death_seconds <= 0.0 {
        return 0.0;
    }
    let seconds_left = f64::max(1.0, rot_death_seconds - seconds_under);
    (tiles_left / seconds_left).ceil()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(speed: DoomsdayClockSpeed, team: bool) -> DoomsdayClockProfile {
        DoomsdayClockProfile {
            speed,
            team_game: team,
            six_waves: false,
        }
    }

    #[test]
    fn required_tiles_follow_the_wave_schedule() {
        let n = p(DoomsdayClockSpeed::Normal, false);
        assert_eq!(doomsday_clock_required_tiles(&n, 10000.0, 600.0), 0.0);
        // Halfway through the first 168 s ramp: 200 * 84 / 168 = 100 bp of 10000 tiles.
        assert_eq!(doomsday_clock_required_tiles(&n, 10000.0, 684.0), 100.0);
        // The first pause holds 2%.
        assert_eq!(
            doomsday_clock_required_tiles(&n, 10000.0, 600.0 + 170.0),
            200.0
        );
        assert_eq!(doomsday_clock_required_tiles(&n, 10000.0, 1e6), 3500.0);
        let t = p(DoomsdayClockSpeed::VeryFast, true);
        assert_eq!(doomsday_clock_required_tiles(&t, 10000.0, 1e6), 3500.0);
        assert_eq!(doomsday_clock_required_tiles(&t, 0.0, 1e6), 0.0);
    }

    #[test]
    fn drain_and_floor_are_integer_ramps() {
        let cfg = DoomsdayClockDrainConfig {
            drain_start_percent: 2.0,
            drain_max_percent: 5.0,
            drain_ramp_seconds: 90.0,
        };
        assert_eq!(doomsday_clock_drain(100000.0, 0.0, &cfg, 1.0), 2000.0);
        assert_eq!(doomsday_clock_drain(100000.0, 45.0, &cfg, 1.0), 3000.0);
        assert_eq!(doomsday_clock_drain(100000.0, 90.0, &cfg, 1.0), 5000.0);
        let floor = DoomsdayClockFloorConfig {
            drain_floor_percent: 5.0,
            floor_start_percent: 40.0,
            floor_decay_seconds: 90.0,
        };
        assert_eq!(doomsday_clock_troop_floor(1000.0, 0.0, &floor), 400.0);
        assert_eq!(doomsday_clock_troop_floor(1000.0, 90.0, &floor), 50.0);
        assert_eq!(doomsday_clock_rot_quota(100.0, 140.0, 150.0), 10.0);
    }

    // Golden values computed with the upstream functions under tsx.
    #[test]
    fn noise_and_curves_match_upstream() {
        assert_eq!(rot_speckle_noise(0, 0, 1), 40503.0);
        assert_eq!(rot_speckle_noise(123, 456, 7), 1531.0);
        assert_eq!(rot_speckle_noise(1999, 1500, 300), 11212.0);
        assert_eq!(rot_speckle_noise(65535, 3, 2), 55931.0);
        assert_eq!(rot_front_noise(0, 1), 15680.0);
        assert_eq!(rot_front_noise(123456, 7), 12141.0);
        assert_eq!(rot_front_noise(2999999, 300), 46629.0);
        assert_eq!(rot_front_noise(5, 65535), 50330.0);
        let warship = DoomsdayClockDrainConfig {
            drain_start_percent: 1.0,
            drain_max_percent: 50.0,
            drain_ramp_seconds: 90.0,
        };
        assert_eq!(doomsday_clock_drain(123456.0, 37.0, &warship, 8.0), 1234.0);
        assert_eq!(doomsday_clock_drain(123456.0, 3.0, &warship, 8.0), 1234.0);
        let troops = DoomsdayClockDrainConfig {
            drain_start_percent: 2.0,
            drain_max_percent: 5.0,
            drain_ramp_seconds: 90.0,
        };
        assert_eq!(doomsday_clock_drain(98765.0, 61.0, &troops, 1.0), 3950.0);
        let fast_team = p(DoomsdayClockSpeed::Fast, true);
        assert_eq!(
            doomsday_clock_required_tiles(&fast_team, 1234567.0, 1013.5),
            132222.0
        );
        let slow = p(DoomsdayClockSpeed::Slow, false);
        assert_eq!(
            doomsday_clock_required_tiles(&slow, 987654.0, 1234.0),
            41185.0
        );
    }
}
