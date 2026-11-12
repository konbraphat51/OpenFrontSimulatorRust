// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts (part: economy): gold, trains, trade ships and economy formulas.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
    /// Global train spawn throttle for `num_train_units` trains.
    pub fn train_saturation(&self, num_train_units: f64) -> f64 {
        let boost = if self.rules.has(Change::SmallEconomyBoost5907) {
            1.0
        } else {
            1.0 + 0.5 * self.exp(-num_train_units / 30.0)
        };
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
        let per_factory = if self.rules.has(Change::TrainTradeRetune30) {
            15.0
        } else if self.rules.has(Change::TrainSpawnRate25Beta2) {
            25.0
        } else {
            18.0
        };
        let rate = (num_player_factories + 10.0) * per_factory;
        if self.rules.has(Change::SmallEconomyBoost5907) {
            let boost = self.small_economy_spawn_boost(num_player_factories);
            return js_max(
                1.0,
                (rate / (self.train_saturation(num_train_units) * boost)).floor(),
            );
        }
        if !self.rules.has(Change::TradeTrainSaturation5302) {
            return rate;
        }
        js_max(1.0, (rate / self.train_saturation(num_train_units)).floor())
    }

    /// Per-unit spawn multiplier for a player's first few ports or factories (counted in levels).
    pub fn small_economy_spawn_boost(&self, levels: f64) -> f64 {
        if levels <= 0.0 {
            return 1.0;
        }
        1.0 + (0.5 * crate::Core::Util::within((6.0 - levels) / 5.0, 0.0, 1.0)) / levels
    }

    /// Gold of a train delivery.
    pub fn train_gold(
        &self,
        rel: TrainRelation,
        cities_visited: f64,
        is_lobby_creator: bool,
    ) -> Gold {
        let free_cities = if self.rules.has(Change::TrainTradeRetune30) {
            9.0
        } else {
            5.0
        };
        let cities_visited = if self.rules.has(Change::TrainDistanceGold30Beta) {
            js_max(0.0, cities_visited - free_cities)
        } else {
            0.0
        };
        let base_gold = match rel {
            TrainRelation::Ally => 35_000.0,
            TrainRelation::Team | TrainRelation::Other => 25_000.0,
            TrainRelation::Self_ => 10_000.0,
        };
        if !self.rules.has(Change::TrainDistanceGold30Beta) {
            // Flat gold: no distance penalty and no floor.
            return Util::to_int((base_gold * self.gold_multiplier_for(is_lobby_creator)).floor());
        }
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
        if self.rules.has(Change::TrainRange110Pr4079) {
            110.0
        } else {
            100.0
        }
    }

    /// Longest railroad.
    pub fn railroad_max_size(&self) -> f64 {
        #[allow(clippy::approx_constant)]
        let sqrt2_approx = 1.4142;
        if !self.rules.has(Change::RailroadMaxSizeDiagonal4406) {
            if !self.rules.has(Change::TrainRange110Pr4079) {
                return 120.0;
            }
            return self.train_station_max_range();
        }
        self.train_station_max_range() * sqrt2_approx
    }

    /// Gold of a trade ship delivery over `dist`.
    pub fn trade_ship_gold(&self, dist: f64, is_lobby_creator: bool) -> Gold {
        let debuff = self.trade_ship_short_range_debuff();
        let scale = if self.rules.has(Change::TrainTradeRetune30) {
            75_000.0
        } else {
            50_000.0
        };
        let base_gold = scale / (1.0 + self.exp(-0.03 * (dist - debuff))) + 50.0 * dist;
        let v = (base_gold * self.gold_multiplier_for(is_lobby_creator)).floor();
        assert!(v.is_finite(), "BigInt of a non-finite value throws");
        v as i64
    }

    /// Gold of a trade ship delivery before PR 3232: a port-count bonus on a doubled base.
    pub fn trade_ship_gold_with_ports(
        &self,
        dist: f64,
        num_ports: f64,
        is_lobby_creator: bool,
    ) -> Gold {
        let debuff = self.trade_ship_short_range_debuff();
        let per_tile = if self.rules.has(Change::TradeShipDistanceGold150) {
            150.0
        } else {
            100.0
        };
        let base_gold = 100_000.0 / (1.0 + self.exp(-0.03 * (dist - debuff))) + per_tile * dist;
        let num_port_bonus = num_ports - 1.0;
        // Hyperbolic decay, midpoint at 5 ports, 3x bonus max.
        let bonus = 1.0 + 2.0 * (num_port_bonus / (num_port_bonus + 5.0));
        let multiplier = self.gold_multiplier_for(is_lobby_creator);
        let v = (base_gold * bonus * multiplier).floor();
        assert!(v.is_finite(), "BigInt of a non-finite value throws");
        v as i64
    }

    /// Odds denominator of a trade ship spawn before PR 3232 (per-player port and ship counts).
    pub fn trade_ship_spawn_rate_per_player(
        &self,
        num_trade_ships: f64,
        num_player_ports: f64,
        num_player_trade_ships: f64,
    ) -> f64 {
        let base_spawn = if num_player_trade_ships < 3.0 {
            1.0
        } else {
            let decay_rate = std::f64::consts::LN_2 / 10.0;
            1.0 - self.sigmoid(num_trade_ships, decay_rate, 55.0)
        };
        let decay_rate = 1.0 / 10.0;
        let port_multiplier = 1.0 / (1.0 + decay_rate * num_player_ports);
        let combined = (base_spawn * port_multiplier).sqrt();
        (25.0 / combined).floor()
    }

    /// Global trade ship spawn throttle.
    pub fn trade_ship_saturation(&self, num_trade_ships: f64) -> f64 {
        let boost = if self.rules.has(Change::SmallEconomyBoost5907) {
            1.0
        } else {
            1.0 + 0.45 * self.exp(-num_trade_ships / 120.0)
        };
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
        player_port_levels: f64,
    ) -> f64 {
        let rejection_modifier = 1.0 / (trade_ship_spawn_rejections + 1.0);
        if self.rules.has(Change::SmallEconomyBoost5907) {
            let boost = self.small_economy_spawn_boost(player_port_levels);
            let base = 100.0 / (boost * boost);
            return js_max(
                1.0,
                ((base * rejection_modifier) / self.trade_ship_saturation(num_trade_ships)).floor(),
            );
        }
        if !self.rules.has(Change::TradeTrainSaturation5302) {
            let decay_rate = std::f64::consts::LN_2 / 50.0;
            let midpoint = if self.rules.has(Change::TradeShipSigmoid400Meta) {
                400.0
            } else {
                200.0
            };
            let base_spawn_rate = 1.0 - self.sigmoid(num_trade_ships, decay_rate, midpoint);
            return ((100.0 * rejection_modifier) / base_spawn_rate).floor();
        }
        js_max(
            1.0,
            ((100.0 * rejection_modifier) / self.trade_ship_saturation(num_trade_ships)).floor(),
        )
    }

    pub(super) fn has_infinite_gold_for(&self, is_lobby_creator: bool) -> bool {
        self.infinite_gold()
            || (self
                .game_config
                .host_cheats
                .as_ref()
                .and_then(|h| h.infinite_gold)
                .unwrap_or(false)
                && is_lobby_creator)
    }

    pub(super) fn has_infinite_troops_for(&self, is_lobby_creator: bool) -> bool {
        self.infinite_troops()
            || (self
                .game_config
                .host_cheats
                .as_ref()
                .and_then(|h| h.infinite_troops)
                .unwrap_or(false)
                && is_lobby_creator)
    }

    pub(super) fn gold_multiplier_for(&self, is_lobby_creator: bool) -> f64 {
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
            PlayerType::Human if self.rules.has(Change::ConquerGoldHalfHuman31) => gold / 2,
            PlayerType::Human => gold,
        }
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
}
