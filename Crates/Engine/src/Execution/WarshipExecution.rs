// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/WarshipExecution.ts: a warship that patrols, shoots, hunts trade ships and repairs at ports.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `Execution::is_active` has no game access, so the execution cannot ask its unit directly like
//! upstream's `warship?.isActive()`. It re-checks the unit at the start of every tick instead; a
//! warship destroyed mid-tick is therefore dropped one tick later than upstream (no state change).

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::find_minimum_by;
use crate::Execution::Execution::Execution;
use crate::Execution::ShellExecution::ShellExecution;
use crate::Game::Game::{SmallId, Tick, UnitId, UnitParams, UnitType, WarshipMode};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Legacy28::PathFinding::PathFinder as LegacyPathFinder;
use crate::Pathfinding::PathFinder::{WaterPathFinder, WaterStepperEnv};
use crate::Pathfinding::Types::PathResult;
use std::collections::HashSet;

/// What the execution was created from: params of a new warship, or an existing unit.
enum Input {
    /// `UnitParams<Warship> & OwnerComp`.
    Params {
        owner: SmallId,
        patrol_tile: TileRef,
    },
    /// An existing unit.
    Unit(UnitId),
}

/// Drives one warship.
pub struct WarshipExecution {
    input: Input,
    random: PseudoRandom,
    warship: Option<UnitId>,
    pathfinder: Option<WaterPathFinder>,
    pub(super) legacy_pathfinder: Option<LegacyPathFinder>,
    pub(super) last_shell_attack: f64,
    pub(super) already_sent_shell: HashSet<UnitId>,
    last_manual_move_tick_retreat_disabled: f64,
    last_observed_patrol_tile: Option<TileRef>,
    active_healing_remainder: f64,
    active: bool,
}

impl WarshipExecution {
    /// `new WarshipExecution({ owner, patrolTile })`: builds the warship at the owner's closest port on init.
    pub fn new(owner: SmallId, patrol_tile: TileRef) -> Self {
        Self::with_input(Input::Params { owner, patrol_tile })
    }

    /// `new WarshipExecution(unit)`: drives an existing warship.
    pub fn from_unit(unit: UnitId) -> Self {
        Self::with_input(Input::Unit(unit))
    }

    fn with_input(input: Input) -> Self {
        Self {
            input,
            random: PseudoRandom::new(0.0),
            warship: None,
            pathfinder: None,
            legacy_pathfinder: None,
            last_shell_attack: 0.0,
            already_sent_shell: HashSet::new(),
            last_manual_move_tick_retreat_disabled: 0.0,
            last_observed_patrol_tile: None,
            active_healing_remainder: 0.0,
            active: true,
        }
    }

    pub(super) fn ship(&self) -> UnitId {
        self.warship.expect("warship exists after init")
    }

    fn state(&self, game: &GameImpl) -> WarshipMode {
        game.unit_any(self.ship()).warship_state().state
    }

    pub(super) fn patrol_tile(&self, game: &GameImpl) -> Option<TileRef> {
        game.unit_any(self.ship()).warship_state().patrol_tile
    }

    fn retreat_port(&self, game: &GameImpl) -> Option<TileRef> {
        game.unit_any(self.ship()).warship_state().retreat_port
    }

    pub(super) fn owner(&self, game: &GameImpl) -> SmallId {
        game.unit_any(self.ship()).owner_id()
    }

    /// Ends the execution (its warship is gone).
    pub(super) fn set_inactive(&mut self) {
        self.active = false;
    }

    /// `isDocked`.
    pub fn is_docked(&self, game: &GameImpl) -> bool {
        self.warship.is_some() && self.state(game) == WarshipMode::Docked
    }

