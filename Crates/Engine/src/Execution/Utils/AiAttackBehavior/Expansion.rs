// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: expansion): free-land expansion and sending attacks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    /// `attackRandomTarget()`.
    pub fn attack_random_target(&mut self, game: &mut GameImpl) {
        // Save up troops until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) {
            return;
        }
        // Retaliate against incoming attacks.
        if let Some(incoming) = self.find_incoming_attack_player(game) {
            let r = self.send_attack(game, incoming, true);
            if Self::sent(game, r) {
                return;
            }
        }
        // Select a traitor as an enemy.
        if let Some(to_attack) = self.get_neighbor_traitor_to_attack(game, false)
            && self.chance(3.0)
        {
            let r = self.send_attack(game, to_attack, false);
            if Self::sent(game, r) {
                return;
            }
        }
        // Choose a new enemy randomly.
        let neighbors = game.player(self.player).nearby();
        let shuffled = self.random.borrow_mut().shuffle_array(&neighbors);
        for neighbor in shuffled {
            if neighbor == 0 {
                continue;
            }
            if self.is_friendly(game, neighbor) {
                continue;
            }
            let t = game.player(neighbor).player_type();
            if (t == PlayerType::Nation || t == PlayerType::Human) && self.chance(2.0) {
                continue;
            }
            let r = self.send_attack(game, neighbor, false);
            if Self::sent(game, r) {
                return;
            }
        }
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(
        &mut self,
        game: &GameImpl,
        include_allies: bool,
    ) -> Option<SmallId> {
        if game.config().nation_alliances_disabled() {
            return None;
        }
        let traitors: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| {
                n != 0
                    && (include_allies || !self.is_friendly(game, n))
                    && game.player(n).is_traitor()
            })
            .collect();
        if traitors.is_empty() {
            None
        } else {
            Some(*self.random.borrow_mut().rand_element(&traitors))
        }
    }

    /// `expandIntoFreeLand()`: grows into free land we border, never by boat and never into a player.
    pub fn expand_into_free_land(&mut self, game: &mut GameImpl) -> bool {
        if !self.has_land_border_with_terra_nullius(game) {
            return false;
        }
        self.send_land_attack(game, 0, None, None)
    }

    /// `forceSendAttack(target)`: attacks with half of the troops regardless of the checks.
    pub fn force_send_attack(&mut self, game: &mut GameImpl, target: SmallId) {
        let troops = self.troops(game) / 2.0;
        let target_id = Self::target_id(game, target);
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        )));
    }

    pub(super) fn target_id(game: &GameImpl, target: SmallId) -> Option<String> {
        (target != 0).then(|| game.player(target).id().to_string())
    }

    /// `sendAttack(target, force)`: true when an attack or boat was launched.
    pub fn send_attack(&mut self, game: &mut GameImpl, target: SmallId, force: bool) -> bool {
        if !force && !self.should_attack(game, target) {
            return false;
        }
        if Self::legacy31(game) {
            if game.player(self.player).shares_border_with(target) {
                return self.send_land_attack(game, target, None, None);
            } else if target != 0 {
                return self.send_boat_attack(game, target);
            }
            return false;
        }
        if target != 0 {
            if game.player(self.player).shares_border_with(target) {
                self.send_land_attack(game, target, None, None)
            } else {
                self.send_boat_attack(game, target)
            }
        } else if self.has_land_border_with_terra_nullius(game) {
            // `sharesBorderWith(TerraNullius)` counts water as terra nullius, so land is checked.
            self.send_land_attack(game, target, None, None)
        } else {
            self.send_boat_attack_to_nearby_terra_nullius(game)
        }
    }

    pub(super) fn has_land_border_with_terra_nullius(&self, game: &GameImpl) -> bool {
        let map = &game.map;
        let mut nbuf = [0; 4];
        for border in game.player(self.player).border_tiles().iter() {
            let n = map.neighbors4_nswe(border, &mut nbuf);
            for &neighbor in &nbuf[..n] {
                if map.is_land(neighbor) && !map.is_impassable(neighbor) && !map.has_owner(neighbor)
                {
                    return true;
                }
            }
        }
        false
    }

    /// The player's shore border tiles, in border-set order.
    pub(super) fn shore_tiles(&self, game: &GameImpl, player: SmallId) -> Vec<TileRef> {
        game.player(player)
            .border_tiles()
            .iter()
            .filter(|&t| {
                if Self::legacy31(game) {
                    game.map.is_ocean_shore(t)
                } else {
                    game.map.is_shore(t)
                }
            })
            .collect()
    }

    // Scans shore border tiles (every 10th) for unowned land 5 tiles away across water in each
    // cardinal direction and sends a transport ship to the first match.
    pub(super) fn send_boat_attack_to_nearby_terra_nullius(&mut self, game: &mut GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return false;
        }
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return false;
        }
        const DIRECTIONS: [(i32, i32); 4] = [(0, -1), (0, 1), (-1, 0), (1, 0)];
        let shores = self.shore_tiles(game, self.player);
        let mut i = 0;
        while i < shores.len() {
            let border = shores[i];
            let bx = game.x(border) as i32;
            let by = game.y(border) as i32;
            for (dx, dy) in DIRECTIONS {
                let (x1, y1) = (bx + dx, by + dy);
                if !game.is_valid_coord(x1, y1) {
                    continue;
                }
                if !game.is_water(game.tile_ref(x1, y1)) {
                    continue;
                }
                let (nx, ny) = (bx + dx * 5, by + dy * 5);
                if !game.is_valid_coord(nx, ny) {
                    continue;
                }
                let tile = game.tile_ref(nx, ny);
                if !game.is_land(tile) {
                    continue;
                }
                if game.map.is_impassable(tile) {
                    continue;
                }
                if game.has_owner(tile) {
                    continue;
                }
                if game.map.has_fallout(tile) {
                    continue;
                }
                let launch = can_build_transport_ship(game, self.player, tile);
                if Self::v2(game) {
                    let Some(launch) = launch else { continue };
                    if self.sail(game, launch, tile, None).1.is_some() {
                        continue;
                    }
                    if self.lands_beachheads(game) {
                        return self.send_beachhead(game, 0, tile);
                    }
                } else if !launch.is_some_and(|t| t != 0) {
                    continue;
                }
                let troops = if game.has(Change::NationExpansionCap5290) {
                    (self.home_troops(game) / 5.0).min(self.troop_send_cap_for_expansion(game))
                } else {
                    self.home_troops(game) / 5.0
                };
                if troops < 1.0 {
                    return false;
                }
                let exec = Box::new(TransportShipExecution::new(self.player, tile, Some(troops)));
                self.send(game, exec, troops);
                return true;
            }
            i += 10;
        }
        false
    }
}
