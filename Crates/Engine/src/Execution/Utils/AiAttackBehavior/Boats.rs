// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: boats): boat routes and landings.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    pub(super) fn send_land_attack(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
        fixed_troops: Option<f64>,
        opponent: Option<SmallId>,
    ) -> bool {
        let Some(troops) = self.calculate_attack_troops(
            game,
            target,
            |this, g, target_troops| {
                fixed_troops.unwrap_or_else(|| this.home_troops(g) - target_troops)
            },
            opponent,
        ) else {
            return false;
        };
        let target_id = Self::target_id(game, target);
        let exec = Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        ));
        self.send(game, exec, troops);
        true
    }

    pub(super) fn send_boat_attack(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        if Self::v2(game) {
            return self.send_boat_attack_v2(game, target);
        }
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return false;
        }
        let my_shore = self.shore_tiles(game, self.player);
        let their_shore = self.shore_tiles(game, target);
        let Some((_, closest_y)) = closest_two_tiles(
            &game.map,
            my_shore.iter().copied(),
            their_shore.iter().copied(),
        ) else {
            return false;
        };
        if game.has(Change::NationAiRework2984) && !can_build_truthy(game, self.player, closest_y) {
            return false;
        }
        let troops = if game.has(Change::AiBoatTroopsUnified4297) {
            self.calculate_attack_troops(game, target, |this, g, _| this.troops(g) / 5.0, None)
        } else {
            self.legacy_boat_troops(game, target)
        };
        let Some(troops) = troops else {
            return false;
        };
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            closest_y,
            Some(troops),
        )));
        true
    }

    pub(super) fn send_boat_attack_v2(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        let Some(route) = self.boat_route(game, target) else {
            return false;
        };
        // A beachhead boat is cheap enough to risk past a warship, one at a time.
        let beachhead = self.lands_beachheads(game);
        if route.blocker.is_some()
            && (!beachhead || game.player(self.player).unit_count(UnitType::TransportShip) > 0)
        {
            return false;
        }
        if beachhead {
            return self.send_beachhead(game, target, route.landing);
        }
        let Some(troops) = self.calculate_attack_troops(
            game,
            target,
            |this, g, _| this.home_troops(g) / 5.0,
            None,
        ) else {
            return false;
        };
        let exec = Box::new(TransportShipExecution::new(
            self.player,
            route.landing,
            Some(troops),
        ));
        self.send(game, exec, troops);
        true
    }

    /// Hard and Impossible nations boat a tiny force and attack by land as soon as it lands.
    pub(super) fn lands_beachheads(&self, game: &GameImpl) -> bool {
        let difficulty = game.config().game_config().difficulty;
        game.player(self.player).player_type() == PlayerType::Nation
            && (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
    }

    /// A 1% boat, if the land attack following its landing would be worth making now.
    pub(super) fn send_beachhead(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
        landing: TileRef,
    ) -> bool {
        let Some(troops) = self.calculate_attack_troops(
            game,
            target,
            |this, g, target_troops| this.home_troops(g) - target_troops,
            (target != 0).then_some(target),
        ) else {
            return false;
        };
        let boat_troops = f64::max(1.0, (self.home_troops(game) / 100.0).floor());
        // Only the boat leaves now: the land attack is budgeted once it lands.
        if target != 0 && game.player(target).player_type() == PlayerType::Bot {
            self.bot_attack_troops_sent += boat_troops - troops;
        }
        let exec = Box::new(TransportShipExecution::new(
            self.player,
            landing,
            Some(boat_troops),
        ));
        self.send(game, exec, boat_troops);
        true
    }

    pub(super) fn borders_by_land(&self, game: &GameImpl, target: SmallId) -> bool {
        match &self.land_neighbors {
            Some(set) => set.contains(&target),
            None => game.player(self.player).shares_border_with(target),
        }
    }

    /// By land, or by a boat that passes no hostile warship and is not sailing too far.
    pub(super) fn can_reach(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        if self.borders_by_land(game, target) {
            return true;
        }
        match self.boat_route(game, target) {
            Some(route) => route.blocker.is_none() && route.length <= self.max_boat_route(game),
            None => false,
        }
    }

    pub(super) fn boat_route(&mut self, game: &mut GameImpl, target: SmallId) -> Option<BoatRoute> {
        if let Some(routes) = &self.boat_routes
            && let Some((_, known)) = routes.iter().find(|(t, _)| *t == target)
        {
            return *known;
        }
        let route = self.plan_boat_route(game, target);
        if let Some(routes) = self.boat_routes.as_mut() {
            routes.push((target, route));
        }
        route
    }

    /// The direct crossing, or if a warship blocks it, the shortest safe one landing elsewhere.
    pub(super) fn plan_boat_route(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
    ) -> Option<BoatRoute> {
        if game.config().is_unit_disabled(UnitType::TransportShip)
            || game.player(self.player).unit_count(UnitType::TransportShip)
                >= game.config().boat_max_number() as i64
        {
            return None;
        }
        let ours = self.shore_tiles(game, self.player);
        let theirs = self.shore_tiles(game, target);
        let (_, closest_y) =
            closest_two_tiles(&game.map, ours.iter().copied(), theirs.iter().copied())?;
        let warships = self.hostile_warships(game);
        let direct = self.route_to(game, closest_y, &warships);
        if let Some(d) = direct
            && d.blocker.is_none()
        {
            return Some(d);
        }
        let mut best: Option<BoatRoute> = None;
        for landing in self.other_landings(game, &ours, &theirs, closest_y, &warships) {
            let route = self.route_to(game, landing, &warships);
            if let Some(r) = route
                && r.blocker.is_none()
                && best.is_none_or(|b| r.length < b.length)
            {
                best = Some(r);
            }
        }
        best.or(direct)
    }

    /// The launch follows from the landing: our shore closest to it by water.
    pub(super) fn route_to(
        &mut self,
        game: &mut GameImpl,
        landing: TileRef,
        warships: &[UnitId],
    ) -> Option<BoatRoute> {
        let launch = can_build_transport_ship(game, self.player, landing)?;
        let (length, blocker) = self.sail(game, launch, landing, Some(warships));
        Some(BoatRoute {
            landing,
            length,
            blocker,
        })
    }

    /// Landing spots on their coast apart from `tried` and clear of hostile warships, nearest to
    /// our coast first: Medium tries 1, Hard 2, Impossible 4.
    pub(super) fn other_landings(
        &self,
        game: &GameImpl,
        ours: &[TileRef],
        theirs: &[TileRef],
        tried: TileRef,
        warships: &[UnitId],
    ) -> Vec<TileRef> {
        let count = match game.config().game_config().difficulty {
            Difficulty::Easy => return Vec::new(),
            Difficulty::Medium => 1,
            Difficulty::Hard => 2,
            Difficulty::Impossible => 4,
        };
        let range = self.danger_range(game);
        let range_squared = range * range;
        let our_step = usize::max(1, ours.len() / 32);
        let our_sample: Vec<TileRef> = ours
            .iter()
            .enumerate()
            .filter(|(i, _)| i % our_step == 0)
            .map(|(_, &t)| t)
            .collect();
        let mut candidates: Vec<(TileRef, f64)> = Vec::new();
        let their_step = usize::max(1, theirs.len() / 32);
        let mut i = 0;
        while i < theirs.len() {
            let tile = theirs[i];
            i += their_step;
            if warships.iter().any(|&w| {
                game.map
                    .euclidean_dist_squared(game.unit_any(w).tile(), tile) as f64
                    <= range_squared
            }) {
                continue;
            }
            let mut dist = f64::INFINITY;
            for &s in &our_sample {
                dist = dist.min(game.map.manhattan_dist(s, tile) as f64);
            }
            candidates.push((tile, dist));
        }
        candidates.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(Ordering::Equal));
        let mut picked = vec![tried];
        for (tile, _) in candidates {
            if picked.len() > count {
                break;
            }
            if picked
                .iter()
                .any(|&p| game.map.manhattan_dist(p, tile) < 50)
            {
                continue;
            }
            picked.push(tile);
        }
        picked.split_off(1)
    }

    pub(super) fn hostile_warships(&self, game: &GameImpl) -> Vec<UnitId> {
        game.units_of(UnitType::Warship)
            .into_iter()
            .filter(|&w| {
                let unit = game.unit_any(w);
                unit.owner_id() != self.player
                    && !unit.is_under_construction()
                    && unit.warship_state().state != WarshipMode::Docked
                    && game
                        .player(unit.owner_id())
                        .can_attack_player(self.player, true)
            })
            .collect()
    }

    /// Warship targeting range, plus what it may move while our boat approaches.
    pub(super) fn danger_range(&self, game: &GameImpl) -> f64 {
        game.config().warship_targetting_range() + 20.0
    }

    /// Tiles a boat sails from `launch` to the shore at `target`, and the hostile warship that sinks it.
    /// Easy does not look, Medium checks both shores, Hard and Impossible pathfind.
    pub(super) fn sail(
        &self,
        game: &mut GameImpl,
        launch: TileRef,
        target: TileRef,
        warships: Option<&[UnitId]>,
    ) -> (f64, Option<UnitId>) {
        let landing = target_transport_tile(game, self.player, target).unwrap_or(target);
        let direct = game.map.manhattan_dist(launch, landing) as f64;
        let difficulty = game.config().game_config().difficulty;
        if game.player(self.player).player_type() == PlayerType::Bot
            || difficulty == Difficulty::Easy
        {
            return (direct, None);
        }
        let hostile;
        let warships: &[UnitId] = match warships {
            Some(w) => w,
            None => {
                hostile = self.hostile_warships(game);
                &hostile
            }
        };

        let path: Option<Vec<TileRef>> = if difficulty == Difficulty::Medium || direct <= 20.0 {
            None
        } else {
            find_water_path(
                &mut game.water_manager,
                &game.map,
                Source::Single(launch),
                landing,
            )
        };
        let route: Vec<TileRef> = path.clone().unwrap_or_else(|| vec![launch, landing]);
        let length = path.as_ref().map_or(direct, |p| p.len() as f64);
        // Without a path, assume the warship can fire as soon as we launch.
        let sailed = |i: usize| if path.is_none() { 0.0 } else { i as f64 };

        let range = self.danger_range(game);
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
        );
        for &t in &route {
            let (x, y) = (game.map.x(t) as f64, game.map.y(t) as f64);
            min_x = min_x.min(x);
            max_x = max_x.max(x);
            min_y = min_y.min(y);
            max_y = max_y.max(y);
        }
        for &w in warships {
            let w_tile = game.unit_any(w).tile();
            let (wx, wy) = (game.map.x(w_tile) as f64, game.map.y(w_tile) as f64);
            if wx < min_x - range || wx > max_x + range || wy < min_y - range || wy > max_y + range
            {
                continue;
            }
            for (i, &r) in route.iter().enumerate() {
                if game.map.euclidean_dist_squared(w_tile, r) as f64 > range * range {
                    continue;
                }
                // Shell and boat close in by at most 4 tiles a tick; 10 tiles spare for its reaction.
                let ticks_left = length - sailed(i);
                if 4.0 * ticks_left >= game.map.manhattan_dist(w_tile, r) as f64 - 10.0 {
                    return (length, Some(w));
                }
                break;
            }
        }
        (length, None)
    }

    /// Hard and Impossible do not sail further than this to attack someone.
    pub(super) fn max_boat_route(&self, game: &GameImpl) -> f64 {
        match game.config().game_config().difficulty {
            Difficulty::Easy | Difficulty::Medium => f64::INFINITY,
            Difficulty::Hard => 300.0,
            Difficulty::Impossible => 500.0,
        }
    }

    /// Enemies across the water (not bordering us) whose main territory is within boat range of ours.
    pub(super) fn overseas_targets(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Vec<SmallId> {
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return Vec::new();
        }
        let range = self.max_boat_route(game);
        let ours = game.player(self.player).largest_cluster_bounding_box;
        game.players()
            .into_iter()
            .filter(|&p| {
                p != self.player
                    && game.player(p).player_type() != PlayerType::Bot
                    && !self.is_friendly(game, p)
                    && !bordering_enemies.contains(&p)
                    && (!self.is_ffa(game) || game.player(p).troops() < self.troops(game))
                    && box_gap(ours, game.player(p).largest_cluster_bounding_box) <= range
            })
            .collect()
    }

    /// Hard and Impossible send warships at whatever blocks the juiciest target they could not reach.
    /// Without a land front any distance counts: the sea is the only way to attack.
    pub(super) fn clear_blocked_lane(
        &self,
        game: &GameImpl,
        routes: &[(SmallId, Option<BoatRoute>)],
        land_front: bool,
    ) -> Option<UnitId> {
        let difficulty = game.config().game_config().difficulty;
        if difficulty != Difficulty::Hard && difficulty != Difficulty::Impossible {
            return None;
        }
        let max_route = self.max_boat_route(game);
        let mut blocked: Vec<(SmallId, UnitId)> = Vec::new();
        for (target, route) in routes {
            let Some(route) = route else { continue };
            let Some(blocker) = route.blocker else {
                continue;
            };
            if (!land_front || route.length <= max_route)
                && (!self.is_ffa(game) || game.player(*target).troops() < self.troops(game))
            {
                blocked.push((*target, blocker));
            }
        }
        let keys: Vec<SmallId> = blocked.iter().map(|(t, _)| *t).collect();
        let target = find_juiciest_target(game, &keys)?;
        blocked.iter().find(|(t, _)| *t == target).map(|(_, b)| *b)
    }

    /// Boat troops before PR 4297: no reserve subtraction for bot targets and the cap always applies.
    pub(super) fn legacy_boat_troops(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
    ) -> Option<f64> {
        let fifth = self.troops(game) / 5.0;
        let mut troops = if game.player(target).player_type() == PlayerType::Bot {
            self.calculate_bot_attack_troops(game, target, fifth)
        } else {
            fifth
        };
        troops = troops.min(self.troop_send_cap(game, None));
        if troops < 1.0 {
            return None;
        }
        if self.is_attack_too_weak(game, troops, target) {
            return None;
        }
        if game.player(self.player).player_type() == PlayerType::Nation {
            let emoji = self.emoji_behavior.clone().expect("not initialized");
            emoji.borrow_mut().maybe_send_attack_emoji(game, target);
        }
        Some(troops)
    }
}