    fn heal_warship(&mut self, game: &mut GameImpl) {
        let owner = self.owner(game);
        // A doomed side cannot repair its navy
        if game.player(owner).in_doomsday_clock() {
            return;
        }
        let passive_healing = game.config().warship_passive_healing();
        let range = game.config().warship_passive_healing_range();
        let range_squared = range * range;
        let warship_tile = game.unit_any(self.ship()).tile();

        let mut is_near_port = false;
        for port in game.player(owner).units_of(UnitType::Port) {
            let dist_squared = game
                .map
                .euclidean_dist_squared(warship_tile, game.unit_any(port).tile());
            if dist_squared as f64 <= range_squared {
                is_near_port = true;
                break;
            }
        }

        if is_near_port {
            game.unit_mut(self.ship())
                .modify_health(passive_healing, None);
        }

        if self.state(game) == WarshipMode::Docked {
            self.apply_active_docked_healing(game);
        }
    }

    fn is_fully_healed(&self, game: &GameImpl) -> bool {
        let unit = game.unit_ref(self.ship());
        if !unit.has_health() {
            return true;
        }
        unit.health() >= unit.max_health()
    }

    fn should_start_repair_retreat(&self, game: &GameImpl, health_before_healing: f64) -> bool {
        if self.state(game) != WarshipMode::Patrolling {
            return false;
        }
        // A doomed side cannot repair, so there is nothing to retreat for
        if game.player(self.owner(game)).in_doomsday_clock() {
            return false;
        }
        const MANUAL_MOVE_RETREAT_DISABLED_DURATION: f64 = 50.0;
        if game.ticks() as f64 - self.last_manual_move_tick_retreat_disabled
            < MANUAL_MOVE_RETREAT_DISABLED_DURATION
        {
            return false;
        }
        // Percentage of the veterancy-adjusted max health, integer math
        let retreat_threshold = (game.unit_ref(self.ship()).max_health()
            * game.config().warship_retreat_health_percent()
            / 100.0)
            .floor();
        if health_before_healing >= retreat_threshold {
            return false;
        }
        !game
            .player(self.owner(game))
            .units_of(UnitType::Port)
            .is_empty()
    }

    fn find_nearest_port(&self, game: &GameImpl) -> Option<TileRef> {
        let ports = game.player(self.owner(game)).units_of(UnitType::Port);
        if ports.is_empty() {
            return None;
        }
        let warship_tile = game.unit_any(self.ship()).tile();
        let warship_component = game
            .water_manager
            .get_water_component(&game.map, warship_tile)
            .unwrap_or_else(|| panic!("Warship at tile {warship_tile} has no water component"));

        find_minimum_by(
            &ports,
            |&port| {
                game.map
                    .euclidean_dist_squared(warship_tile, game.unit_any(port).tile())
                    as f64
            },
            |&port| {
                let port_tile = game.unit_any(port).tile();
                let port_component = game
                    .water_manager
                    .get_water_component(&game.map, port_tile)
                    .unwrap_or_else(|| panic!("Port at tile {port_tile} has no water component"));
                port_component == warship_component
            },
        )
        .map(|&port| game.unit_any(port).tile())
    }

    fn find_retreat_aggro_target(&self, game: &GameImpl) -> Option<UnitId> {
        self.find_best_target(game, &[UnitType::TransportShip, UnitType::Warship], false)
    }

    fn find_target_unit(&self, game: &GameImpl) -> Option<UnitId> {
        self.find_best_target(
            game,
            &[
                UnitType::TransportShip,
                UnitType::Warship,
                UnitType::TradeShip,
            ],
            true,
        )
    }

