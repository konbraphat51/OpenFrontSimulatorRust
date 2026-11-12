// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/UnitImpl.ts: unit state plus the mutating methods that touch the game.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `toUpdate`, `touch`, `displayMessageOnDeleted` and every `stats()` call (record-keeping only)
//! are client plumbing and skipped; `addUpdate` calls are dropped. Units live in
//! `GameImpl.units` (index `id - 1`) and stay there after deletion so executions holding an id can
//! still ask `is_active`; `GameImpl::unit(id)` mirrors upstream's `game.unit(id)` and returns
//! `None` once the unit left the game's unit map.

use super::Game::{
    NukeState, SamLauncherState, SmallId, TrainType, TransportShipState, UnitId, UnitInfo,
    UnitParams, UnitType, WarshipMode, WarshipState,
};
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use super::Veterancy::max_health_with_veterancy;
use crate::Builds::Rules::Change;
use crate::Core::Util::{simple_hash, to_int};
use std::ops::{Deref, DerefMut};

/// State of one unit.
#[derive(Clone, Debug)]
pub struct UnitImpl {
    pub(crate) unit_type: UnitType,
    pub(crate) id: UnitId,
    pub(crate) tile: TileRef,
    pub(crate) last_tile: TileRef,
    pub(crate) owner: SmallId,
    pub(crate) last_owner: Option<SmallId>,
    pub(crate) active: bool,
    /// True while the unit is in the game's unit map (upstream `_unitMap`).
    pub(crate) in_unit_map: bool,
    pub(crate) target_tile: Option<TileRef>,
    pub(crate) target_player: Option<SmallId>,
    pub(crate) target_unit: Option<UnitId>,
    pub(crate) health: i64,
    pub(crate) transport_ship_state: Option<TransportShipState>,
    pub(crate) warship_state: Option<WarshipState>,
    pub(crate) nuke_state: Option<NukeState>,
    pub(crate) reached_target: bool,
    pub(crate) was_destroyed_by_enemy: bool,
    pub(crate) destroyer: Option<SmallId>,
    pub(crate) last_set_safe_from_pirates: f64,
    pub(crate) under_construction: bool,
    pub(crate) troops: f64,
    /// Ticks of missiles in cooldown, oldest first.
    pub(crate) missile_timer_queue: Vec<f64>,
    pub(crate) has_train_station: bool,
    pub(crate) level: u32,
    pub(crate) targetable: bool,
    pub(crate) loaded: Option<bool>,
    pub(crate) train_type: Option<TrainType>,
    pub(crate) deletion_at: Option<f64>,
    pub(crate) sam_launcher_state: Option<SamLauncherState>,
}

impl UnitImpl {
    /// `new UnitImpl(type, mg, tile, id, owner, params)`.
    pub(crate) fn new(
        game: &GameImpl,
        unit_type: UnitType,
        tile: TileRef,
        id: UnitId,
        owner: SmallId,
        params: UnitParams,
    ) -> Self {
        let config = game.config();
        let info = config.unit_info(unit_type);
        let nuke_state = if params.trajectory.is_some() || params.has_wait_ticks {
            Some(NukeState {
                trajectory: params.trajectory.unwrap_or_default(),
                wait_ticks: 0.0,
                trajectory_index: 0,
                targeted_by_sam: false,
            })
        } else {
            None
        };
        Self {
            unit_type,
            id,
            tile,
            last_tile: tile,
            owner,
            last_owner: None,
            active: true,
            in_unit_map: false,
            target_tile: params.target_tile,
            target_player: params.target_player,
            target_unit: params.target_unit,
            health: to_int(info.max_health.unwrap_or(1.0)),
            transport_ship_state: (unit_type == UnitType::TransportShip).then_some(
                TransportShipState {
                    is_retreating: false,
                },
            ),
            warship_state: params.patrol_tile.map(|p| WarshipState {
                state: WarshipMode::Patrolling,
                patrol_tile: Some(p),
                retreat_port: None,
                last_combat_tick: -100.0,
                veterancy: 0.0,
                veterancy_progress: 0.0,
            }),
            nuke_state,
            reached_target: false,
            was_destroyed_by_enemy: false,
            destroyer: None,
            last_set_safe_from_pirates: params.last_set_safe_from_pirates.unwrap_or(0.0),
            under_construction: false,
            troops: params.troops.unwrap_or(0.0),
            missile_timer_queue: Vec::new(),
            has_train_station: false,
            level: 1,
            targetable: true,
            loaded: params.loaded,
            train_type: params.train_type,
            deletion_at: None,
            sam_launcher_state: (unit_type == UnitType::SAMLauncher).then(|| SamLauncherState {
                upgrade_start_tick: None,
                start_range: config.sam_range(1.0),
                target_level: 1.0,
                duration: config.sam_upgrade_duration(),
            }),
        }
    }

