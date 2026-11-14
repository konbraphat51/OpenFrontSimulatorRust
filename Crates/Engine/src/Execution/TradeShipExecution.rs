// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TradeShipExecution.ts: a trade ship sailing between two ports.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Motion plans, messages and stats are client plumbing and skipped; the freshness check of the
//! motion-plan path query is kept (`WaterPathFinder::touch`) because it advances the rebuild
//! stagger. The memoized path cache of upstream's trade ship finder cannot change results.

use crate::Builds::Rules::Change;
use crate::Core::Util::find_closest_by;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Legacy28::PathFinding::{PathFinder as LegacyPathFinder, StepResult};
use crate::Pathfinding::PathFinder::{WaterPathFinder, WaterStepperEnv};
use crate::Pathfinding::Types::{PathResult, Source};

/// Moves a trade ship from `src_port` to `dst_port` and pays both owners on arrival.
pub struct TradeShipExecution {
    active: bool,
    trade_ship: Option<UnitId>,
    was_captured: bool,
    path_finder: Option<WaterPathFinder>,
    legacy_path_finder: Option<LegacyPathFinder>,
    tiles_traveled: f64,
    motion_plan_dst: Option<TileRef>,
    orig_owner: SmallId,
    src_port: UnitId,
    dst_port: UnitId,
}

impl TradeShipExecution {
    /// `new TradeShipExecution(origOwner, srcPort, dstPort)`.
    pub fn new(orig_owner: SmallId, src_port: UnitId, dst_port: UnitId) -> Self {
        Self {
            active: true,
            trade_ship: None,
            was_captured: false,
            path_finder: None,
            legacy_path_finder: None,
            tiles_traveled: 0.0,
            motion_plan_dst: None,
            orig_owner,
            src_port,
            dst_port,
        }
    }

    /// `dstPort()`: the destination port's tile.
    pub fn dst_port_tile(&self, game: &GameImpl) -> TileRef {
        game.unit_any(self.dst_port).tile()
    }

    fn complete(&mut self, game: &mut GameImpl) {
        self.active = false;
        let ship = self.trade_ship.expect("ship exists when completing");
        game.unit_mut(ship).delete(Some(false), None);
        let ship_owner = game.unit_any(ship).owner_id();
        let is_lobby_creator = game.player(ship_owner).is_lobby_creator();
        let gold = if game.has(Change::TradeShipMeta3232) {
            game.config()
                .trade_ship_gold(self.tiles_traveled, is_lobby_creator)
        } else {
            let ports = game.player(ship_owner).unit_count(UnitType::Port);
            game.config().trade_ship_gold_with_ports(
                self.tiles_traveled,
                ports as f64,
                is_lobby_creator,
            )
        };

        if self.was_captured && ship_owner == self.orig_owner {
            // Retaken by its original owner: the payout stands, but nobody pirated it
            game.player_mut(self.orig_owner).add_gold(gold);
        } else if self.was_captured {
            game.player_mut(ship_owner).add_gold(gold);
            game.player_mut(ship_owner).add_piracy_gold(gold);
        } else {
            let src_owner = game.unit_any(self.src_port).owner_id();
            let dst_owner = game.unit_any(self.dst_port).owner_id();
            game.player_mut(src_owner).add_gold(gold);
            game.player_mut(dst_owner).add_gold(gold);
            game.player_mut(src_owner).add_trade_gold(gold);
            game.player_mut(dst_owner).add_trade_gold(gold);
        }
    }
}