    /// Shared target selection: nearby units of the given types, common exclusions, best by type
    /// priority (lower index first) then distance; trade ships get extra filters.
    fn find_best_target(
        &self,
        game: &GameImpl,
        types: &[UnitType],
        include_trade_ships: bool,
    ) -> Option<UnitId> {
        let config = game.config();
        let warship = self.ship();
        let owner = self.owner(game);
        let warship_tile = game.unit_any(warship).tile();

        let ships = game.nearby_units(
            warship_tile,
            config.warship_targetting_range(),
            types,
            None,
            false,
        );

        // Trade-ship-specific state, computed on the first trade ship that needs it
        let mut trade_state: Option<(bool, Option<TileRef>, f64)> = None;

        let mut best_unit: Option<UnitId> = None;
        let mut best_type_priority = 0;
        let mut best_dist_squared = 0.0;

        for (unit_id, dist_squared) in ships {
            let unit = game.unit_any(unit_id);
            if unit_id == warship
                || unit.owner_id() == owner
                || !game.player(owner).can_attack_player(unit.owner_id(), true)
                || self.already_sent_shell.contains(&unit_id)
                || (unit.unit_type() == UnitType::Warship
                    && unit.warship_state().state == WarshipMode::Docked)
            {
                continue;
            }

            let unit_type = unit.unit_type();

            if include_trade_ships && unit_type == UnitType::TradeShip {
                let (has_reachable_port, patrol_tile, patrol_range_squared) = *trade_state
                    .get_or_insert_with(|| {
                        let component = game
                            .water_manager
                            .get_water_component(&game.map, warship_tile);
                        let reachable_port_rule =
                            game.config().rules().has(Change::PirateReachablePort4458);
                        let has_reachable_port = if !reachable_port_rule {
                            game.player(owner).unit_count(UnitType::Port) > 0
                        } else {
                            component.is_some_and(|c| {
                                game.player(owner)
                                    .units_of(UnitType::Port)
                                    .iter()
                                    .any(|&p| {
                                        let port = game.unit_any(p);
                                        port.is_active()
                                            && !port.is_marked_for_deletion()
                                            && !port.is_under_construction()
                                            && game.water_manager.has_water_component(
                                                &game.map,
                                                port.tile(),
                                                c,
                                            )
                                    })
                            })
                        };
                        (
                            has_reachable_port,
                            self.patrol_tile(game),
                            config.warship_patrol_range().powi(2),
                        )
                    });
                let target_owner = unit.target_unit().map(|t| game.unit_any(t).owner_id());
                if !has_reachable_port
                    || patrol_tile.is_none()
                    || game.unit_ref(unit_id).is_safe_from_pirates()
                    || target_owner == Some(owner)
                    || target_owner.is_some_and(|t| game.player(t).is_friendly(owner, false))
                {
                    continue;
                }
                let patrol_tile = patrol_tile.expect("checked above");
                if !game.config().rules().has(Change::PirateReachablePort4458) {
                    let component = game
                        .water_manager
                        .get_water_component(&game.map, warship_tile);
                    if component.is_some_and(|c| {
                        !game
                            .water_manager
                            .has_water_component(&game.map, unit.tile(), c)
                    }) {
                        continue;
                    }
                }
                if game.map.euclidean_dist_squared(patrol_tile, unit.tile()) as f64
                    > patrol_range_squared
                {
                    continue;
                }
            }

            let type_priority = match unit_type {
                UnitType::TransportShip => 0,
                UnitType::Warship => 1,
                _ => 2,
            };

            if best_unit.is_none()
                || type_priority < best_type_priority
                || (type_priority == best_type_priority && dist_squared < best_dist_squared)
            {
                best_unit = Some(unit_id);
                best_type_priority = type_priority;
                best_dist_squared = dist_squared;
            }
        }

        best_unit
    }

    fn start_repair_retreat(&mut self, game: &mut GameImpl) {
        let Some(port_tile) = self.find_nearest_port(game) else {
            return;
        };
        let ship = self.ship();
        game.unit_mut(ship).update_warship_state(
            false,
            Some(WarshipMode::Retreating),
            None,
            Some(Some(port_tile)),
        );
        self.active_healing_remainder = 0.0;
        game.unit_mut(ship).set_target_unit(None);
    }

    fn cancel_repair_retreat(&mut self, game: &mut GameImpl, clear_target_tile: bool) {
        self.active_healing_remainder = 0.0;
        let ship = self.ship();
        game.unit_mut(ship).update_warship_state(
            false,
            Some(WarshipMode::Patrolling),
            None,
            Some(None),
        );
        if clear_target_tile {
            game.unit_mut(ship).set_target_tile(None);
        }
    }

