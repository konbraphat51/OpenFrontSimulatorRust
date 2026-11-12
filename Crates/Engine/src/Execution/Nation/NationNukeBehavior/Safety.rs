// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: safety): blast coverage and friendly-fire checks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// Hard and Impossible in FFA: whether they are holding gold back for a MIRV their income will soon reach.
    pub(super) fn is_saving_for_mirv(&self, game: &GameImpl) -> bool {
        Self::forgets_nuke_prices(game)
            && !self.pays_real_nuke_price(game)
            && is_mirv_worth_saving_for(game, self.player)
            && is_save_up_within_reach(game, self.player, self.mirv_save_up_target(game))
    }

    pub(super) fn mirv_save_up_target(&self, game: &GameImpl) -> Gold {
        self.cost(game, UnitType::MIRV) + self.cost(game, UnitType::HydrogenBomb)
    }

    /// Whether the player is attacking us or has a bomb flying at our land.
    pub(super) fn is_hitting_us(&self, game: &GameImpl, other: SmallId) -> bool {
        if game
            .player(self.player)
            .incoming_attacks()
            .into_iter()
            .any(|a| game.attack(a).attacker() == other)
        {
            return true;
        }
        game.player(other)
            .units_of_types(&[
                UnitType::AtomBomb,
                UnitType::HydrogenBomb,
                UnitType::MIRV,
                UnitType::MIRVWarhead,
            ])
            .into_iter()
            .any(|nuke| {
                game.unit_any(nuke)
                    .target_tile()
                    .is_some_and(|t| game.owner_id(t) == self.player)
            })
    }

    /// Whether our inner blast would overlap the inner blast of any player's nuke in flight.
    pub(super) fn is_nuke_already_landing_near(
        &self,
        game: &GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
    ) -> bool {
        let config = game.config();
        let our_inner_radius = config.nuke_magnitudes(nuke_type).inner;
        for nuke in game.units_of_types(&[
            UnitType::AtomBomb,
            UnitType::HydrogenBomb,
            UnitType::MIRVWarhead,
        ]) {
            let unit = game.unit_any(nuke);
            let Some(target_tile) = unit.target_tile() else {
                continue;
            };
            let sum_radius = our_inner_radius + config.nuke_magnitudes(unit.unit_type).inner;
            if game.map.euclidean_dist_squared(tile, target_tile) as f64 <= sum_radius * sum_radius
            {
                return true;
            }
        }
        false
    }

    /// Grid-sampled share of the blast that hits enemy land; inner radius tiles count double.
    pub(super) fn enemy_land_blast_coverage(
        &self,
        game: &GameImpl,
        center: TileRef,
        nuke_type: UnitType,
    ) -> f64 {
        let magnitude = game.config().nuke_magnitudes(nuke_type);
        let (inner, outer) = (magnitude.inner, magnitude.outer);
        let step = f64::max(1.0, (outer / 10.0).floor());
        let cx = game.x(center) as f64;
        let cy = game.y(center) as f64;
        let mut total = 0.0;
        let mut enemy_land = 0.0;
        let mut dy = -outer;
        while dy <= outer {
            let mut dx = -outer;
            while dx <= outer {
                let dist_squared = dx * dx + dy * dy;
                if dist_squared <= outer * outer {
                    let weight = if dist_squared <= inner * inner {
                        2.0
                    } else {
                        1.0
                    };
                    total += weight;
                    if game.is_valid_coord((cx + dx) as i32, (cy + dy) as i32) {
                        // Water, fallout and unowned land have no player owner.
                        let owner =
                            game.owner_id(game.tile_ref((cx + dx) as i32, (cy + dy) as i32));
                        if owner != 0 && !game.player(self.player).is_friendly(owner, true) {
                            enemy_land += weight;
                        }
                    }
                }
                dx += step;
            }
            dy += step;
        }
        enemy_land / total
    }

    /// Whether any tile in the blast, widened by `margin`, belongs to us, a teammate or an ally (even AFK ones).
    pub(super) fn blast_hits_friendly_land(
        &self,
        game: &GameImpl,
        center: TileRef,
        nuke_type: UnitType,
        margin: f64,
    ) -> bool {
        let friendly_ids: HashSet<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(self.player).is_friendly(p, true))
            .collect();
        let outer = game.config().nuke_magnitudes(nuke_type).outer + margin;
        let cx = game.x(center) as f64;
        let cy = game.y(center) as f64;
        let mut dy = -outer;
        while dy <= outer {
            let mut dx = -outer;
            while dx <= outer {
                if dx * dx + dy * dy <= outer * outer
                    && game.is_valid_coord((cx + dx) as i32, (cy + dy) as i32)
                {
                    let owner_id = game.owner_id(game.tile_ref((cx + dx) as i32, (cy + dy) as i32));
                    if friendly_ids.contains(&owner_id) {
                        return true;
                    }
                }
                dx += 1.0;
            }
            dy += 1.0;
        }
        false
    }
}
