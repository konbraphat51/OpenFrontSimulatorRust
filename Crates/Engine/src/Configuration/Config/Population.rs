// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts (part: population): troop capacity and growth.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
    /// Starting troops of a player.
    pub fn start_manpower(&self, player_type: PlayerType, is_lobby_creator: bool) -> f64 {
        if player_type == PlayerType::Bot {
            return 10_000.0;
        }
        if player_type == PlayerType::Nation {
            if self.rules.has(Change::NationDifficultyEconomy27) {
                // Nations are built without a manifest strength here, so the multiplier is 1.
                return match self.game_config.difficulty {
                    Difficulty::Easy => 2_500.0,
                    Difficulty::Medium => 5_000.0,
                    Difficulty::Hard => 20_000.0,
                    Difficulty::Impossible => 50_000.0,
                };
            }
            if self.rules.has(Change::NationDifficultyEconomy28) {
                return match self.game_config.difficulty {
                    Difficulty::Easy => 18_750.0,
                    Difficulty::Medium => 25_000.0,
                    Difficulty::Hard => 31_250.0,
                    Difficulty::Impossible => 37_500.0,
                };
            }
            let rebalanced = self.rules.has(Change::NationDifficultyRebalance3184);
            return match self.game_config.difficulty {
                Difficulty::Easy if self.rules.has(Change::EasyNationsEasier3072) => 12_500.0,
                Difficulty::Easy => 18_750.0,
                Difficulty::Medium if rebalanced => 18_750.0,
                Difficulty::Medium => 25_000.0,
                Difficulty::Hard if rebalanced => 25_000.0,
                Difficulty::Hard => 28_125.0,
                Difficulty::Impossible => 31_250.0,
            };
        }
        // The onboarding tutorial's player starts about half full, where troops grow fastest.
        if self.rules.has(Change::TutorialSilo6242) && self.game_config.tutorial.unwrap_or(false) {
            return 60_000.0;
        }
        if self.has_infinite_troops_for(is_lobby_creator) {
            1_000_000.0
        } else {
            25_000.0
        }
    }

    /// True in the onboarding tutorial.
    pub fn tutorial(&self) -> bool {
        self.game_config.tutorial.unwrap_or(false)
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

        // The tutorial's nations and tribe never spend troops on attacks.
        if self.rules.has(Change::TutorialMode6204)
            && self.tutorial()
            && p.player_type != PlayerType::Human
        {
            return max_troops * 0.1;
        }

        if p.player_type == PlayerType::Bot {
            return max_troops / 3.0;
        }
        if p.player_type == PlayerType::Human {
            return max_troops;
        }
        if self.rules.has(Change::NationDifficultyEconomy27) {
            return match self.game_config.difficulty {
                Difficulty::Easy => max_troops * 0.5,
                Difficulty::Medium => max_troops * 1.0,
                Difficulty::Hard => max_troops * 1.5,
                Difficulty::Impossible => max_troops * 2.0,
            };
        }
        if self.rules.has(Change::NationDifficultyEconomy28) {
            return match self.game_config.difficulty {
                Difficulty::Easy => max_troops * 0.75,
                Difficulty::Medium => max_troops * 1.0,
                Difficulty::Hard => max_troops * 1.25,
                Difficulty::Impossible => max_troops * 1.5,
            };
        }
        let rebalanced = self.rules.has(Change::NationDifficultyRebalance3184);
        match self.game_config.difficulty {
            Difficulty::Easy if self.rules.has(Change::EasyNationsEasier3072) => max_troops * 0.5,
            Difficulty::Easy => max_troops * 0.75,
            Difficulty::Medium if rebalanced => max_troops * 0.75,
            Difficulty::Medium => max_troops * 1.0,
            Difficulty::Hard if rebalanced => max_troops * 1.0,
            Difficulty::Hard => max_troops * 1.125,
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
            to_add *= if self.rules.has(Change::BotTroopGrowth05V31) {
                0.5
            } else {
                0.6
            };
        }
        if p.player_type == PlayerType::Nation && self.rules.has(Change::NationDifficultyEconomy27)
        {
            match self.game_config.difficulty {
                Difficulty::Easy => to_add *= 0.9,
                Difficulty::Medium => to_add *= 1.0,
                Difficulty::Hard => to_add *= 1.1,
                Difficulty::Impossible => to_add *= 1.2,
            }
        } else if p.player_type == PlayerType::Nation
            && self.rules.has(Change::NationDifficultyEconomy28)
        {
            match self.game_config.difficulty {
                Difficulty::Easy => to_add *= 0.95,
                Difficulty::Medium => to_add *= 1.0,
                Difficulty::Hard => to_add *= 1.05,
                Difficulty::Impossible => to_add *= 1.1,
            }
        } else if p.player_type == PlayerType::Nation {
            let rebalanced = self.rules.has(Change::NationDifficultyRebalance3184);
            match self.game_config.difficulty {
                Difficulty::Easy if self.rules.has(Change::EasyNationsEasier3072) => to_add *= 0.9,
                Difficulty::Easy => to_add *= 0.95,
                Difficulty::Medium if rebalanced => to_add *= 0.95,
                Difficulty::Medium => to_add *= 1.0,
                Difficulty::Hard if rebalanced => to_add *= 1.0,
                Difficulty::Hard => to_add *= 1.025,
                Difficulty::Impossible => to_add *= 1.05,
            }
        }
        js_min(p.troops + to_add, max) - p.troops
    }
}
