// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/TransportShipExecution.ts: a transport ship carrying troops to a landing tile.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Motion plans, messages and stats are client plumbing and skipped. The discarded `findPath`
//! calls upstream makes to record motion plans are reduced to the freshness check they trigger
//! (`WaterPathFinder::touch`), because that check advances the rebuild stagger countdown.

use crate::Builds::Rules::Change;
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{PlayerType, SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Game::TransportShipUtils::target_transport_tile;
use crate::Pathfinding::PathFinder::{WaterPathFinder, WaterStepperEnv};
use crate::Pathfinding::Types::PathResult;

/// Percentage of troops lost when a boat lands on its owner's own tile (retreat).
const MALUS_FOR_RETREAT: f64 = 25.0;

/// Sends troops by sea to the closest reachable shore next to a target tile.
pub struct TransportShipExecution {
    active: bool,
    ticks_per_move: u32,
    last_move: Tick,
    target: SmallId,
    path_finder: Option<WaterPathFinder>,
    dst: Option<TileRef>,
    /// `null` (unresolved) / `false` (none found) / a tile.
    retreat_dst: Option<Option<TileRef>>,
    boat: Option<UnitId>,
    motion_plan_dst: Option<TileRef>,
    original_owner: SmallId,
    attacker: SmallId,
    tile_ref: TileRef,
    troops: Option<f64>,
}

impl TransportShipExecution {
    /// `new TransportShipExecution(attacker, ref, troops)`; `troops` is `None` when upstream gets null/undefined.
    pub fn new(attacker: SmallId, tile_ref: TileRef, troops: Option<f64>) -> Self {
        Self {
            active: true,
            ticks_per_move: 1,
            last_move: 0,
            target: 0,
            path_finder: None,
            dst: None,
            retreat_dst: None,
            boat: None,
            motion_plan_dst: None,
            original_owner: attacker,
            attacker,
            tile_ref,
            troops,
        }
    }

    fn reject_incoming_alliance_requests(&self, game: &mut GameImpl, target: SmallId) {
        let request = game
            .player(self.attacker)
            .incoming_alliance_requests()
            .into_iter()
            .find(|&ar| game.alliance_request(ar).requestor() == target);
        if let Some(request) = request {
            game.reject_request(request);
        }
    }
}

impl Execution for TransportShipExecution {
    fn active_during_spawn_phase(&self) -> bool {
        false
    }

    fn init(&mut self, game: &mut GameImpl, ticks: Tick) {
        if !game.is_valid_ref(self.tile_ref) {
            self.active = false;
            return;
        }

        self.last_move = ticks;
        self.target = game.owner_id(self.tile_ref);
        let stagger = game.transport_ship_stagger % WaterPathFinder::STAGGER_SPREAD;
        game.transport_ship_stagger += 1;
        self.path_finder = Some(WaterPathFinder::new(&game.water_manager, stagger));

        let boat_max = game.config().boat_max_number() as i64;
        if game
            .player(self.attacker)
            .unit_count(UnitType::TransportShip)
            >= boat_max
        {
            self.active = false;
            return;
        }

        if self.target != 0 {
            let target_type = game.player(self.target).player_type();
            if target_type != PlayerType::Bot
                && game.player(self.attacker).player_type() != PlayerType::Bot
            {
                self.reject_incoming_alliance_requests(game, self.target);
            }
        }

        if self.target == self.attacker && game.config().rules().has(Change::SelfActionGuards4014) {
            self.active = false;
            return;
        }

        if self.target != 0
            && !game
                .player(self.attacker)
                .can_attack_player(self.target, false)
        {
            self.active = false;
            return;
        }

        let attacker_troops = game.player(self.attacker).troops();
        let troops = self
            .troops
            .unwrap_or_else(|| game.config().boat_attack_amount(attacker_troops));
        let troops = troops.min(attacker_troops);
        self.troops = Some(troops);

        self.dst = target_transport_tile(game, self.attacker, self.tile_ref);
        let Some(dst) = self.dst else {
            self.active = false;
            return;
        };

        let Some(src) = game
            .player(self.attacker)
            .can_build(UnitType::TransportShip, dst, None)
        else {
            self.active = false;
            return;
        };

        let boat = game.player_mut(self.attacker).build_unit(
            UnitType::TransportShip,
            src,
            UnitParams {
                troops: Some(troops),
                target_tile: Some(dst),
                ..UnitParams::default()
            },
        );
        self.boat = Some(boat);
        self.motion_plan_dst = Some(dst);
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        if self.dst.is_none() {
            self.active = false;
            return;
        }
        if !self.active {
            return;
        }
        let boat = self.boat.expect("boat is built with dst");
        if !game.unit_any(boat).is_active() {
            self.active = false;
            return;
        }
        if ticks - self.last_move < self.ticks_per_move {
            return;
        }
        self.last_move = ticks;

        // A team mate who conquered a disconnected player now owns the ship: follow the owner
        let boat_owner = game.unit_any(boat).owner_id();
        if game.player(self.original_owner).is_disconnected()
            && boat_owner != self.original_owner
            && game.player(boat_owner).is_on_same_team(self.original_owner)
        {
            self.attacker = boat_owner;
            self.original_owner = boat_owner;
        }

        let path_finder = self.path_finder.as_mut().expect("set in init");
        if path_finder.rebuilt(&game.water_manager) {
            self.motion_plan_dst = None;
        }

        // Auto-retreat if the destination was destroyed by a nuke (turned to water)
        if let Some(dst) = self.dst
            && game.map.is_water(dst)
        {
            if !game.unit_any(boat).transport_ship_state().is_retreating {
                game.unit_mut(boat).update_transport_ship_state(true);
            }
            self.retreat_dst = None;
        }

        if game.unit_any(boat).transport_ship_state().is_retreating {
            // Resolve the retreat destination once, from the boat's location when the retreat begins
            let boat_tile = game.unit_any(boat).tile();
            let retreat_dst = *self.retreat_dst.get_or_insert_with(|| {
                game.player(self.attacker)
                    .best_transport_ship_spawn(boat_tile)
            });
            match retreat_dst {
                None => {
                    let troops = game.unit_any(boat).troops();
                    game.player_mut(self.attacker).add_troops(troops);
                    game.unit_mut(boat).delete(Some(false), None);
                    self.active = false;
                    return;
                }
                Some(retreat) => {
                    self.dst = Some(retreat);
                    if game.unit_any(boat).target_tile() != self.dst {
                        game.unit_mut(boat).set_target_tile(self.dst);
                    }
                }
            }
        }

        let dst = self.dst.expect("checked above");
        let boat_tile = game.unit_any(boat).tile();
        let result = {
            let mut env = WaterStepperEnv {
                water: &mut game.water_manager,
                map: &game.map,
                chain: None,
            };
            path_finder.next(&mut env, boat_tile, dst, None)
        };
        match result {
            PathResult::Complete(_) => {
                let boat_troops = game.unit_any(boat).troops();
                if game.owner_id(dst) == self.attacker {
                    let deaths = boat_troops * (MALUS_FOR_RETREAT / 100.0);
                    let survivors = boat_troops - deaths;
                    game.player_mut(self.attacker).add_troops(survivors);
                    game.unit_mut(boat).delete(Some(false), None);
                    self.active = false;
                    return;
                }
                game.player_mut(self.attacker).conquer(dst);
                if self.target != 0 && game.player(self.attacker).is_friendly(self.target, false) {
                    game.player_mut(self.attacker).add_troops(boat_troops);
                } else {
                    let target_id =
                        (self.target != 0).then(|| game.player(self.target).id().to_string());
                    game.add_execution(Box::new(AttackExecution::new(
                        Some(boat_troops),
                        self.attacker,
                        target_id,
                        Some(dst),
                        false,
                    )));
                }
                game.unit_mut(boat).delete(Some(false), None);
                self.active = false;
            }
            PathResult::Next(node) => {
                game.unit_mut(boat).move_to(node);
                // Upstream re-queries the path for the motion plan when the destination changed
                if self.dst != self.motion_plan_dst {
                    path_finder.touch(&game.water_manager);
                    self.motion_plan_dst = self.dst;
                }
            }
            PathResult::NotFound => {
                let troops = game.unit_any(boat).troops();
                game.player_mut(self.attacker).add_troops(troops);
                game.unit_mut(boat).delete(Some(false), None);
                self.active = false;
            }
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }
}
