// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: strikes): strike and follow-up planning.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    /// A player not much weaker than us: Medium one it is attacking, Hard and Impossible the strongest neighbour they can follow up on.
    pub(super) fn find_strike_target(&self, game: &GameImpl) -> Option<SmallId> {
        let my_troops = game.player(self.player).troops();
        let is_candidate = |other: SmallId| {
            other != 0
                && game.player(other).player_type() != PlayerType::Bot
                && !game.player(self.player).is_friendly(other, false)
                && game.player(other).troops() >= my_troops * STRIKE_MIN_TROOP_RATIO
        };
        match Self::difficulty(game) {
            Difficulty::Easy => None,
            Difficulty::Medium => {
                for &attack in game.player(self.player).outgoing_attacks() {
                    let target = game.attack(attack).target();
                    if is_candidate(target) {
                        return Some(target);
                    }
                }
                None
            }
            Difficulty::Hard | Difficulty::Impossible => {
                let mut best: Option<SmallId> = None;
                for other in game.player(self.player).nearby() {
                    if !is_candidate(other) {
                        continue;
                    }
                    if best.is_some_and(|b| game.player(other).troops() <= game.player(b).troops())
                    {
                        continue;
                    }
                    if self.can_strike(game, other) {
                        best = Some(other);
                    }
                }
                best
            }
        }
    }

    /// Whether the attack right after our nuke could send enough troops into `target` by land.
    pub(super) fn can_strike(&self, game: &GameImpl, target: SmallId) -> bool {
        game.player(target).player_type() != PlayerType::Bot
            && !game.player(self.player).is_friendly(target, false)
            && self.attack_behavior.borrow().strike_troops(game, target)
                >= game.player(target).troops() * STRIKE_MIN_STACK_RATIO
            && game.player(self.player).shares_border_with(target)
    }

    pub(super) fn follows_up_strikes(game: &GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible
    }

    /// `followUpStrikes()`, called every tick: attacks a strike's target the moment its bomb, or its MIRV's last warhead, lands.
    pub fn follow_up_strikes(&mut self, game: &mut GameImpl) {
        if !game.has(Change::NationsLikeHumans5899) {
            return;
        }
        if Self::follows_up_strikes(game) && game.player(self.player).unit_count(UnitType::MIRV) > 0
        {
            self.track_mirv_strikes(game);
        }
        if self.strikes.is_empty() {
            return;
        }
        let mut pending: Vec<Strike> = Vec::new();
        let count = self.strikes.len();
        for i in 0..count {
            if self.strikes[i].nuke.is_none() {
                let found = self.find_strike_nuke(game, self.strikes[i].tile);
                self.strikes[i].nuke = found;
            }
            let (target, tick, nuke) = (
                self.strikes[i].target,
                self.strikes[i].tick,
                self.strikes[i].nuke,
            );
            let Some(nuke) = nuke else {
                if (game.ticks() as f64) - (tick as f64) <= STRIKE_SPAWN_TICKS {
                    pending.push(Strike {
                        target,
                        tile: self.strikes[i].tile,
                        tick,
                        nuke: None,
                    });
                }
                continue;
            };
            if game.unit_any(nuke).is_active() {
                pending.push(Strike {
                    target,
                    tile: self.strikes[i].tile,
                    tick,
                    nuke: Some(nuke),
                });
                continue;
            }
            if game.unit_any(nuke).unit_type() == UnitType::MIRV {
                // A MIRV deletes itself when it splits into warheads.
                if game.player(self.player).unit_count(UnitType::MIRVWarhead) > 0 {
                    pending.push(Strike {
                        target,
                        tile: self.strikes[i].tile,
                        tick,
                        nuke: Some(nuke),
                    });
                    continue;
                }
            } else if !game.unit_any(nuke).reached_target() {
                // Intercepted bombs leave the target's troops alone.
                continue;
            }
            if game.player(target).is_alive() {
                self.attack_behavior.borrow_mut().strike(game, target);
            }
        }
        self.strikes = pending;
    }

    /// Our MIRVs at bordering players are strikes too: a MIRV leaves next to no troops to defend with.
    pub(super) fn track_mirv_strikes(&mut self, game: &GameImpl) {
        for mirv in game.player(self.player).units_of(UnitType::MIRV) {
            if self.strikes.iter().any(|s| s.nuke == Some(mirv)) {
                continue;
            }
            let unit = game.unit_any(mirv);
            let (Some(target), Some(tile)) = (unit.target_player(), unit.target_tile()) else {
                continue;
            };
            if target == 0 {
                continue;
            }
            if game.player(self.player).is_friendly(target, false) {
                continue;
            }
            if !game.player(self.player).shares_border_with(target) {
                continue;
            }
            self.strikes.push(Strike {
                target,
                tile,
                tick: game.ticks(),
                nuke: Some(mirv),
            });
        }
    }

    pub(super) fn find_strike_nuke(&self, game: &GameImpl, tile: TileRef) -> Option<UnitId> {
        game.player(self.player)
            .units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb])
            .into_iter()
            .find(|&nuke| {
                game.unit_any(nuke).target_tile() == Some(tile)
                    && !self.strikes.iter().any(|s| s.nuke == Some(nuke))
            })
    }

    pub(super) fn troops_attacking(&self, game: &GameImpl, target: SmallId) -> f64 {
        let mut troops = 0.0;
        for &attack in game.player(self.player).outgoing_attacks() {
            if game.attack(attack).target() == target {
                troops += game.attack(attack).troops();
            }
        }
        troops
    }

    pub(super) fn find_incoming_nuke_player(&self, game: &GameImpl) -> Option<SmallId> {
        for nuke in game.units_of_types(&[UnitType::AtomBomb, UnitType::HydrogenBomb]) {
            let unit = game.unit_any(nuke);
            let sender = unit.owner;
            if game.player(self.player).is_friendly(sender, false) {
                continue;
            }
            if let Some(target_tile) = unit.target_tile()
                && game.owner_id(target_tile) == self.player
            {
                return Some(sender);
            }
        }
        None
    }

    /// Medium and up: the enemy attacking our teammates hardest, else the one they attack hardest.
    pub(super) fn find_teammate_fight_target(&self, game: &GameImpl) -> Option<SmallId> {
        if Self::game_mode(game) != GameMode::Team || Self::difficulty(game) == Difficulty::Easy {
            return None;
        }
        let teammates: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != self.player && game.player(self.player).is_on_same_team(p))
            .collect();
        let is_enemy = |p: SmallId| {
            p != 0
                && game.player(p).player_type() != PlayerType::Bot
                && !game.player(self.player).is_friendly(p, false)
        };
        let mut best: Option<SmallId> = None;
        let mut best_troops = 0.0;
        for &mate in &teammates {
            for attack in game.player(mate).incoming_attacks() {
                let attacker = game.attack(attack).attacker();
                if is_enemy(attacker) && game.attack(attack).troops() > best_troops {
                    best = Some(attacker);
                    best_troops = game.attack(attack).troops();
                }
            }
        }
        if best.is_some() {
            return best;
        }
        for &mate in &teammates {
            for &attack in game.player(mate).outgoing_attacks() {
                let target = game.attack(attack).target();
                if is_enemy(target) && game.attack(attack).troops() > best_troops {
                    best = Some(target);
                    best_troops = game.attack(attack).troops();
                }
            }
        }
        best
    }
}
