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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bonus_is_floored_percent() {
        assert_eq!(max_health_with_veterancy(1000.0, 0.0, 10.0), 1000.0);
        assert_eq!(max_health_with_veterancy(1000.0, 2.0, 10.0), 1200.0);
        assert_eq!(max_health_with_veterancy(1001.0, 1.0, 15.0), 1001.0 + 150.0);
    }
}
