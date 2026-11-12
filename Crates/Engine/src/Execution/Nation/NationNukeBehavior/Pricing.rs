// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: pricing): perceived nuke cost of v0.35.0 on.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    pub(super) fn get_perceived_nuke_cost_v2(&self, game: &GameImpl, t: UnitType) -> Gold {
        if self.pays_real_nuke_price(game) {
            return self.cost(game, t);
        }
        // Easy and Medium keep the price up for good, so only while a MIRV is worth saving for.
        if !Self::forgets_nuke_prices(game) && !is_mirv_worth_saving_for(game, self.player) {
            return self.cost(game, t);
        }
        self.inflated_nuke_cost(game, t)
    }

    pub(super) fn pays_real_nuke_price(&self, game: &GameImpl) -> bool {
        // With only 2 players left, use the actual cost (no point saving for a MIRV).
        if game.players().len() == 2 {
            return true;
        }
        if game.config().is_unit_disabled(UnitType::MIRV) {
            return true;
        }
        // A limited amount in team games, synced with NationStructureBehavior.
        let gold = game.player(self.player).gold();
        if Self::game_mode(game) == GameMode::Team && gold > self.cost(game, UnitType::HydrogenBomb)
        {
            return true;
        }
        // The actual cost if there is already enough gold for both a MIRV and a hydro.
        if gold > self.mirv_save_up_target(game) {
            return true;
        }
        // On Hard and Impossible, ignore the perceived cost when under heavy attack (go all-in on nukes).
        let difficulty = Self::difficulty(game);
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && self.is_under_heavy_attack(game)
    }

    /// Hard and Impossible in FFA: the perceived price falls back to the real one within minutes of the last launch.
    pub(super) fn forgets_nuke_prices(game: &GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && Self::game_mode(game) != GameMode::Team
    }

    pub(super) fn inflated_nuke_cost(&self, game: &GameImpl, t: UnitType) -> Gold {
        let real = self.cost(game, t);
        let (perceived, last_launch) = if t == UnitType::AtomBomb {
            (self.atom_bomb_perceived_cost, self.atom_bomb_last_launch)
        } else {
            (
                self.hydrogen_bomb_perceived_cost,
                self.hydrogen_bomb_last_launch,
            )
        };
        if perceived <= real {
            return real;
        }
        if !Self::forgets_nuke_prices(game) {
            return perceived;
        }
        let left: i128 = match last_launch {
            None => 0,
            Some(l) => NUKE_PRICE_MEMORY_TICKS - (game.ticks() as i128 - l as i128),
        };
        if left <= 0 {
            return real;
        }
        (real as i128 + ((perceived - real) as i128 * left) / NUKE_PRICE_MEMORY_TICKS) as Gold
    }
}
