// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/Veterancy.ts: effective warship max health by veterancy level.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

/// Effective max health of a warship at `veterancy`; unchanged at level 0.
pub fn max_health_with_veterancy(
    base_max_health: f64,
    veterancy: f64,
    health_bonus_percent: f64,
) -> f64 {
    if veterancy <= 0.0 {
        return base_max_health;
    }
    base_max_health + ((base_max_health * veterancy * health_bonus_percent) / 100.0).floor()
}