    fn handle_manual_patrol_override(&mut self, game: &mut GameImpl) {
        let patrol_tile = self.patrol_tile(game);
        if self.last_observed_patrol_tile.is_some() && patrol_tile != self.last_observed_patrol_tile
        {
            self.last_manual_move_tick_retreat_disabled = game.ticks() as f64;
            if self.state(game) != WarshipMode::Patrolling {
                self.cancel_repair_retreat(game, false);
            }
        }
        self.last_observed_patrol_tile = patrol_tile;
    }

    fn handle_repair_retreat(&mut self, game: &mut GameImpl) -> bool {
        let ship = self.ship();
        if self.state(game) == WarshipMode::Patrolling {
            return false;
        }

        let retreat_aggro_target = self.find_retreat_aggro_target(game);
        if let Some(target) = retreat_aggro_target {
            game.unit_mut(ship).set_target_unit(Some(target));
            self.shoot_target(game);
            // Continue retreating toward the port even while firing back
        }

        if !self.refresh_retreat_port_tile(game) {
            self.cancel_repair_retreat(game, true);
            return false;
        }

        // Only clear the target when there is no active aggro target this tick
        if retreat_aggro_target.is_none() {
            game.unit_mut(ship).set_target_unit(None);
        }

        let Some(retreat_port_tile) = self.retreat_port(game) else {
            return false;
        };

        let docking_radius = game.config().warship_docking_range();
        let docking_radius_sq = docking_radius * docking_radius;
        let ship_tile = game.unit_any(ship).tile();
        let dist_to_port = game
            .map
            .euclidean_dist_squared(ship_tile, retreat_port_tile);

        if dist_to_port as f64 <= docking_radius_sq {
            // Dock if the port has capacity (this warship excluded from the count)
            let owner = self.owner(game);
            let port = game
                .player(owner)
                .units_of(UnitType::Port)
                .into_iter()
                .find(|&p| game.unit_any(p).tile() == retreat_port_tile);
            if let Some(port) = port
                && !self.is_port_full_of_healing(game, port, Some(ship))
            {
                game.unit_mut(ship).set_target_tile(None);
                game.unit_mut(ship).update_warship_state(
                    false,
                    Some(WarshipMode::Docked),
                    None,
                    None,
                );
                return true;
            }
            // Port is full: wait near it, but leave if already fully healed
            if self.is_fully_healed(game) {
                self.cancel_repair_retreat(game, true);
                return false;
            }
            return true;
        }

        game.unit_mut(ship).set_target_tile(Some(retreat_port_tile));
        let result = self.next_step(game, ship_tile, retreat_port_tile, None);
        match result {
            PathResult::Complete(node) => {
                game.unit_mut(ship).move_to(node);
                if node == retreat_port_tile {
                    game.unit_mut(ship).set_target_tile(None);
                }
            }
            PathResult::Next(node) => game.unit_mut(ship).move_to(node),
            PathResult::NotFound => {
                let new_port = self.find_nearest_available_port_tile(game);
                game.unit_mut(ship)
                    .update_warship_state(false, None, None, Some(new_port));
                if new_port.is_none() {
                    self.cancel_repair_retreat(game, true);
                }
            }
        }

        true
    }

    fn refresh_retreat_port_tile(&mut self, game: &mut GameImpl) -> bool {
        let ship = self.ship();
        let ports = game.player(self.owner(game)).units_of(UnitType::Port);
        if ports.is_empty() {
            return false;
        }

        let current_retreat_port = self.retreat_port(game);

        // Does the current retreat port still exist?
        let current_port_exists = current_retreat_port
            .is_some_and(|rp| ports.iter().any(|&p| game.unit_any(p).tile() == rp));

        if !current_port_exists {
            let new_port = self.find_nearest_available_port_tile(game);
            game.unit_mut(ship)
                .update_warship_state(false, None, None, Some(new_port));
            return new_port.is_some();
        }

        // Is the current port now full of healing ships (arrived warships not counted)?
        let current_port = ports
            .iter()
            .copied()
            .find(|&p| Some(game.unit_any(p).tile()) == current_retreat_port);
        if let Some(current_port) = current_port
            && self.is_port_full_of_healing(game, current_port, None)
        {
            if let Some(alternative) = self.find_nearest_available_port(game) {
                game.unit_mut(ship).update_warship_state(
                    false,
                    None,
                    None,
                    Some(Some(alternative)),
                );
            }
            return self.retreat_port(game).is_some();
        }

        // Is a significantly closer port available?
        if let Some(closer) = self.find_better_port_tile(game)
            && Some(closer) != current_retreat_port
        {
            game.unit_mut(ship)
                .update_warship_state(false, None, None, Some(Some(closer)));
            return true;
        }

        true
    }

