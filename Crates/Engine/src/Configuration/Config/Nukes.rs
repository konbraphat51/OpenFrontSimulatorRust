// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts and src/core/configuration/DefaultConfig.ts (part: nukes): nuke magnitudes, speed, SAM range and death factor.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
    /// Blast radii of a nuke type; panics for other units like upstream throws.
    pub fn nuke_magnitudes(&self, unit_type: UnitType) -> NukeMagnitude {
        match unit_type {
            UnitType::MIRVWarhead => NukeMagnitude {
                inner: 12.0,
                outer: 18.0,
            },
            UnitType::AtomBomb => {
                // The tutorial's nukes hurt without wiping out a new player's small territory.
                if self.rules.has(Change::TutorialSilo6242) && self.tutorial() {
                    NukeMagnitude {
                        inner: 8.0,
                        outer: 16.0,
                    }
                } else {
                    NukeMagnitude {
                        inner: 12.0,
                        outer: 30.0,
                    }
                }
            }
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

    /// `defaultNukeSpeed()`.
    pub(super) fn default_nuke_speed(&self) -> f64 {
        if self.rules.has(Change::NukeSpeed10Meta) {
            10.0
        } else if self.rules.has(Change::NukeSpeed8V31) {
            8.0
        } else {
            6.0
        }
    }

    /// Nuke speed; panics for other units like upstream throws.
    pub fn nuke_speed(&self, unit_type: UnitType) -> f64 {
        match unit_type {
            UnitType::AtomBomb | UnitType::HydrogenBomb => self.default_nuke_speed(),
            UnitType::MIRV | UnitType::MIRVWarhead
                if !self.rules.has(Change::MirvWarheadsStandard4706) =>
            {
                if unit_type == UnitType::MIRV {
                    self.default_nuke_speed()
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
}