    /// `id`.
    pub fn id(&self) -> UnitId {
        self.id
    }
    /// `type`.
    pub fn unit_type(&self) -> UnitType {
        self.unit_type
    }
    /// `tile`.
    pub fn tile(&self) -> TileRef {
        self.tile
    }
    /// `lastTile`.
    pub fn last_tile(&self) -> TileRef {
        self.last_tile
    }
    /// `owner()` as a small id.
    pub fn owner_id(&self) -> SmallId {
        self.owner
    }
    /// `isActive`.
    pub fn is_active(&self) -> bool {
        self.active
    }
    /// `isTargetable`.
    pub fn is_targetable(&self) -> bool {
        self.targetable
    }
    /// `troops`.
    pub fn troops(&self) -> f64 {
        self.troops
    }
    /// `health`.
    pub fn health(&self) -> f64 {
        self.health as f64
    }
    /// `isUnderConstruction`.
    pub fn is_under_construction(&self) -> bool {
        self.under_construction
    }
    /// `isMarkedForDeletion`.
    pub fn is_marked_for_deletion(&self) -> bool {
        self.deletion_at.is_some()
    }
    /// `wasDestroyedByEnemy`.
    pub fn was_destroyed_by_enemy(&self) -> bool {
        self.was_destroyed_by_enemy
    }
    /// `destroyer`.
    pub fn destroyer(&self) -> Option<SmallId> {
        self.destroyer
    }
    /// `lastOwner` as a small id.
    pub fn last_owner(&self) -> Option<SmallId> {
        self.last_owner
    }
    /// `level`.
    pub fn level(&self) -> u32 {
        self.level
    }
    /// `reachedTarget`.
    pub fn reached_target(&self) -> bool {
        self.reached_target
    }
    /// `targetTile` / `tileTarget`.
    pub fn target_tile(&self) -> Option<TileRef> {
        self.target_tile
    }
    /// `targetPlayer`.
    pub fn target_player(&self) -> Option<SmallId> {
        self.target_player
    }
    /// `targetUnit`.
    pub fn target_unit(&self) -> Option<UnitId> {
        self.target_unit
    }
    /// `hasTrainStation`.
    pub fn has_train_station(&self) -> bool {
        self.has_train_station
    }
    /// `trainType`.
    pub fn train_type(&self) -> Option<TrainType> {
        self.train_type
    }
    /// `isLoaded`.
    pub fn is_loaded(&self) -> Option<bool> {
        self.loaded
    }
    /// `missileTimerQueue`.
    pub fn missile_timer_queue(&self) -> &[f64] {
        &self.missile_timer_queue
    }
    /// `ticksLeftInCooldown`: the oldest launch tick.
    pub fn ticks_left_in_cooldown(&self) -> Option<f64> {
        self.missile_timer_queue.first().copied()
    }
    /// `isInCooldown`.
    pub fn is_in_cooldown(&self) -> bool {
        self.missile_timer_queue.len() == self.level as usize
    }
    /// `samLauncherState`.
    pub fn sam_launcher_state(&self) -> Option<&SamLauncherState> {
        self.sam_launcher_state.as_ref()
    }
    /// `veterancy`: 0 for non-warships.
    pub fn veterancy(&self) -> f64 {
        self.warship_state.map_or(0.0, |s| s.veterancy)
    }
    /// `transportShipState` (panics on other units like upstream throws).
    pub fn transport_ship_state(&self) -> TransportShipState {
        self.transport_ship_state
            .expect("transportShipState called on non-transport-ship unit")
    }
    /// `nukeState` (panics on other units).
    pub fn nuke_state(&self) -> &NukeState {
        self.nuke_state
            .as_ref()
            .expect("nukeState called on non-nuke unit")
    }
    /// The raw warship state (`warshipState()` minus the derived `isInCombat`); panics on other units.
    pub fn warship_state(&self) -> &WarshipState {
        self.warship_state
            .as_ref()
            .expect("warshipState called on non-warship unit")
    }
    /// `isInCombat` (needs the current tick).
    pub fn is_in_combat(&self, ticks: u32) -> bool {
        ticks as f64 - self.warship_state().last_combat_tick <= 3.0
    }
    /// `trajectoryIndex`.
    pub fn trajectory_index(&self) -> usize {
        self.nuke_state.as_ref().map_or(0, |s| s.trajectory_index)
    }
    /// `targetedBySAM`.
    pub fn targeted_by_sam(&self) -> bool {
        self.nuke_state
            .as_ref()
            .expect("targetedBySAM on non-nuke unit")
            .targeted_by_sam
    }
    /// `hash`: `tile + simpleHash(type) * id`.
    pub fn hash(&self) -> f64 {
        self.tile as f64 + simple_hash(self.unit_type.as_str()) as f64 * self.id as f64
    }
    /// `isOverdueDeletion` (needs the current tick).
    pub fn is_overdue_deletion(&self, ticks: u32) -> bool {
        self.active && self.deletion_at.is_some_and(|d| ticks as f64 - d > 0.0)
    }
}