    fn is_port_full_of_healing(
        &self,
        game: &GameImpl,
        port: UnitId,
        exclude_ship: Option<UnitId>,
    ) -> bool {
        let max_ships_healing = game.unit_any(port).level() as usize;
        self.docked_ships_at_port(game, port, exclude_ship).len() >= max_ships_healing
    }

    fn docked_ships_at_port(
        &self,
        game: &GameImpl,
        port: UnitId,
        exclude_ship: Option<UnitId>,
    ) -> Vec<UnitId> {
        let docking_radius = game.config().warship_docking_range();
        let owner = self.owner(game);

        game.nearby_units(
            game.unit_any(port).tile(),
            docking_radius,
            &[UnitType::Warship],
            None,
            false,
        )
        .into_iter()
        .map(|(unit, _)| unit)
        .filter(|&ship| {
            if exclude_ship == Some(ship) {
                return false;
            }
            let unit = game.unit_any(ship);
            if unit.owner_id() != owner {
                return false;
            }
            if unit.warship_state().state == WarshipMode::Patrolling {
                return false;
            }
            if unit.target_tile().is_some() {
                return false;
            }
            true
        })
        .collect()
    }

    fn apply_active_docked_healing(&mut self, game: &mut GameImpl) {
        let Some(docked_port) = self.current_retreat_port(game) else {
            return;
        };

        let docked_ships = self.docked_ships_at_port(game, docked_port, None);
        if !docked_ships.contains(&self.ship()) {
            return;
        }

        let healing_pool = game.unit_any(docked_port).level() as f64
            * game.config().warship_port_healing_bonus_per_level();
        if healing_pool <= 0.0 || docked_ships.is_empty() {
            return;
        }

        // Preserve fractional split healing over time with a per-ship remainder
        let active_healing = healing_pool / docked_ships.len() as f64;
        self.active_healing_remainder += active_healing;
        let integer_healing = self.active_healing_remainder.floor();
        if integer_healing <= 0.0 {
            return;
        }

        self.active_healing_remainder -= integer_healing;
        game.unit_mut(self.ship())
            .modify_health(integer_healing, None);
    }

    fn current_retreat_port(&self, game: &GameImpl) -> Option<UnitId> {
        let retreat_port = self.retreat_port(game)?;
        game.player(self.owner(game))
            .units_of(UnitType::Port)
            .into_iter()
            .find(|&port| game.unit_any(port).tile() == retreat_port)
    }

    /// Nearest port with free healing capacity in the warship's water component: (tile, distSquared).
    fn nearest_available_port_tile(
        &self,
        game: &GameImpl,
        exclude_ship: Option<UnitId>,
    ) -> Option<(TileRef, f64)> {
        let ports = game.player(self.owner(game)).units_of(UnitType::Port);
        let warship_tile = game.unit_any(self.ship()).tile();
        let warship_component = game
            .water_manager
            .get_water_component(&game.map, warship_tile)
            .unwrap_or_else(|| panic!("Warship at tile {warship_tile} has no water component"));

        let mut best_tile: Option<TileRef> = None;
        let mut best_distance = f64::INFINITY;

        for port in ports {
            if self.is_port_full_of_healing(game, port, exclude_ship) {
                continue;
            }

            let port_tile = game.unit_any(port).tile();
            if !game
                .water_manager
                .has_water_component(&game.map, port_tile, warship_component)
            {
                continue;
            }

            let distance = game.map.euclidean_dist_squared(warship_tile, port_tile) as f64;
            if distance < best_distance {
                best_distance = distance;
                best_tile = Some(port_tile);
            }
        }

        best_tile.map(|tile| (tile, best_distance))
    }

