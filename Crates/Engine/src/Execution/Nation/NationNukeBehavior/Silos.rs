// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: silos): SAM destruction and silo upgrades.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// On Impossible, when no good nuke target exists (score <= 0), tries to destroy enemy SAMs by
    /// overwhelming them with atom bombs: a SAM of level N intercepts N nukes before its cooldown.
    pub(super) fn maybe_destroy_enemy_sam(&mut self, game: &mut GameImpl, nuke_target: SmallId) {
        if game.config().is_unit_disabled(UnitType::AtomBomb) {
            return;
        }

        // Don't launch another salvo while atom bombs are in flight.
        if !game
            .player(self.player)
            .units_of(UnitType::AtomBomb)
            .is_empty()
        {
            return;
        }

        let atom_cost = self.cost(game, UnitType::AtomBomb);
        let enemy_sams = game.player(nuke_target).units_of(UnitType::SAMLauncher);
        if enemy_sams.is_empty() {
            return;
        }

        let our_silos: Vec<UnitId> = game
            .player(self.player)
            .units_of(UnitType::MissileSilo)
            .into_iter()
            .filter(|&silo| !game.unit_any(silo).is_under_construction())
            .collect();
        if our_silos.is_empty() {
            return;
        }

        // Try each enemy SAM as a target, easiest (lowest level) first.
        let mut sorted_sams = enemy_sams.clone();
        sorted_sams.sort_by_key(|&s| game.unit_any(s).level);
        let mut needs_more_silos = false;
        // The first failed attempt, to upgrade a silo that would have helped that plan.
        let mut failed_target: Option<FailedTarget> = None;

        for target_sam in sorted_sams {
            let target_tile = game.unit_any(target_sam).tile;
            // Hitting an ally's land would break the alliance.
            if game.has(Change::NationsLikeHumans5899)
                && self.blast_hits_friendly_land(game, target_tile, UnitType::AtomBomb, 0.0)
            {
                continue;
            }

            // All enemy SAMs whose range covers the target tile will try to intercept.
            let covering_sams = self.find_enemy_sams_covering_tile(game, target_tile);
            let covering_sam_ids: HashSet<UnitId> = covering_sams.iter().copied().collect();

            // Total interception capacity = sum of covering SAM levels.
            let mut total_interceptions = 0.0;
            for &sam in &covering_sams {
                total_interceptions += game.unit_any(sam).level as f64;
            }
            let bombs_needed = total_interceptions + 1.0;

            // NukeExecution picks the closest non-cooldown silo by Manhattan distance (via nukeSpawn);
            // the plan mirrors that order.
            let mut all_available_silos: Vec<AvailableSilo> = Vec::new();
            for &silo in &our_silos {
                let silo_unit = game.unit_any(silo);
                let available_slots =
                    silo_unit.level as i64 - silo_unit.missile_timer_queue().len() as i64;
                if available_slots <= 0 {
                    continue;
                }
                let interceptable = self.is_trajectory_interceptable_by_sam(
                    game,
                    silo_unit.tile,
                    target_tile,
                    Some(&covering_sam_ids),
                );
                // Actual parabolic flight time in ticks.
                let trajectory = Self::nuke_trajectory(game, silo_unit.tile, target_tile);
                if trajectory.is_empty() {
                    continue;
                }
                all_available_silos.push(AvailableSilo {
                    silo,
                    slots: available_slots,
                    flight_ticks: trajectory.len() as f64,
                    interceptable,
                });
            }

            // Sort by Manhattan distance to the target (matching nukeSpawn's pick order).
            all_available_silos.sort_by_key(|e| {
                game.map
                    .manhattan_dist(game.unit_any(e.silo).tile, target_tile)
            });

            // Per-bomb launch sequence matching NukeExecution's order: each silo contributes its slots in turn.
            let mut launch_sequence: Vec<(f64, bool)> = Vec::new();
            for entry in &all_available_silos {
                for _ in 0..entry.slots {
                    launch_sequence.push((entry.flight_ticks, entry.interceptable));
                }
            }

            // Half the SAM cooldown as the maximum total arrival spread, to be safe.
            let sam_cooldown = game.config().sam_cooldown() as f64;
            let max_total_arrival_spread = (sam_cooldown / 2.0).floor();

            // 1 extra bomb for every 5, for SAMs the enemy builds while the bombs fly.
            let extra_bombs = (bombs_needed / 5.0).floor();
            let total_bombs = bombs_needed + extra_bombs;

            // Bombs of silos whose trajectory is not blocked by SAMs other than the covering ones.
            let mut unblocked_bombs: Vec<(usize, f64)> = Vec::new();
            for (i, &(flight_ticks, interceptable)) in launch_sequence.iter().enumerate() {
                if !interceptable {
                    unblocked_bombs.push((i, flight_ticks));
                }
            }

            if (unblocked_bombs.len() as f64) < total_bombs {
                failed_target.get_or_insert(FailedTarget {
                    target_tile,
                    covering_sam_ids: covering_sam_ids.clone(),
                    total_bombs,
                });
                needs_more_silos = true;
                continue;
            }

            // Sort unblocked bombs by flight time to find the window of maxTotalArrivalSpread with the most bombs.
            let mut sorted_by_flight = unblocked_bombs.clone();
            sorted_by_flight.sort_by(|a, b| a.1.total_cmp(&b.1));

            let mut best_window_start = 0;
            let mut best_window_count = 0;
            for start in 0..sorted_by_flight.len() {
                let mut end = start;
                while end < sorted_by_flight.len()
                    && sorted_by_flight[end].1 - sorted_by_flight[start].1
                        <= max_total_arrival_spread
                {
                    end += 1;
                }
                if end - start > best_window_count {
                    best_window_count = end - start;
                    best_window_start = start;
                }
            }

            if (best_window_count as f64) < total_bombs {
                failed_target.get_or_insert(FailedTarget {
                    target_tile,
                    covering_sam_ids: covering_sam_ids.clone(),
                    total_bombs,
                });
                needs_more_silos = true;
                continue;
            }

            // From the window, the totalBombs bombs with the lowest launch-sequence indices (minimises the gold cost).
            let window_bombs =
                &sorted_by_flight[best_window_start..best_window_start + best_window_count];
            let mut window_by_index: Vec<(usize, f64)> = window_bombs.to_vec();
            window_by_index.sort_by_key(|b| b.0);
            let selected: Vec<(usize, f64)> = window_by_index
                .into_iter()
                .take(total_bombs as usize)
                .collect();
            let selected_set: HashSet<usize> = selected.iter().map(|b| b.0).collect();
            let last_selected_index = selected[selected.len() - 1].0;
            let bombs_to_fire = last_selected_index + 1;

            // Per-bomb waitTicks so all selected bombs arrive in the window, spread evenly from the earliest flight time.
            let mut selected_flight_min = f64::INFINITY;
            for b in &selected {
                selected_flight_min = selected_flight_min.min(b.1);
            }
            let stagger_interval = ((max_total_arrival_spread / total_bombs).floor()).max(1.0);
            let mut selected_idx = 0.0;
            let mut wait_ticks_per_bomb: Vec<f64> = Vec::new();
            for (i, launch) in launch_sequence.iter().enumerate().take(bombs_to_fire) {
                if selected_set.contains(&i) {
                    let target_arrival = selected_flight_min + selected_idx * stagger_interval;
                    wait_ticks_per_bomb.push((target_arrival - launch.0).max(0.0));
                    selected_idx += 1.0;
                } else {
                    // Wasted bomb (interceptable or out of the window): launch immediately.
                    wait_ticks_per_bomb.push(0.0);
                }
            }

            // Gold for all fired bombs (including the wasted ones).
            let total_cost = atom_cost * bombs_to_fire as Gold;
            if game.player(self.player).gold() < total_cost {
                continue;
            }

            // Fire the salvo: NukeExecution picks silos in the same Manhattan order as planned.
            for &wait_ticks in wait_ticks_per_bomb.iter().take(bombs_to_fire) {
                self.send_nuke(
                    game,
                    target_tile,
                    UnitType::AtomBomb,
                    nuke_target,
                    wait_ticks,
                );
            }
            return;
        }

        // Couldn't destroy any SAM: upgrade silos only if capacity was the bottleneck
        // (if only gold is lacking, don't waste it on upgrades).
        if game.has(Change::NationNukeRework31) {
            if needs_more_silos && let Some(failed) = failed_target {
                self.maybe_upgrade_helpful_silo(game, &failed);
            }
        } else if needs_more_silos {
            self.maybe_upgrade_best_protected_silo(game);
        }
    }

    /// All enemy SAMs whose range covers a given tile.
    pub(super) fn find_enemy_sams_covering_tile(
        &self,
        game: &GameImpl,
        tile: TileRef,
    ) -> Vec<UnitId> {
        let nearby_sams = game.nearby_units(
            tile,
            game.config().max_sam_range(),
            &[UnitType::SAMLauncher],
            None,
            false,
        );

        let mut result = Vec::new();
        for (sam, dist_squared) in nearby_sams {
            let unit = game.unit_any(sam);
            let owner = unit.owner;
            if owner == self.player || game.player(self.player).is_friendly(owner, false) {
                continue;
            }
            let range = game.config().sam_range(unit.level as f64);
            if dist_squared <= range * range {
                result.push(sam);
            }
        }
        result
    }

    /// Before v0.31: upgrades the silo best protected by our own SAMs.
    pub(super) fn maybe_upgrade_best_protected_silo(&mut self, game: &mut GameImpl) {
        let silos = game.player(self.player).units_of(UnitType::MissileSilo);
        if silos.is_empty() {
            return;
        }
        let our_sams = game.player(self.player).units_of(UnitType::SAMLauncher);
        let mut best_silo: Option<UnitId> = None;
        let mut best_protection = -1.0;
        for &silo in &silos {
            let silo_unit = game.unit_any(silo);
            if !game.player(self.player).can_upgrade_unit(silo) {
                continue;
            }
            let mut protection = 0.0;
            for &sam in &our_sams {
                let sam_unit = game.unit_any(sam);
                let range = game.config().sam_range(sam_unit.level as f64);
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(silo_unit.tile, sam_unit.tile)
                    as f64;
                if dist_squared <= range * range {
                    protection += sam_unit.level as f64;
                }
            }
            if protection > best_protection {
                best_protection = protection;
                best_silo = Some(silo);
            }
        }
        if let Some(silo) = best_silo {
            self.log.queue(
                game,
                self.player,
                Decision::Upgrade {
                    unit: silo,
                    amount: 1,
                },
            );
        }
    }

    /// Upgrades a silo that would have helped the failed overwhelm attempt: its trajectory to the failed
    /// target is not blocked by non-covering enemy SAMs and it is below the upgrade cap; among those,
    /// the one best protected by our own SAMs.
    pub(super) fn maybe_upgrade_helpful_silo(
        &mut self,
        game: &mut GameImpl,
        failed_target: &FailedTarget,
    ) {
        let silos = game.player(self.player).units_of(UnitType::MissileSilo);
        if silos.is_empty() {
            return;
        }

        // Silos with an unblocked trajectory to the failed target (not interceptable by non-covering SAMs).
        let mut unblocked_silos: Vec<UnitId> = Vec::new();
        for &silo in &silos {
            if !self.is_trajectory_interceptable_by_sam(
                game,
                game.unit_any(silo).tile,
                failed_target.target_tile,
                Some(&failed_target.covering_sam_ids),
            ) {
                unblocked_silos.push(silo);
            }
        }
        if unblocked_silos.is_empty() {
            return;
        }

        // Bail out if the target is unreachable even at max silo level.
        let max_achievable_slots = unblocked_silos.len() as f64 * MAX_NATION_SILO_UPGRADE_LEVEL;
        if max_achievable_slots < failed_target.total_bombs {
            return;
        }

        let our_sams = game.player(self.player).units_of(UnitType::SAMLauncher);
        let mut best_silo: Option<UnitId> = None;
        let mut best_protection = -1.0;

        for &silo in &unblocked_silos {
            let silo_unit = game.unit_any(silo);
            if silo_unit.level as f64 >= MAX_NATION_SILO_UPGRADE_LEVEL {
                continue;
            }
            if !game.player(self.player).can_upgrade_unit(silo) {
                continue;
            }

            let mut protection = 0.0;
            for &sam in &our_sams {
                let sam_unit = game.unit_any(sam);
                let range = game.config().sam_range(sam_unit.level as f64);
                let dist_squared = game
                    .map
                    .euclidean_dist_squared(silo_unit.tile, sam_unit.tile)
                    as f64;
                if dist_squared <= range * range {
                    protection += sam_unit.level as f64;
                }
            }

            if protection > best_protection {
                best_protection = protection;
                best_silo = Some(silo);
            }
        }

        if let Some(silo) = best_silo {
            self.log.queue(
                game,
                self.player,
                Decision::Upgrade {
                    unit: silo,
                    amount: 1,
                },
            );
        }
    }
}