/// Read handle on a unit with the methods that need the game.
pub struct UnitRef<'a> {
    pub(crate) game: &'a GameImpl,
    pub(crate) id: UnitId,
}

impl Deref for UnitRef<'_> {
    type Target = UnitImpl;
    fn deref(&self) -> &UnitImpl {
        self.game.unit_any(self.id)
    }
}

impl UnitRef<'_> {
    /// `info()`.
    pub fn info(&self) -> UnitInfo {
        self.game.config().unit_info(self.unit_type)
    }
    /// `hasHealth`.
    pub fn has_health(&self) -> bool {
        self.info().max_health.is_some()
    }
    /// `maxHealth`: base plus veterancy bonus.
    pub fn max_health(&self) -> f64 {
        let base = self.info().max_health.unwrap_or(1.0);
        max_health_with_veterancy(
            base,
            self.veterancy(),
            self.game.config().warship_veterancy_health_bonus(),
        )
    }
    /// `isSafeFromPirates`.
    pub fn is_safe_from_pirates(&self) -> bool {
        self.game.ticks() as f64 - self.last_set_safe_from_pirates
            < self.game.config().safe_from_pirates_cooldown_max()
    }
    /// `isInCombat` at the game's current tick.
    pub fn in_combat(&self) -> bool {
        self.is_in_combat(self.game.ticks())
    }
}

/// Mutating handle on a unit.
pub struct UnitMut<'a> {
    pub(crate) game: &'a mut GameImpl,
    pub(crate) id: UnitId,
}

impl Deref for UnitMut<'_> {
    type Target = UnitImpl;
    fn deref(&self) -> &UnitImpl {
        self.game.unit_any(self.id)
    }
}

impl DerefMut for UnitMut<'_> {
    fn deref_mut(&mut self) -> &mut UnitImpl {
        self.game.unit_any_mut(self.id)
    }
}