    fn find_nearest_available_port(&self, game: &GameImpl) -> Option<TileRef> {
        self.nearest_available_port_tile(game, None).map(|r| r.0)
    }

    fn find_better_port_tile(&self, game: &GameImpl) -> Option<TileRef> {
        let (tile, dist_squared) = self.nearest_available_port_tile(game, None)?;

        let mut current_distance = f64::INFINITY;
        if let Some(current_retreat_port) = self.retreat_port(game) {
            current_distance = game
                .map
                .euclidean_dist_squared(game.unit_any(self.ship()).tile(), current_retreat_port)
                as f64;
        }

        if dist_squared < current_distance * game.config().warship_port_switch_threshold() {
            return Some(tile);
        }
        None
    }

    fn find_nearest_available_port_tile(&self, game: &GameImpl) -> Option<TileRef> {
        self.nearest_available_port_tile(game, Some(self.ship()))
            .map(|r| r.0)
    }

    fn shoot_target(&mut self, game: &mut GameImpl) {
        let ship = self.ship();
        game.unit_mut(ship)
            .update_warship_state(true, None, None, None);
        let shell_attack_rate = game.config().warship_shell_attack_rate();
        let ticks = game.ticks() as f64;
        if ticks - self.last_shell_attack > shell_attack_rate {
            let target = game
                .unit_any(ship)
                .target_unit()
                .expect("shootTarget needs a target");
            if game.unit_any(target).unit_type() != UnitType::TransportShip {
                // Warships don't need to reload when attacking transport ships
                self.last_shell_attack = ticks;
            }
            let owner = self.owner(game);
            let ship_tile = game.unit_any(ship).tile();
            game.add_execution(Box::new(ShellExecution::new(
                ship_tile, owner, ship, target,
            )));
            if !game.unit_ref(target).has_health() {
                // Don't send multiple shells to a target that can be one-shotted
                self.already_sent_shell.insert(target);
                game.unit_mut(ship).set_target_unit(None);
            }
        }
    }

    fn hunt_down_trade_ship(&mut self, game: &mut GameImpl) {
        let ship = self.ship();
        game.unit_mut(ship)
            .update_warship_state(true, None, None, None);
        for _ in 0..2 {
            let target = game
                .unit_any(ship)
                .target_unit()
                .expect("hunt needs a target");
            let target_tile = game.unit_any(target).tile();
            let ship_tile = game.unit_any(ship).tile();
            let dist = game.map.manhattan_dist(ship_tile, target_tile);

            if dist <= 5 {
                self.capture_target(game, target);
                return;
            }

            // When close, the minimap's diagonal upscaled paths hinder convergence: move greedily
            if dist <= 20
                && let Some(next_tile) = self.best_neighbor_toward(game, target_tile)
            {
                game.unit_mut(ship).move_to(next_tile);
                continue;
            }

            match self.next_step(game, ship_tile, target_tile, Some(5.0)) {
                PathResult::Complete(_) => {
                    self.capture_target(game, target);
                    return;
                }
                PathResult::Next(node) => game.unit_mut(ship).move_to(node),
                PathResult::NotFound => {}
            }
        }
    }

    fn capture_target(&mut self, game: &mut GameImpl, target: UnitId) {
        let ship = self.ship();
        let owner = self.owner(game);
        game.player_mut(owner).capture_unit(target);
        game.unit_mut(ship).record_trade_capture();
        game.unit_mut(ship).set_target_unit(None);
    }

