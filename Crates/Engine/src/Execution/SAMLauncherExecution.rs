// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/SAMLauncherExecution.ts: SAM launcher targeting and missile launches.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::Execution::Execution;
use crate::Execution::SAMMissileExecution::SAMMissileExecution;
use crate::Game::Game::{GameType, SmallId, Tick, UnitId, UnitParams, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::collections::HashMap;

struct Target {
    unit: UnitId,
    tile: TileRef,
    score: f64,
}

struct InterceptionTile {
    tile: TileRef,
    tick: f64,
}

/// Cached interception state of one nuke; `tick` >= 0 is the scheduled launch tick, -1 is
/// unreachable at the current upgrade, -2 is permanently out of reach.
struct CachedInterception {
    tick: f64,
    tile: TileRef,
    min_dist_sq: f64,
    last_seen_tick: f64,
}

/// Interception state of the pre-PR-5197 targeting: `None` means no tile in range.
type LegacyCache = HashMap<UnitId, Option<InterceptionTile>>;

/// Smart SAM targeting system pre-shooting nukes so its range is strictly enforced.
struct SamTargetingSystem {
    precomputed_nukes: HashMap<UnitId, CachedInterception>,
    legacy_nukes: LegacyCache,
    missile_speed: f64,
    sam: UnitId,
}

impl SamTargetingSystem {
    fn new(game: &GameImpl, sam: UnitId) -> Self {
        Self {
            precomputed_nukes: HashMap::new(),
            legacy_nukes: HashMap::new(),
            missile_speed: game.config().default_sam_missile_speed(),
            sam,
        }
    }

    fn on_level_up(&mut self) {
        self.precomputed_nukes.retain(|_, c| c.tick != -1.0);
    }

    fn update_unreachable_nukes(&mut self, current_tick: f64) {
        self.precomputed_nukes
            .retain(|_, c| c.last_seen_tick == current_tick);
    }

    fn tick_to_reach(&self, game: &GameImpl, current_tile: TileRef, tile: TileRef) -> f64 {
        (game.manhattan_dist(current_tile, tile) as f64 / self.missile_speed).ceil()
    }

    fn dynamic_range(&self, game: &GameImpl, tick: f64) -> f64 {
        let sam = game.unit_any(self.sam);
        game.config()
            .dynamic_sam_range(sam.sam_launcher_state(), sam.level() as f64, tick)
    }

    fn check_detonation_interception(
        &self,
        game: &GameImpl,
        unit: UnitId,
        sam_tile: TileRef,
        ticks: f64,
    ) -> Option<InterceptionTile> {
        let state = game.unit_any(unit).nuke_state();
        let trajectory = &state.trajectory;
        let len = trajectory.len() as i64;
        let max_idx = len - 2;
        let final_tile = trajectory.last()?;
        if !final_tile.targetable {
            return None;
        }

        let cur_idx = state.trajectory_index as i64;
        let wait_ticks = state.wait_ticks;
        let exp_ticks = (len - 1 - cur_idx) as f64 + wait_ticks;
        let range = self.dynamic_range(game, ticks + exp_ticks);
        if (game.map.euclidean_dist_squared(sam_tile, final_tile.tile) as f64) > range * range {
            return None;
        }
        let flight_tile = usize::try_from(max_idx)
            .ok()
            .and_then(|i| trajectory.get(i))?;
        if !flight_tile.targetable {
            return None;
        }

        let nuke_ticks = (max_idx - cur_idx) as f64 + wait_ticks;
        let sam_ticks = self.tick_to_reach(game, sam_tile, flight_tile.tile);
        let tick_before_shooting = nuke_ticks - sam_ticks;
        if tick_before_shooting >= 0.0 {
            Some(InterceptionTile {
                tick: tick_before_shooting,
                tile: flight_tile.tile,
            })
        } else {
            None
        }
    }

    fn compute_interception_tile(
        &self,
        game: &GameImpl,
        unit: UnitId,
        sam_tile: TileRef,
        ticks: f64,
    ) -> CachedInterception {
        let state = game.unit_any(unit).nuke_state();
        let trajectory = &state.trajectory;
        let cur_idx = state.trajectory_index as i64;
        let wait_ticks = state.wait_ticks;
        let max_idx = trajectory.len() as i64 - 2;
        let max_sam_range = game.config().max_sam_range();
        let max_sam_range_sq = max_sam_range * max_sam_range;
        let mut min_dist_sq = f64::INFINITY;
        let mut closest_tile = sam_tile;
        let mut inc_steps = 0;
        let mut last_dist_sq = -1.0;

        let mut i = cur_idx;
        while i <= max_idx {
            let tile = trajectory[i as usize];
            let dist_sq = game.map.euclidean_dist_squared(sam_tile, tile.tile) as f64;
            if dist_sq < min_dist_sq {
                min_dist_sq = dist_sq;
                closest_tile = tile.tile;
            }
            inc_steps = if last_dist_sq != -1.0 && dist_sq > last_dist_sq {
                inc_steps + 1
            } else {
                0
            };
            last_dist_sq = dist_sq;

            let nuke_ticks = (i - cur_idx) as f64 + wait_ticks;
            let sam_ticks = self.tick_to_reach(game, sam_tile, tile.tile);
            let allowed = self.dynamic_range(game, ticks + nuke_ticks);
            if tile.targetable && dist_sq <= allowed * allowed && nuke_ticks >= sam_ticks {
                return CachedInterception {
                    tick: nuke_ticks - sam_ticks,
                    tile: tile.tile,
                    min_dist_sq,
                    last_seen_tick: ticks,
                };
            }
            if inc_steps > 3 && dist_sq > max_sam_range_sq {
                break;
            }
            i += 1;
        }

        if let Some(det) = self.check_detonation_interception(game, unit, sam_tile, ticks) {
            return CachedInterception {
                tick: det.tick,
                tile: det.tile,
                min_dist_sq,
                last_seen_tick: ticks,
            };
        }
        CachedInterception {
            tick: if min_dist_sq > max_sam_range_sq {
                -2.0
            } else {
                -1.0
            },
            tile: closest_tile,
            min_dist_sq,
            last_seen_tick: ticks,
        }
    }

    /// Pre-PR-5197 `computeInterceptionTile`.
    fn legacy_interception_tile(
        &self,
        game: &GameImpl,
        unit: UnitId,
        sam_tile: TileRef,
        range_squared: f64,
    ) -> Option<InterceptionTile> {
        let state = game.unit_any(unit).nuke_state();
        let trajectory = &state.trajectory;
        let current_index = state.trajectory_index as i64;
        // The nuke executes before SAM missiles, so the final tick cannot be intercepted.
        let max_interception_index = trajectory.len() as i64 - 2;
        let mut i = current_index;
        while i <= max_interception_index {
            let tile = trajectory[i as usize];
            if tile.targetable
                && (game.map.euclidean_dist_squared(sam_tile, tile.tile) as f64) <= range_squared
            {
                let nuke_tick_to_reach = (i - current_index) as f64;
                let sam_tick_to_reach = self.tick_to_reach(game, sam_tile, tile.tile);
                let tick_before_shooting = nuke_tick_to_reach - sam_tick_to_reach;
                if tick_before_shooting >= 0.0 {
                    return Some(InterceptionTile {
                        tick: tick_before_shooting,
                        tile: tile.tile,
                    });
                }
            }
            i += 1;
        }
        let final_tile = trajectory.last()?;
        if final_tile.targetable
            && (game.map.euclidean_dist_squared(sam_tile, final_tile.tile) as f64) <= range_squared
        {
            let in_flight = usize::try_from(max_interception_index)
                .ok()
                .and_then(|i| trajectory.get(i))?;
            if in_flight.targetable {
                let nuke_tick_to_reach = (max_interception_index - current_index) as f64;
                let sam_tick_to_reach = self.tick_to_reach(game, sam_tile, in_flight.tile);
                let tick_before_shooting = nuke_tick_to_reach - sam_tick_to_reach;
                if tick_before_shooting >= 0.0 {
                    return Some(InterceptionTile {
                        tick: tick_before_shooting,
                        tile: in_flight.tile,
                    });
                }
            }
        }
        None
    }

    /// Pre-PR-5197 `getValidTargets`.
    fn legacy_valid_targets(&mut self, game: &GameImpl, ticks: f64) -> Vec<Target> {
        let sam_tile = game.unit_any(self.sam).tile();
        let range = game
            .config()
            .sam_range(game.unit_any(self.sam).level() as f64);
        let range_squared = range * range;
        let detection_range = game.config().max_sam_range() * 4.0;
        let nukes = game.nearby_units(
            sam_tile,
            detection_range,
            &[
                UnitType::AtomBomb,
                UnitType::HydrogenBomb,
                UnitType::MIRVWarhead,
            ],
            Some(&mut |unit, _| self.is_valid_nuke_target(game, unit)),
            false,
        );
        self.legacy_nukes
            .retain(|id, _| nukes.iter().any(|(n, _)| n == id));

        let mut targets: Vec<Target> = Vec::new();
        for (id, _) in nukes {
            if let Some(cached) = self.legacy_nukes.get(&id) {
                match cached {
                    None => continue,
                    Some(c) if c.tick == ticks || c.tick == ticks + 1.0 => {
                        targets.push(Target {
                            tile: c.tile,
                            unit: id,
                            score: 0.0,
                        });
                        self.legacy_nukes.remove(&id);
                        continue;
                    }
                    Some(c) if c.tick > ticks => continue,
                    Some(_) => {
                        self.legacy_nukes.remove(&id);
                    }
                }
            }
            match self.legacy_interception_tile(game, id, sam_tile, range_squared) {
                Some(hit) if hit.tick <= 1.0 => targets.push(Target {
                    unit: id,
                    tile: hit.tile,
                    score: 0.0,
                }),
                Some(hit) => {
                    self.legacy_nukes.insert(
                        id,
                        Some(InterceptionTile {
                            tick: hit.tick + ticks,
                            tile: hit.tile,
                        }),
                    );
                }
                None => {
                    self.legacy_nukes.insert(id, None);
                }
            }
        }
        self.sort_targets(game, targets)
    }

    /// Pre-PR-4770 `computeInterceptionTile`: the first targetable tile in range the missile can reach before the explosion.
    fn single_interception_tile(
        &self,
        game: &GameImpl,
        unit: UnitId,
        sam_tile: TileRef,
        range_squared: f64,
    ) -> Option<InterceptionTile> {
        let state = game.unit_any(unit).nuke_state();
        let trajectory = &state.trajectory;
        let current_index = state.trajectory_index as i64;
        let explosion_tick = trajectory.len() as f64 - current_index as f64;
        let mut i = current_index;
        while i < trajectory.len() as i64 {
            let tile = trajectory[i as usize];
            if tile.targetable
                && (game.map.euclidean_dist_squared(sam_tile, tile.tile) as f64) <= range_squared
            {
                let nuke_tick_to_reach = (i - current_index) as f64;
                let sam_tick_to_reach = self.tick_to_reach(game, sam_tile, tile.tile);
                let tick_before_shooting = nuke_tick_to_reach - sam_tick_to_reach;
                if sam_tick_to_reach < explosion_tick && tick_before_shooting >= 0.0 {
                    return Some(InterceptionTile {
                        tick: tick_before_shooting,
                        tile: tile.tile,
                    });
                }
            }
            i += 1;
        }
        None
    }

    /// Pre-PR-4770 `getSingleTarget`: at most one target, hydrogen bombs preferred.
    fn get_single_target(&mut self, game: &GameImpl, ticks: f64) -> Option<Target> {
        let sam_tile = game.unit_any(self.sam).tile();
        let range = game
            .config()
            .sam_range(game.unit_any(self.sam).level() as f64);
        let range_squared = range * range;
        let detection_range = game.config().max_sam_range() * 2.0;
        let nukes = game.nearby_units(
            sam_tile,
            detection_range,
            &[UnitType::AtomBomb, UnitType::HydrogenBomb],
            Some(&mut |unit, _| self.is_valid_nuke_target(game, unit)),
            false,
        );
        self.legacy_nukes
            .retain(|id, _| nukes.iter().any(|(n, _)| n == id));

        let mut best: Option<Target> = None;
        let prefer = |best: &Option<Target>, game: &GameImpl, candidate: UnitId| -> bool {
            match best {
                None => true,
                Some(b) => {
                    game.unit_any(candidate).unit_type == UnitType::HydrogenBomb
                        && game.unit_any(b.unit).unit_type != UnitType::HydrogenBomb
                }
            }
        };
        for (id, _) in nukes {
            if let Some(cached) = self.legacy_nukes.get(&id) {
                match cached {
                    None => continue,
                    Some(c) if c.tick == ticks => {
                        let target = Target {
                            tile: c.tile,
                            unit: id,
                            score: 0.0,
                        };
                        if prefer(&best, game, id) {
                            best = Some(target);
                        }
                        self.legacy_nukes.remove(&id);
                        continue;
                    }
                    Some(c) if c.tick > ticks => continue,
                    Some(_) => {
                        self.legacy_nukes.remove(&id);
                    }
                }
            }
            match self.single_interception_tile(game, id, sam_tile, range_squared) {
                Some(hit) if hit.tick <= 1.0 => {
                    if prefer(&best, game, id) {
                        best = Some(Target {
                            unit: id,
                            tile: hit.tile,
                            score: 0.0,
                        });
                    }
                }
                Some(hit) => {
                    self.legacy_nukes.insert(
                        id,
                        Some(InterceptionTile {
                            tick: hit.tick + ticks,
                            tile: hit.tile,
                        }),
                    );
                }
                None => {
                    self.legacy_nukes.insert(id, None);
                }
            }
        }
        best
    }

    fn is_valid_nuke_target(
        &self,
        game: &GameImpl,
        unit: &crate::Game::UnitImpl::UnitImpl,
    ) -> bool {
        let sam_owner = game.unit_any(self.sam).owner_id();
        let skips_targeted = game.has(Change::SamTargetingFix3223);
        if (skips_targeted && unit.targeted_by_sam()) || unit.owner_id() == sam_owner {
            return false;
        }
        let nuke_owner = unit.owner_id();
        if game.player(sam_owner).is_friendly(nuke_owner, false) {
            let game_over = game.has(Change::AftergameTeammateNukes3410)
                && game.winner().is_some()
                && (!game.has(Change::AftergameSingleplayer5332)
                    || game.config().game_config().game_type != GameType::Singleplayer);
            return game_over && game.player(sam_owner).is_on_same_team(nuke_owner);
        }
        true
    }

    fn compute_target_score(&self, game: &GameImpl, target: &Target) -> f64 {
        let sam_tile = game.unit_any(self.sam).tile();
        let unit = game.unit_any(target.unit);
        let state = unit.nuke_state();
        let trajectory = &state.trajectory;
        let current_index = state.trajectory_index;
        let time_to_explode = f64::max(1.0, trajectory.len() as f64 - current_index as f64);

        let target_tile = unit
            .target_tile()
            .unwrap_or_else(|| trajectory.last().map(|t| t.tile).unwrap_or(sam_tile));

        let dist_to_silo = game.manhattan_dist(sam_tile, target_tile) as f64;
        let type_bonus = if unit.unit_type == UnitType::HydrogenBomb {
            70_001.0
        } else {
            0.0
        };
        let distance_bonus = f64::max(0.0, 200_000.0 - dist_to_silo * 1000.0);
        let urgency_bonus = f64::max(0.0, 10_000.0 - time_to_explode * 100.0);
        type_bonus + distance_bonus + urgency_bonus
    }

    fn sort_targets(&self, game: &GameImpl, mut targets: Vec<Target>) -> Vec<Target> {
        if targets.len() <= 1 {
            return targets;
        }
        for target in targets.iter_mut() {
            target.score = self.compute_target_score(game, target);
        }
        targets.sort_by(|a, b| b.score.total_cmp(&a.score));
        targets
    }

    fn get_valid_targets(&mut self, game: &GameImpl, ticks: f64) -> Vec<Target> {
        let sam_tile = game.unit_any(self.sam).tile();
        let detection_range = game.config().max_sam_range() * 4.0;
        let nukes = game.nearby_units(
            sam_tile,
            detection_range,
            &[
                UnitType::AtomBomb,
                UnitType::HydrogenBomb,
                UnitType::MIRVWarhead,
            ],
            Some(&mut |unit, _| self.is_valid_nuke_target(game, unit)),
            false,
        );

        let mut targets: Vec<Target> = Vec::new();
        for (id, _) in nukes {
            if let Some(cached) = self.precomputed_nukes.get_mut(&id) {
                cached.last_seen_tick = ticks;

                if cached.tick == -2.0 || cached.tick == -1.0 {
                    continue;
                }

                if cached.tick == ticks || cached.tick == ticks + 1.0 {
                    targets.push(Target {
                        tile: cached.tile,
                        unit: id,
                        score: 0.0,
                    });
                    self.precomputed_nukes.remove(&id);
                    continue;
                }

                if cached.tick > ticks {
                    continue;
                }

                self.precomputed_nukes.remove(&id);
            }

            let res = self.compute_interception_tile(game, id, sam_tile, ticks);
            if res.tick >= 0.0 && res.tick <= 1.0 {
                targets.push(Target {
                    unit: id,
                    tile: res.tile,
                    score: 0.0,
                });
            } else {
                self.precomputed_nukes.insert(
                    id,
                    CachedInterception {
                        tick: if res.tick >= 0.0 {
                            res.tick + ticks
                        } else {
                            res.tick
                        },
                        tile: res.tile,
                        min_dist_sq: res.min_dist_sq,
                        last_seen_tick: ticks,
                    },
                );
            }
        }
        self.update_unreachable_nukes(ticks);
        self.sort_targets(game, targets)
    }
}

/// A SAM launcher: builds itself (when not given a unit), reloads and shoots nukes down.
pub struct SAMLauncherExecution {
    active: bool,
    targeting_system: Option<SamTargetingSystem>,
    last_level: u32,
    player: SmallId,
    tile: Option<TileRef>,
    sam: Option<UnitId>,
    /// Hit chance draws of this launcher (before PR 2665), seeded by the unit id.
    pseudo_random: Option<crate::Core::PseudoRandom::PseudoRandom>,
}

impl SAMLauncherExecution {
    /// `new SAMLauncherExecution(player, tile, sam = null)`.
    pub fn new(player: SmallId, tile: Option<TileRef>, sam: Option<UnitId>) -> Self {
        Self {
            active: true,
            targeting_system: None,
            last_level: 1,
            player,
            tile,
            sam,
            pseudo_random: None,
        }
    }
}

impl SAMLauncherExecution {
    /// Pre-PR-4770 targeting step: MIRV warheads aimed at the SAM are removed, otherwise one nuke is shot.
    fn tick_single_target(&mut self, game: &mut GameImpl, ticks: Tick, sam: UnitId) {
        const MIRV_WARHEAD_SEARCH_RADIUS: f64 = 400.0;
        const MIRV_WARHEAD_PROTECTION_RADIUS: u32 = 50;
        let sam_tile = game.unit_any(sam).tile();
        let player = self.player;
        let warheads: Vec<UnitId> = game
            .nearby_units(
                sam_tile,
                MIRV_WARHEAD_SEARCH_RADIUS,
                &[UnitType::MIRVWarhead],
                Some(&mut |unit, _| {
                    let nuke_owner = unit.owner_id();
                    if nuke_owner == player {
                        return false;
                    }
                    if game.player(player).is_friendly(nuke_owner, false)
                        && (game.winner().is_none()
                            || !game.player(player).is_on_same_team(nuke_owner)
                            || !game.has(Change::AftergameTeammateNukes3410))
                    {
                        return false;
                    }
                    unit.target_tile().is_some_and(|dst| {
                        game.manhattan_dist(dst, sam_tile) < MIRV_WARHEAD_PROTECTION_RADIUS
                    })
                }),
                false,
            )
            .into_iter()
            .map(|(id, _)| id)
            .collect();

        let target = if warheads.is_empty() {
            self.targeting_system
                .as_mut()
                .expect("set before")
                .get_single_target(game, ticks as f64)
        } else {
            None
        };
        if target.is_none() && warheads.is_empty() {
            return;
        }
        // Before PR 3223 the best target may already be flagged, and then nothing is shot.
        let target_flagged = target
            .as_ref()
            .is_some_and(|t| game.unit_any(t.unit).targeted_by_sam());
        if warheads.is_empty() && target_flagged {
            return;
        }
        game.unit_mut(sam).launch();
        if !game.has(Change::SamNoRng2665) {
            // Before PR 2665 every shot draws against the hit chance (nukes: always, warheads 0.5, others 0.8).
            let hit_type = if warheads.is_empty() {
                target.as_ref().map(|t| game.unit_any(t.unit).unit_type())
            } else {
                Some(UnitType::MIRVWarhead)
            };
            let rules = game.config().rules().clone();
            let random = self
                .pseudo_random
                .get_or_insert_with(|| {
                    crate::Core::PseudoRandom::PseudoRandom::for_rules(&rules, sam as f64)
                })
                .next();
            let hit = match hit_type {
                Some(UnitType::AtomBomb) => true,
                Some(UnitType::MIRVWarhead) => random < 0.5,
                _ => random < 0.8,
            };
            if !hit {
                return;
            }
        }
        if !warheads.is_empty() {
            for warhead in warheads {
                game.unit_mut(warhead).delete(None, None);
            }
        } else if let Some(target) = target {
            game.unit_mut(target.unit).set_targeted_by_sam(true);
            let (sam_tile, sam_owner) = {
                let u = game.unit_any(sam);
                (u.tile(), u.owner_id())
            };
            game.add_execution(Box::new(SAMMissileExecution::new(
                sam_tile,
                sam_owner,
                sam,
                target.unit,
                target.tile,
            )));
        }
    }
}

impl Execution for SAMLauncherExecution {
    fn init(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        let sam = match self.sam {
            Some(sam) => sam,
            None => {
                let tile = self.tile.expect("tile is null");
                let Some(spawn_tile) =
                    game.player(self.player)
                        .can_build(UnitType::SAMLauncher, tile, None)
                else {
                    self.active = false;
                    return;
                };
                let sam = game.player_mut(self.player).build_unit(
                    UnitType::SAMLauncher,
                    spawn_tile,
                    UnitParams::default(),
                );
                self.sam = Some(sam);
                sam
            }
        };
        if self.targeting_system.is_none() {
            self.targeting_system = Some(SamTargetingSystem::new(game, sam));
        }

        let modern = game.has(Change::SamTargeting5197);
        let level = game.unit_any(sam).level();
        if modern && self.last_level != level {
            self.last_level = level;
            self.targeting_system
                .as_mut()
                .expect("set above")
                .on_level_up();
        }

        if game.unit_any(sam).is_under_construction() {
            return;
        }

        if !game.unit_any(sam).is_active() {
            self.active = false;
            return;
        }

        let owner = game.unit_any(sam).owner_id();
        if self.player != owner {
            self.player = owner;
        }

        let was_in_cooldown = game.unit_any(sam).is_in_cooldown();
        let reload_first = game.has(Change::SamReloadThenFire31);
        // Before v0.31 missiles only reload while every slot is in cooldown.
        while (reload_first || was_in_cooldown)
            && let Some(&front_time) = game.unit_any(sam).missile_timer_queue().first()
        {
            let cooldown = game.config().sam_cooldown() as f64 - (game.ticks() as f64 - front_time);
            if cooldown > 0.0 {
                break;
            }
            game.unit_mut(sam).reload_missile();
            if !modern {
                break;
            }
        }
        if game.unit_any(sam).is_in_cooldown()
            || (was_in_cooldown && !game.has(Change::SamReloadThenFire31))
        {
            return;
        }

        if !game.has(Change::SamTargetingFixes4770) {
            self.tick_single_target(game, ticks, sam);
            return;
        }

        let targeting = self.targeting_system.as_mut().expect("set above");
        let targets = if modern {
            if game.unit_count(UnitType::AtomBomb) == 0
                && game.unit_count(UnitType::HydrogenBomb) == 0
                && game.unit_count(UnitType::MIRVWarhead) == 0
            {
                return;
            }
            targeting.get_valid_targets(game, ticks as f64)
        } else {
            targeting.legacy_valid_targets(game, ticks as f64)
        };
        for target in targets {
            if game.unit_any(sam).is_in_cooldown() {
                break;
            }
            game.unit_mut(sam).launch();
            game.unit_mut(target.unit).set_targeted_by_sam(true);
            let (sam_tile, sam_owner) = {
                let u = game.unit_any(sam);
                (u.tile(), u.owner_id())
            };
            game.add_execution(Box::new(SAMMissileExecution::new(
                sam_tile,
                sam_owner,
                sam,
                target.unit,
                target.tile,
            )));
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