impl UnitMut<'_> {
    /// Read view of the same unit.
    pub fn as_ref(&self) -> UnitRef<'_> {
        UnitRef {
            game: self.game,
            id: self.id,
        }
    }

    fn u(&mut self) -> &mut UnitImpl {
        self.game.unit_any_mut(self.id)
    }

    /// `setTargetable`.
    pub fn set_targetable(&mut self, targetable: bool) {
        self.u().targetable = targetable;
    }

    /// `setTileTarget` / `setTargetTile`.
    pub fn set_target_tile(&mut self, tile: Option<TileRef>) {
        self.u().target_tile = tile;
    }

    /// `move`: relocates the unit and refreshes the spatial grid.
    pub fn move_to(&mut self, tile: TileRef) {
        let u = self.u();
        u.last_tile = u.tile;
        u.tile = tile;
        self.game.update_unit_tile(self.id);
    }

    /// `setTroops`: clamped at 0.
    pub fn set_troops(&mut self, troops: f64) {
        let next = if troops > 0.0 || !self.game.has(Change::NukeExplosionBulk3176) {
            troops
        } else {
            0.0
        };
        self.u().troops = next;
    }

    /// `setOwner`: transfers the unit to `new_owner`, appending it to the new owner's unit list.
    pub fn set_owner(&mut self, new_owner: SmallId) {
        let id = self.id;
        self.clear_pending_deletion();
        let old = self.owner;
        self.u().last_owner = Some(old);
        let old_units = &mut self.game.player_impl_mut(old).units;
        old_units.retain(|&u| u != id);
        self.game.player_impl_mut(old).my_units_version += 1;
        self.u().owner = new_owner;
        let np = self.game.player_impl_mut(new_owner);
        np.units.push(id);
        np.my_units_version += 1;
        self.game.bump_units_version();
    }

    /// `modifyHealth`: clamps into [0, maxHealth] and deletes the unit at 0.
    pub fn modify_health(&mut self, delta: f64, attacker: Option<SmallId>) {
        let previous = self.health;
        let max = to_int(self.as_ref().max_health());
        let next = (previous + to_int(delta)).clamp(0, max);
        if next == previous {
            return;
        }
        let ticks = self.game.ticks() as f64;
        if attacker.is_some()
            && delta < 0.0
            && let Some(ws) = self.u().warship_state.as_mut()
        {
            ws.last_combat_tick = ticks;
        }
        self.u().health = next;
        if next == 0 {
            self.delete(Some(true), attacker);
        }
    }

    /// `clearPendingDeletion`.
    pub fn clear_pending_deletion(&mut self) {
        self.u().deletion_at = None;
    }

    /// `markForDeletion`.
    pub fn mark_for_deletion(&mut self) {
        if !self.is_active() {
            return;
        }
        let at = self.game.ticks() as f64 + self.game.config().deletion_mark_duration() as f64;
        self.u().deletion_at = Some(at);
    }

    /// `delete(displayMessage, destroyer)`: `Some(false)` retires a unit that was not destroyed.
    pub fn delete(&mut self, _display_message: Option<bool>, destroyer: Option<SmallId>) {
        assert!(
            self.is_active(),
            "cannot delete {:?} not active",
            self.unit_type
        );
        let id = self.id;
        let owner = self.owner;
        let u = self.u();
        u.was_destroyed_by_enemy = destroyer.is_some();
        u.destroyer = destroyer;
        let p = self.game.player_impl_mut(owner);
        p.units.retain(|&x| x != id);
        p.my_units_version += 1;
        self.u().active = false;
        self.game.remove_unit(id);
    }

    /// `updateWarshipState`: merges the given fields; `isInCombat` is not stored.
    pub fn update_warship_state(
        &mut self,
        is_in_combat: bool,
        state: Option<WarshipMode>,
        patrol_tile: Option<Option<TileRef>>,
        retreat_port: Option<Option<TileRef>>,
    ) {
        let ticks = self.game.ticks() as f64;
        let ws = self
            .u()
            .warship_state
            .as_mut()
            .expect("updateWarshipState called on non-warship unit");
        if is_in_combat {
            ws.last_combat_tick = ticks;
        }
        let merged_state = state.unwrap_or(ws.state);
        let merged_patrol = patrol_tile.unwrap_or(ws.patrol_tile);
        let merged_retreat = retreat_port.unwrap_or(ws.retreat_port);
        if merged_state == ws.state
            && merged_patrol == ws.patrol_tile
            && merged_retreat == ws.retreat_port
        {
            return;
        }
        ws.state = merged_state;
        ws.patrol_tile = merged_patrol;
        ws.retreat_port = merged_retreat;
    }

    /// `updateTransportShipState({ isRetreating })`.
    pub fn update_transport_ship_state(&mut self, is_retreating: bool) {
        let s = self
            .u()
            .transport_ship_state
            .as_mut()
            .expect("updateTransportShipState called on non-transport-ship unit");
        s.is_retreating = is_retreating;
    }

    /// `updateNukeState`: merges the three mutable fields.
    pub fn update_nuke_state(
        &mut self,
        targeted_by_sam: Option<bool>,
        trajectory_index: Option<usize>,
        wait_ticks: Option<f64>,
    ) {
        let s = self
            .u()
            .nuke_state
            .as_mut()
            .expect("updateNukeState called on non-nuke unit");
        if let Some(v) = targeted_by_sam {
            s.targeted_by_sam = v;
        }
        if let Some(v) = trajectory_index {
            s.trajectory_index = v;
        }
        if let Some(v) = wait_ticks {
            s.wait_ticks = v;
        }
    }

    /// `setUnderConstruction`.
    pub fn set_under_construction(&mut self, under_construction: bool) {
        if self.under_construction != under_construction {
            self.u().under_construction = under_construction;
            let owner = self.owner;
            self.game.player_impl_mut(owner).my_units_version += 1;
        }
    }

    /// `launch`: starts a missile cooldown.
    pub fn launch(&mut self) {
        let t = self.game.ticks() as f64;
        self.u().missile_timer_queue.push(t);
    }

    /// `reloadMissile`.
    pub fn reload_missile(&mut self) {
        let q = &mut self.u().missile_timer_queue;
        if !q.is_empty() {
            q.remove(0);
        }
    }

    /// `setTrajectoryIndex`: clamped into the trajectory.
    pub fn set_trajectory_index(&mut self, i: i64) {
        let s = self
            .u()
            .nuke_state
            .as_mut()
            .expect("setTrajectoryIndex called on non-nuke unit");
        let max = s.trajectory.len() as i64 - 1;
        s.trajectory_index = if i < 0 {
            0
        } else if i > max {
            max.max(0) as usize
        } else {
            i as usize
        };
    }

    /// `setTargetUnit`.
    pub fn set_target_unit(&mut self, target: Option<UnitId>) {
        self.u().target_unit = target;
    }

    /// `setTargetedBySAM`.
    pub fn set_targeted_by_sam(&mut self, targeted: bool) {
        self.u()
            .nuke_state
            .as_mut()
            .expect("setTargetedBySAM on non-nuke unit")
            .targeted_by_sam = targeted;
    }

    /// `setReachedTarget`.
    pub fn set_reached_target(&mut self) {
        self.u().reached_target = true;
    }

    /// `setSafeFromPirates`.
    pub fn set_safe_from_pirates(&mut self) {
        let t = self.game.ticks() as f64;
        self.u().last_set_safe_from_pirates = t;
    }

    fn increase_veterancy(&mut self) {
        let max = self.game.config().warship_max_veterancy();
        let Some(ws) = self.u().warship_state.as_mut() else {
            return;
        };
        if ws.veterancy >= max {
            return;
        }
        ws.veterancy += 1.0;
    }

    /// `recordKill`.
    pub fn record_kill(&mut self, target_type: UnitType) {
        if self.warship_state.is_none() {
            return;
        }
        if target_type == UnitType::Warship {
            self.u().warship_state.as_mut().unwrap().veterancy_progress = 0.0;
            self.increase_veterancy();
        } else if target_type == UnitType::TransportShip {
            self.add_veterancy_progress(UnitType::TransportShip);
        }
    }

    /// `recordTradeCapture`.
    pub fn record_trade_capture(&mut self) {
        if self.warship_state.is_none() {
            return;
        }
        self.add_veterancy_progress(UnitType::TradeShip);
    }

    fn add_veterancy_progress(&mut self, source: UnitType) {
        let config = self.game.config();
        let max_veterancy = config.warship_max_veterancy();
        let transport_threshold = config.warship_veterancy_transport_kills();
        let capture_threshold = config.warship_veterancy_trade_captures();
        let Some(ws) = self.u().warship_state.as_mut() else {
            return;
        };
        if ws.veterancy >= max_veterancy {
            return;
        }
        let points_per_level = transport_threshold * capture_threshold;
        ws.veterancy_progress += if source == UnitType::TransportShip {
            capture_threshold
        } else {
            transport_threshold
        };
        while ws.veterancy_progress >= points_per_level && ws.veterancy < max_veterancy {
            ws.veterancy_progress -= points_per_level;
            ws.veterancy += 1.0;
        }
        if ws.veterancy >= max_veterancy {
            ws.veterancy_progress = 0.0;
        }
    }

    /// `setTrainStation`.
    pub fn set_train_station(&mut self, train_station: bool) {
        self.u().has_train_station = train_station;
    }

    /// `increaseLevel`.
    pub fn increase_level(&mut self) {
        let tick = self.game.ticks() as f64;
        if self.unit_type == UnitType::SAMLauncher {
            let state = self.sam_launcher_state;
            let range =
                self.game
                    .config()
                    .dynamic_sam_range(state.as_ref(), self.level as f64, tick);
            let duration = self.game.config().sam_upgrade_duration();
            let target_level = self.level as f64 + 1.0;
            self.u().sam_launcher_state = Some(SamLauncherState {
                upgrade_start_tick: Some(tick),
                start_range: range,
                target_level,
                duration,
            });
        }
        self.u().level += 1;
        self.game.bump_units_version();
        let owner = self.owner;
        self.game.player_impl_mut(owner).my_units_version += 1;
        if matches!(
            self.unit_type,
            UnitType::MissileSilo | UnitType::SAMLauncher
        ) {
            self.u().missile_timer_queue.push(tick);
        }
    }

    /// `decreaseLevel`.
    pub fn decrease_level(&mut self, destroyer: Option<SmallId>) {
        self.u().level -= 1;
        if matches!(
            self.unit_type,
            UnitType::MissileSilo | UnitType::SAMLauncher
        ) {
            self.u().missile_timer_queue.pop();
        }
        if self.unit_type == UnitType::SAMLauncher {
            self.u().sam_launcher_state = None;
        }
        if self.level == 0 {
            self.delete(Some(true), destroyer);
            return;
        }
        self.game.bump_units_version();
        let owner = self.owner;
        self.game.player_impl_mut(owner).my_units_version += 1;
    }

    /// `setLoaded`.
    pub fn set_loaded(&mut self, loaded: bool) {
        self.u().loaded = Some(loaded);
    }
}