    fn best_neighbor_toward(&self, game: &GameImpl, target_tile: TileRef) -> Option<TileRef> {
        let warship_tile = game.unit_any(self.ship()).tile();
        let mut best: Option<TileRef> = None;
        let mut best_dist = game.map.manhattan_dist(warship_tile, target_tile);
        game.map.for_each_neighbor(warship_tile, |neighbor| {
            if !game.map.is_water(neighbor) {
                return;
            }
            let d = game.map.manhattan_dist(neighbor, target_tile);
            if d < best_dist {
                best_dist = d;
                best = Some(neighbor);
            }
        });
        best
    }

    pub(super) fn next_step(
        &mut self,
        game: &mut GameImpl,
        from: TileRef,
        to: TileRef,
        dist: Option<f64>,
    ) -> PathResult<TileRef> {
        let mut env = WaterStepperEnv {
            water: &mut game.water_manager,
            map: &game.map,
            chain: None,
        };
        self.pathfinder
            .as_mut()
            .expect("set in init")
            .next(&mut env, from, to, dist)
    }

    fn patrol(&mut self, game: &mut GameImpl) {
        let ship = self.ship();
        if game.unit_any(ship).target_tile().is_none() {
            let tile = self.random_tile(game, false);
            game.unit_mut(ship).set_target_tile(tile);
            if tile.is_none() {
                return;
            }
        }

        let ship_tile = game.unit_any(ship).tile();
        let target_tile = game.unit_any(ship).target_tile().expect("set above");
        match self.next_step(game, ship_tile, target_tile, None) {
            PathResult::Complete(node) => {
                game.unit_mut(ship).set_target_tile(None);
                game.unit_mut(ship).move_to(node);
            }
            PathResult::Next(node) => game.unit_mut(ship).move_to(node),
            PathResult::NotFound => game.unit_mut(ship).set_target_tile(None),
        }
    }

    /// `randomTile`: a water tile near the patrol tile in the warship's component.
    pub fn random_tile(&mut self, game: &GameImpl, allow_shoreline: bool) -> Option<TileRef> {
        let mut warship_patrol_range = game.config().warship_patrol_range();
        const MAX_ATTEMPT_BEFORE_EXPAND: u32 = 500;
        let mut attempts: u32 = 0;
        let mut expand_count: u32 = 0;

        // The warship's water component for the connectivity check
        let warship_component = game
            .water_manager
            .get_water_component(&game.map, game.unit_any(self.ship()).tile());

        let patrol_tile = self.patrol_tile(game);
        // An invalid patrol tile would spin the loop below forever
        let patrol_tile = match patrol_tile {
            Some(t) if game.is_valid_ref(t) => t,
            _ => return None,
        };

        while expand_count < 3 {
            let x = game.x(patrol_tile) as f64
                + self
                    .random
                    .next_int(-warship_patrol_range / 2.0, warship_patrol_range / 2.0);
            let y = game.y(patrol_tile) as f64
                + self
                    .random
                    .next_int(-warship_patrol_range / 2.0, warship_patrol_range / 2.0);
            let legacy = !game.config().rules().has(Change::WarshipDocking31);
            if legacy {
                // Before v0.31 fractional coordinates pass `isValidCoord`, index no tile and are rejected.
                let (w, h) = (game.map.width() as f64, game.map.height() as f64);
                if !(x >= 0.0 && x < w && y >= 0.0 && y < h) {
                    continue;
                }
                if x.fract() != 0.0 || y.fract() != 0.0 {
                    attempts += 1;
                    if attempts == MAX_ATTEMPT_BEFORE_EXPAND {
                        expand_count += 1;
                        attempts = 0;
                        warship_patrol_range += (warship_patrol_range / 2.0).floor();
                    }
                    continue;
                }
            }
            if !game.is_valid_coord(x as i32, y as i32) {
                continue;
            }
            let tile = game.tile_ref(x as i32, y as i32);
            let water = if game.config().rules().has(Change::WarshipDocking31) {
                game.map.is_water(tile)
            } else {
                game.map.is_ocean(tile)
            };
            let rejected = !water
                || (!allow_shoreline && game.map.is_shoreline(tile))
                // Water component connectivity
                || (!game.config().rules().has(Change::WarshipLegacy28)) && warship_component.is_some_and(|c| {
                    !game.water_manager.has_water_component(&game.map, tile, c)
                });
            if rejected {
                attempts += 1;
                if attempts == MAX_ATTEMPT_BEFORE_EXPAND {
                    expand_count += 1;
                    attempts = 0;
                    warship_patrol_range += (warship_patrol_range / 2.0).floor();
                }
                continue;
            }
            return Some(tile);
        }
        if !allow_shoreline {
            // Failed on the ocean: try again but allow shoreline
            return self.random_tile(game, true);
        }
        None
    }
}