impl Execution for TradeShipExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let stagger = game.trade_ship_stagger % WaterPathFinder::STAGGER_SPREAD;
        game.trade_ship_stagger += 1;
        self.path_finder = Some(WaterPathFinder::new(&game.water_manager, stagger));
        if game.has(Change::LegacyAStar28) {
            self.legacy_path_finder = Some(
                LegacyPathFinder::mini(2500.0, true, 20.0)
                    .with_keep_path_on_arrival(game.has(Change::PathFinderKeepsPath26)),
            );
        }
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        let rebuilt = self
            .path_finder
            .as_mut()
            .expect("set in init")
            .rebuilt(&game.water_manager);
        if rebuilt {
            self.motion_plan_dst = None;
        }

        if self.trade_ship.is_none() {
            let src_tile = game.unit_any(self.src_port).tile();
            let Some(spawn) =
                game.player(self.orig_owner)
                    .can_build(UnitType::TradeShip, src_tile, None)
            else {
                self.active = false;
                return;
            };
            self.trade_ship = Some(game.player_mut(self.orig_owner).build_unit(
                UnitType::TradeShip,
                spawn,
                UnitParams {
                    target_unit: Some(self.dst_port),
                    last_set_safe_from_pirates: Some(ticks as f64),
                    ..UnitParams::default()
                },
            ));
        }
        let ship = self.trade_ship.expect("built above");

        if !game.unit_any(ship).is_active() {
            self.active = false;
            return;
        }

        let ship_owner = game.unit_any(ship).owner_id();
        let dst_port_owner = game.unit_any(self.dst_port).owner_id();
        if !self.was_captured && self.orig_owner != ship_owner {
            // Remembered in case the original owner recaptures the ship
            self.was_captured = true;
        }

        // A player who captures the port the ship left from while it trades loses the ship
        if dst_port_owner == game.unit_any(self.src_port).owner_id() {
            game.unit_mut(ship).delete(Some(false), None);
            self.active = false;
            return;
        }

        if !self.was_captured
            && (!game.unit_any(self.dst_port).is_active()
                || !game.player(ship_owner).can_trade(dst_port_owner))
        {
            game.unit_mut(ship).delete(Some(false), None);
            self.active = false;
            return;
        }

        let cur_tile = game.unit_any(ship).tile();

        if self.was_captured
            && (ship_owner != dst_port_owner || !game.unit_any(self.dst_port).is_active())
        {
            let my_component = game.water_manager.get_water_component(&game.map, cur_tile);
            let ports = game.player(ship_owner).units_of(UnitType::Port);
            let reachable = game.has(Change::TradeWaterComponents31);
            let nearest = find_closest_by(
                &ports,
                |&port| {
                    game.map
                        .manhattan_dist(game.unit_any(port).tile(), cur_tile)
                        as f64
                },
                |&port| {
                    if !reachable {
                        // `sort(distSortUnit)` upstream: every port counts, the first nearest wins.
                        return true;
                    }
                    let p = game.unit_any(port);
                    p.is_active()
                        && !p.is_marked_for_deletion()
                        && !p.is_under_construction()
                        && my_component.is_some_and(|c| {
                            game.water_manager
                                .has_water_component(&game.map, p.tile(), c)
                        })
                },
            )
            .copied();
            match nearest {
                None => {
                    game.unit_mut(ship).delete(Some(false), None);
                    self.active = false;
                    return;
                }
                Some(port) => {
                    self.dst_port = port;
                    game.unit_mut(ship).set_target_unit(Some(port));
                }
            }
        }

        if cur_tile == self.dst_port_tile(game) {
            self.complete(game);
            return;
        }

        let dst = self.dst_port_tile(game);
        if let Some(legacy) = self.legacy_path_finder.as_mut() {
            let result =
                legacy.next_tile(&game.map, game.water_manager.mini_map(), cur_tile, dst, 1);
            match result {
                StepResult::Pending => {}
                StepResult::NextTile(node) => {
                    if game.map.is_water(node) && game.map.is_shoreline(node) {
                        game.unit_mut(ship).set_safe_from_pirates();
                    }
                    game.unit_mut(ship).move_to(node);
                    self.tiles_traveled += 1.0;
                }
                StepResult::Completed(_) => self.complete(game),
                StepResult::PathNotFound => {
                    if game.unit_any(ship).is_active() {
                        game.unit_mut(ship).delete(Some(false), None);
                    }
                    self.active = false;
                }
            }
            return;
        }
        let result = {
            let mut env = WaterStepperEnv {
                water: &mut game.water_manager,
                map: &game.map,
                chain: None,
            };
            self.path_finder
                .as_mut()
                .expect("set in init")
                .next(&mut env, cur_tile, dst, None)
        };

        match result {
            PathResult::Next(node) => {
                if Some(dst) != self.motion_plan_dst {
                    if game.has(Change::TradeShipPathReuse4654) {
                        self.path_finder
                            .as_mut()
                            .expect("set in init")
                            .touch(&game.water_manager);
                    } else {
                        // Older builds run a real one-shot query for the motion plan.
                        let mut env = WaterStepperEnv {
                            water: &mut game.water_manager,
                            map: &game.map,
                            chain: None,
                        };
                        let _ = self.path_finder.as_mut().expect("set in init").find_path(
                            &mut env,
                            Source::Single(node),
                            dst,
                        );
                    }
                    self.motion_plan_dst = Some(dst);
                }
                // Update safeFromPirates status
                if game.map.is_water(node) && game.map.is_shoreline(node) {
                    game.unit_mut(ship).set_safe_from_pirates();
                }
                game.unit_mut(ship).move_to(node);
                self.tiles_traveled += 1.0;
            }
            PathResult::Complete(_) => {
                self.complete(game);
            }
            PathResult::NotFound => {
                if game.unit_any(ship).is_active() {
                    game.unit_mut(ship).delete(Some(false), None);
                }
                self.active = false;
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
