// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/AttackImpl.ts: one ongoing attack and its border bookkeeping.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! `clusteredPositions` only feeds the client's attack labels and is not ported. Attacks live in
//! `GameImpl.attacks` (never freed, so executions may keep an `AttackId`); `delete` also removes
//! the attack from both players' ordered lists.

use super::Game::{AttackId, SmallId};
use super::GameImpl::GameImpl;
use super::GameMap::TileRef;
use crate::Compat::OrderedSet::OrderedSet;

/// State of an attack.
#[derive(Clone, Debug)]
pub struct AttackImpl {
    pub(crate) id: String,
    pub(crate) target: SmallId,
    pub(crate) attacker: SmallId,
    pub(crate) troops: f64,
    pub(crate) source_tile: Option<TileRef>,
    pub(crate) border: OrderedSet<TileRef>,
    pub(crate) is_active: bool,
    pub(crate) border_size: f64,
    pub(crate) retreating: bool,
    pub(crate) retreated: bool,
}

impl AttackImpl {
    /// `new AttackImpl(...)`; `border` is the caller's initial border set (its size is NOT counted
    /// in `border_size`, exactly like upstream's `_borderSize = 0`).
    pub fn new(
        id: String,
        target: SmallId,
        attacker: SmallId,
        troops: f64,
        source_tile: Option<TileRef>,
        border: OrderedSet<TileRef>,
    ) -> Self {
        Self {
            id,
            target,
            attacker,
            troops,
            source_tile,
            border,
            is_active: true,
            border_size: 0.0,
            retreating: false,
            retreated: false,
        }
    }

    /// `id`.
    pub fn id(&self) -> &str {
        &self.id
    }
    /// `sourceTile`.
    pub fn source_tile(&self) -> Option<TileRef> {
        self.source_tile
    }
    /// `target` (0 = terra nullius).
    pub fn target(&self) -> SmallId {
        self.target
    }
    /// `attacker`.
    pub fn attacker(&self) -> SmallId {
        self.attacker
    }
    /// `troops`.
    pub fn troops(&self) -> f64 {
        self.troops
    }
    /// `setTroops`: clamped at 0.
    pub fn set_troops(&mut self, troops: f64) {
        self.troops = if troops > 0.0 { troops } else { 0.0 };
    }
    /// `setTroops` without the clamp (builds before PR 3176).
    pub fn set_troops_unclamped(&mut self, troops: f64) {
        self.troops = troops;
    }
    /// `isActive`.
    pub fn is_active(&self) -> bool {
        self.is_active
    }
    /// `orderRetreat`.
    pub fn order_retreat(&mut self) {
        self.retreating = true;
    }
    /// `executeRetreat`.
    pub fn execute_retreat(&mut self) {
        self.retreated = true;
    }
    /// `retreating`.
    pub fn retreating(&self) -> bool {
        self.retreating
    }
    /// `retreated`.
    pub fn retreated(&self) -> bool {
        self.retreated
    }
    /// `borderSize`.
    pub fn border_size(&self) -> f64 {
        self.border_size
    }
    /// `clearBorder`.
    pub fn clear_border(&mut self) {
        self.border_size = 0.0;
        self.border.clear();
    }
    /// `addBorderTile`.
    pub fn add_border_tile(&mut self, tile: TileRef) {
        if self.border.insert(tile) {
            self.border_size += 1.0;
        }
    }
    /// `removeBorderTile`.
    pub fn remove_border_tile(&mut self, tile: TileRef) {
        if self.border.remove(&tile) {
            self.border_size -= 1.0;
        }
    }
    /// The border tiles in insertion order.
    pub fn border(&self) -> &OrderedSet<TileRef> {
        &self.border
    }
}

impl GameImpl {
    /// `AttackImpl.delete`: detaches the attack from both players and deactivates it.
    pub fn delete_attack(&mut self, id: AttackId) {
        let (target, attacker) = {
            let a = &self.attacks[id as usize];
            (a.target, a.attacker)
        };
        if target != 0 {
            self.player_impl_mut(target)
                .incoming_attacks
                .retain(|&a| a != id);
        }
        self.player_impl_mut(attacker)
            .outgoing_attacks
            .retain(|&a| a != id);
        let attack = &mut self.attacks[id as usize];
        attack.is_active = false;
        // The handle stays valid, but nothing reads the border of a deleted attack: free its tile set.
        attack.border = OrderedSet::new();
    }
}