impl Execution for WarshipExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.pathfinder = Some(WaterPathFinder::new(&game.water_manager, 0));
        if game.config().rules().has(Change::WarshipLegacy28) {
            self.legacy_pathfinder = Some(LegacyPathFinder::mini(10_000.0, true, 100.0));
        }
        self.random = PseudoRandom::for_rules(game.config().rules(), game.ticks() as f64);
        match self.input {
            Input::Unit(unit) => self.warship = Some(unit),
            Input::Params { owner, patrol_tile } => {
                let Some(spawn) =
                    game.player(owner)
                        .can_build(UnitType::Warship, patrol_tile, None)
                else {
                    self.active = false;
                    return;
                };
                self.warship = Some(game.player_mut(owner).build_unit(
                    UnitType::Warship,
                    spawn,
                    UnitParams {
                        patrol_tile: Some(patrol_tile),
                        ..UnitParams::default()
                    },
                ));
            }
        }
        self.last_observed_patrol_tile = self.patrol_tile(game);
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if game.config().rules().has(Change::WarshipLegacy28) {
            super::WarshipExecutionV28::tick_v28(self, game);
            return;
        }
        if !game.config().rules().has(Change::WarshipDocking31) {
            super::WarshipExecutionV30::tick_v30(self, game);
            return;
        }
        let ship = self.ship();
        if !game.unit_any(ship).is_active() {
            self.active = false;
            return;
        }
        if game.unit_any(ship).health() <= 0.0 {
            game.unit_mut(ship).delete(None, None);
            return;
        }
        let health_before_healing = game.unit_any(ship).health();

        self.heal_warship(game);
        self.handle_manual_patrol_override(game);

        // A doomed side cannot repair its navy: undock or abort any retreat and keep patrolling
        if game.player(self.owner(game)).in_doomsday_clock()
            && self.state(game) != WarshipMode::Patrolling
        {
            self.cancel_repair_retreat(game, true);
        }

        if self.state(game) == WarshipMode::Docked {
            if self.current_retreat_port(game).is_none() {
                self.cancel_repair_retreat(game, true);
            }
            if self.is_fully_healed(game) {
                self.cancel_repair_retreat(game, true);
            }
            if self.state(game) == WarshipMode::Docked {
                return;
            }
        }

        if self.handle_repair_retreat(game) {
            return;
        }

        // Priority 1: heal before doing anything else
        if self.should_start_repair_retreat(game, health_before_healing) {
            self.start_repair_retreat(game);
            if self.handle_repair_retreat(game) {
                return;
            }
        }

        let target = self.find_target_unit(game);
        game.unit_mut(ship).set_target_unit(target);

        let target_type = target.map(|t| game.unit_any(t).unit_type());

        // Priority 1: shoot a transport ship in range
        if target_type == Some(UnitType::TransportShip) {
            self.shoot_target(game);
            self.patrol(game);
            return;
        }

        // Priority 2: fight an enemy warship in range
        if target_type == Some(UnitType::Warship) {
            self.shoot_target(game);
            self.patrol(game);
            return;
        }

        // Priority 3: hunt a trade ship only when not healing and no enemy warship
        if target_type == Some(UnitType::TradeShip) {
            self.hunt_down_trade_ship(game);
            return;
        }

        self.patrol(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
