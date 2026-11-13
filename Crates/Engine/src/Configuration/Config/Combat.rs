// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts and src/core/configuration/DefaultConfig.ts (part: combat): attack logic and attack amounts.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
    pub(super) fn large_territory_bonus(&self, num_tiles: f64, depth: f64) -> f64 {
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
                mag *= if self.rules.has(Change::BotAttackMalus07V31) {
                    0.7
                } else {
                    0.8
                };
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
        let attacker_troop_loss = if !self.rules.has(Change::AttackerLossBlend3227) {
            current_attacker_loss
        } else if self.rules.has(Change::AttackerLoss6040V31) {
            0.6 * current_attacker_loss + 0.4 * alt_attacker_loss
        } else if self.rules.has(Change::AttackerLoss7030V30) {
            0.7 * current_attacker_loss + 0.3 * alt_attacker_loss
        } else {
            0.5 * current_attacker_loss + 0.5 * alt_attacker_loss
        };
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
}
