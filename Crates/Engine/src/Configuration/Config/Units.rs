// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/configuration/Config.ts (part: units): unit infos, costs, structures and warship constants.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl Config {
    /// Static data of a unit type (cost excluded, see [`Self::unit_cost`]).
    pub fn unit_info(&self, unit_type: UnitType) -> UnitInfo {
        let build = |ticks: u32| Some(if self.instant_build() { 0 } else { ticks });
        match unit_type {
            UnitType::Warship => UnitInfo {
                max_health: Some(1000.0),
                ..UnitInfo::default()
            },
            UnitType::Shell => UnitInfo {
                damage: Some(250.0),
                ..UnitInfo::default()
            },
            UnitType::Port => UnitInfo {
                construction_duration: build(if self.rules.has(Change::PortBuild50V31) {
                    5 * 10
                } else {
                    2 * 10
                }),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::MissileSilo => UnitInfo {
                construction_duration: build(10 * 10),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::DefensePost => UnitInfo {
                construction_duration: build(5 * 10),
                ..UnitInfo::default()
            },
            UnitType::SAMLauncher => UnitInfo {
                construction_duration: build(SAM_CONSTRUCTION_TICKS),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::City | UnitType::Factory => UnitInfo {
                construction_duration: build(2 * 10),
                upgradable: Some(true),
                ..UnitInfo::default()
            },
            UnitType::TransportShip
            | UnitType::SAMMissile
            | UnitType::AtomBomb
            | UnitType::HydrogenBomb
            | UnitType::MIRV
            | UnitType::MIRVWarhead
            | UnitType::TradeShip
            | UnitType::Construction
            | UnitType::Train => UnitInfo::default(),
        }
    }

    pub(super) fn cost_wrapped(
        &self,
        ctx: &CostContext,
        cost_fn: impl Fn(f64) -> f64,
        types: &[UnitType],
    ) -> Gold {
        if ctx.player_type == PlayerType::Human && self.has_infinite_gold_for(ctx.is_lobby_creator)
        {
            return 0;
        }
        let num_units: i64 = types.iter().map(|t| (ctx.owned_and_constructed)(*t)).sum();
        cost_fn((num_units + ctx.extra_units) as f64) as i64
    }

    /// Gold cost of building one more unit of `unit_type` (`UnitInfo.cost`).
    pub fn unit_cost(&self, unit_type: UnitType, ctx: &CostContext) -> Gold {
        match unit_type {
            UnitType::TransportShip
            | UnitType::Shell
            | UnitType::SAMMissile
            | UnitType::MIRVWarhead
            | UnitType::TradeShip
            | UnitType::Construction
            | UnitType::Train => 0,
            UnitType::Warship => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, (n + 1.0) * 250_000.0),
                &[UnitType::Warship],
            ),
            UnitType::Port => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::Port, UnitType::Factory],
            ),
            UnitType::AtomBomb => self.cost_wrapped(ctx, |_| 750_000.0, &[UnitType::AtomBomb]),
            UnitType::HydrogenBomb => {
                self.cost_wrapped(ctx, |_| 5_000_000.0, &[UnitType::HydrogenBomb])
            }
            UnitType::MIRV if !self.rules.has(Change::MirvCostScales2621) => {
                self.cost_wrapped(ctx, |_| 35_000_000.0, &[UnitType::MIRV])
            }
            UnitType::MIRV => {
                if ctx.player_type == PlayerType::Human
                    && self.has_infinite_gold_for(ctx.is_lobby_creator)
                {
                    return 0;
                }
                25_000_000 + ctx.mirvs_launched * 15_000_000
            }
            UnitType::MissileSilo => {
                self.cost_wrapped(ctx, |_| 1_000_000.0, &[UnitType::MissileSilo])
            }
            UnitType::DefensePost => self.cost_wrapped(
                ctx,
                |n| js_min(250_000.0, (n + 1.0) * 50_000.0),
                &[UnitType::DefensePost],
            ),
            UnitType::SAMLauncher => self.cost_wrapped(
                ctx,
                |n| js_min(3_000_000.0, (n + 1.0) * 1_500_000.0),
                &[UnitType::SAMLauncher],
            ),
            UnitType::City => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::City],
            ),
            UnitType::Factory => self.cost_wrapped(
                ctx,
                |n| js_min(1_000_000.0, self.pow2(n) * 125_000.0),
                &[UnitType::Factory, UnitType::Port],
            ),
        }
    }

    /// Minimum distance between structures.
    pub fn structure_min_dist(&self) -> f64 {
        15.0
    }

    /// Shell lifetime.
    pub fn shell_lifetime(&self) -> f64 {
        50.0
    }

    /// Warship patrol range.
    pub fn warship_patrol_range(&self) -> f64 {
        100.0
    }

    /// Warship targeting range.
    pub fn warship_targetting_range(&self) -> f64 {
        130.0
    }

    /// Ticks between warship shells.
    pub fn warship_shell_attack_rate(&self) -> f64 {
        20.0
    }

    /// Warship docking range.
    pub fn warship_docking_range(&self) -> f64 {
        5.0
    }

    /// Port healing per level.
    pub fn warship_port_healing_bonus_per_level(&self) -> f64 {
        5.0
    }

    /// Health percent at or below which a warship retreats.
    pub fn warship_retreat_health_percent(&self) -> f64 {
        75.0
    }

    /// Passive warship healing per tick.
    pub fn warship_passive_healing(&self) -> f64 {
        1.0
    }

    /// Range of passive warship healing.
    pub fn warship_passive_healing_range(&self) -> f64 {
        150.0
    }

    /// Ratio deciding whether a warship switches ports.
    pub fn warship_port_switch_threshold(&self) -> f64 {
        0.75
    }

    /// Maximum veterancy level.
    pub fn warship_max_veterancy(&self) -> f64 {
        if !self.rules.has(Change::WarshipVeterancy4433) {
            return 0.0;
        }
        3.0
    }

    /// Max-health bonus per veterancy level, integer percent.
    pub fn warship_veterancy_health_bonus(&self) -> f64 {
        20.0
    }

    /// Shell-damage bonus per veterancy level, integer percent.
    pub fn warship_veterancy_shell_damage_bonus(&self) -> f64 {
        20.0
    }

    /// Transport kills per veterancy level.
    pub fn warship_veterancy_transport_kills(&self) -> f64 {
        10.0
    }

    /// Trade captures per veterancy level.
    pub fn warship_veterancy_trade_captures(&self) -> f64 {
        25.0
    }

    /// Ticks between defense post shells.
    pub fn defense_post_shell_attack_rate(&self) -> f64 {
        100.0
    }

    /// Maximum safe-from-pirates cooldown.
    pub fn safe_from_pirates_cooldown_max(&self) -> f64 {
        20.0
    }

    /// Defense post targeting range.
    pub fn defense_post_targetting_range(&self) -> f64 {
        75.0
    }

    /// Ticks before alliance expiry when extension is offered.
    pub fn alliance_extension_prompt_offset(&self) -> Tick {
        300
    }
}
