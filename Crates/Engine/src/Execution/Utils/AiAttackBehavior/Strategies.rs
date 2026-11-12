// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: strategies): attack strategy selection and boat target search.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    pub(super) fn choose_attack(&mut self, game: &mut GameImpl) {
        assert!(self.alliance_behavior.is_some(), "not initialized");

        // The set's insertion order feeds the stable troop sort below.
        let mut bordering_player_set: OrderedSet<SmallId> = OrderedSet::new();
        let mut border_has_non_nuked_terra_nullius = false;
        let small_id = self.player;
        let player_neighbors;
        {
            let g: &GameImpl = game;
            for t in g.player(self.player).border_tiles().iter() {
                g.map.for_each_neighbor_nswe(t, |n| {
                    if !g.map.is_land(n) || g.map.is_impassable(n) {
                        return;
                    }
                    let owner = g.map.owner_id(n);
                    if owner == small_id {
                        return;
                    }
                    if owner != 0 {
                        bordering_player_set.insert(owner);
                    }
                    if !g.map.has_owner(n) && !g.map.has_fallout(n) {
                        border_has_non_nuked_terra_nullius = true;
                    }
                });
            }
            player_neighbors = g.player(self.player).nearby();
        }
        if Self::v2(game) {
            self.land_neighbors = Some(bordering_player_set.iter().copied().collect());
        }
        if !Self::legacy31(game) {
            for &n in &player_neighbors {
                if n != 0 {
                    bordering_player_set.insert(n);
                }
            }
        }
        let mut bordering_players: Vec<SmallId> = bordering_player_set.iter().copied().collect();
        sort_by_key_f64(&mut bordering_players, |&p| game.player(p).troops());
        let bordering_friends: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| self.is_friendly(game, o))
            .collect();
        let bordering_enemies: Vec<SmallId> = bordering_players
            .iter()
            .copied()
            .filter(|&o| !self.is_friendly(game, o))
            .collect();

        // Terra nullius, but not nuked territory (direct border or across a river).
        if Self::legacy31(game) {
            if border_has_non_nuked_terra_nullius {
                self.send_attack(game, 0, false);
                return;
            }
        } else {
            let has_non_nuked_terra_nullius =
                border_has_non_nuked_terra_nullius || player_neighbors.contains(&0);
            if has_non_nuked_terra_nullius && self.send_attack(game, 0, false) {
                return;
            }
        }

        let hold_boats = Self::v2(game) && self.holds_boats_under_attack(game);
        if bordering_enemies.is_empty() {
            if !hold_boats && self.chance(5.0) {
                self.attack_with_random_boat(game, &[]);
            }
        } else {
            if !hold_boats && self.chance(10.0) {
                self.attack_with_random_boat(game, &bordering_enemies);
                return;
            }
            let behavior = self.alliance_behavior.clone().expect("checked above");
            behavior
                .borrow_mut()
                .maybe_send_alliance_requests(game, &bordering_enemies);
        }

        self.attack_best_target(game, &bordering_friends, &bordering_enemies);
    }

    pub(super) fn attack_with_random_boat(
        &mut self,
        game: &mut GameImpl,
        bordering_enemies: &[SmallId],
    ) {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return;
        }
        // The maximum number of transport ships may already be out.
        if game.has(Change::NationAiRework2984)
            && game.player(self.player).unit_count(UnitType::TransportShip)
                >= game.config().boat_max_number() as i64
        {
            return;
        }
        let shore = self.shore_tiles(game, self.player);
        if shore.is_empty() {
            return;
        }
        let src = *self.random.borrow_mut().rand_element(&shore);

        // High-interest targets first (unowned or bot-owned), mainly relevant early on.
        let mut found = self.find_random_boat_target(game, src, bordering_enemies, true);
        if found.is_none() {
            found = self.find_random_boat_target(game, src, bordering_enemies, false);
            if found.is_none() {
                return;
            }
        }
        let (dst, launch) = found.expect("checked");
        let v2 = Self::v2(game);
        if v2 && self.sail(game, launch, dst, None).1.is_some() {
            return;
        }

        let owner = game.owner_id(dst);
        if v2 && self.lands_beachheads(game) {
            self.send_beachhead(game, owner, dst);
            return;
        }
        if !game.has(Change::AiTroopCap4239) {
            // Before PR 4239 the boat always carried a fifth of the troops, unchecked.
            let troops = self.troops(game) / 5.0;
            game.add_execution(Box::new(TransportShipExecution::new(
                self.player,
                dst,
                Some(troops),
            )));
            return;
        }
        let cap = if owner != 0 {
            self.troop_send_cap(game, None)
        } else if game.has(Change::NationExpansionCap5290) {
            self.troop_send_cap_for_expansion(game)
        } else {
            f64::INFINITY
        };
        let troops = (self.home_troops(game) / 5.0).min(cap);
        if troops < 1.0 {
            return;
        }
        // Hard and Impossible: skip when sending under 20% of the target's troops.
        if owner != 0 && self.is_attack_too_weak(game, troops, owner) {
            return;
        }
        let exec = Box::new(TransportShipExecution::new(self.player, dst, Some(troops)));
        self.send(game, exec, troops);
    }

    pub(super) fn find_random_boat_target(
        &mut self,
        game: &GameImpl,
        tile: TileRef,
        bordering_enemies: &[SmallId],
        high_interest_only: bool,
    ) -> Option<(TileRef, TileRef)> {
        let x = game.x(tile) as f64;
        let y = game.y(tile) as f64;
        let mut unreachable_players: Vec<SmallId> = Vec::new();
        for _ in 0..500 {
            let (rand_x, rand_y) = {
                let mut r = self.random.borrow_mut();
                (
                    r.next_int(x - 150.0, x + 150.0),
                    r.next_int(y - 150.0, y + 150.0),
                )
            };
            if !game.is_valid_coord(rand_x as i32, rand_y as i32) {
                continue;
            }
            let rand_tile = game.tile_ref(rand_x as i32, rand_y as i32);
            if !game.is_land(rand_tile) {
                continue;
            }
            if game.map.is_impassable(rand_tile) {
                continue;
            }
            let owner = game.owner_id(rand_tile);
            if owner == self.player {
                continue;
            }
            // Skip players already known to be unreachable.
            if owner != 0 && unreachable_players.contains(&owner) {
                continue;
            }
            // No boats to players we share a border with, that looks stupid.
            if owner != 0 && bordering_enemies.contains(&owner) {
                continue;
            }
            // In FFA, no boats into players stronger than us.
            if self.is_ffa(game) && owner != 0 && game.player(owner).troops() > self.troops(game) {
                continue;
            }
            let matches_criteria = if high_interest_only {
                owner == 0 || game.player(owner).player_type() == PlayerType::Bot
            } else {
                // The owner is the receiver upstream, which matters for disconnected owners.
                owner == 0 || !game.player(owner).is_friendly(self.player, false)
            };
            if !matches_criteria {
                continue;
            }
            // `canBuildTransportShip(...) === false` is a strict comparison upstream.
            let Some(launch) = can_build_transport_ship(game, self.player, rand_tile) else {
                if owner != 0 {
                    unreachable_players.push(owner);
                }
                continue;
            };
            return Some((rand_tile, launch));
        }
        None
    }

    // `attackBestTarget` is called with friends and enemies sorted by troops (ascending).
    pub(super) fn attack_best_target(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        if Self::v2(game) {
            self.attack_best_target_v2(game, bordering_friends, bordering_enemies);
            return;
        }
        // With high starting gold nations expand slowly and bots steal their structures:
        // attack bots before the ratio checks.
        if game.has(Change::BotStructureFarming2895)
            && self.has_neighboring_bot_with_structures(game)
            && self.attack_bots(game)
        {
            return;
        }
        // Save up troops until the reserve ratio is reached.
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Maybe save up until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) && !self.chance(10.0) {
            return;
        }
        for strategy in Self::attack_strategies(game) {
            if self.run_strategy(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                &mut None,
            ) {
                return;
            }
        }
    }

    pub(super) fn attack_best_target_v2(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        let difficulty = game.config().game_config().difficulty;
        let hard_or_impossible =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;

        // Hard and Impossible: answer attacks right away, even below the reserve and trigger ratios.
        if hard_or_impossible && self.retaliate(game) {
            return;
        }
        // A walled-in bot costs next to nothing to annex, so do not save up for it.
        self.annex_walled_in_bots(game);

        // Attack bots before the ratio checks when they hold structures, which we need back,
        // and with high starting gold, as those checks count the troops of cities built in a rush.
        if (self.has_neighboring_bot_with_structures(game)
            || (self.expands_past_city_troops(game) && self.has_trigger_ratio_troops(game, true)))
            && self.attack_bots(game)
        {
            return;
        }
        // Save up troops until the reserve ratio is reached.
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Medium: answer attacks without saving up to the trigger ratio first.
        if difficulty == Difficulty::Medium && self.retaliate(game) {
            return;
        }
        // Maybe save up until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game, false) && !self.chance(10.0) {
            return;
        }
        let mut groups: Option<Vec<Vec<SmallId>>> = None;
        for strategy in Self::attack_strategies(game) {
            if self.run_strategy(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                &mut groups,
            ) {
                return;
            }
        }
    }

    pub(super) fn attack_strategies(game: &GameImpl) -> Vec<Strategy> {
        let mut list = Self::attack_strategy_list(game);
        if !game.has(Change::NationAiRework2984) {
            list.retain(|s| !matches!(s, Strategy::Donate));
        }
        list
    }

    pub(super) fn attack_strategy_list(game: &GameImpl) -> Vec<Strategy> {
        use Strategy::*;
        if Self::v2(game) {
            return match game.config().game_config().difficulty {
                Difficulty::Easy => vec![Nuked, Bots, Retaliate, Assist, Betray, Hated, Weakest],
                Difficulty::Medium => vec![
                    Bots, Nuked, Assist, Betray, Hated, Afk, Traitor, Crown, Weakest, Island,
                    Donate,
                ],
                Difficulty::Hard => vec![
                    Bots, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Juicy, Victim,
                    Crown, Weakest, Island, Donate,
                ],
                Difficulty::Impossible => vec![
                    Bots, VeryWeak, Betray, Assist, Victim, Crown, Traitor, Juicy, Afk, Nuked,
                    Hated, Weakest, Island, Donate,
                ],
            };
        }
        let juicy = game.has(Change::NationJuicy5294);
        match game.config().game_config().difficulty {
            Difficulty::Easy => vec![Nuked, Bots, Retaliate, Assist, Betray, Hated, Weakest],
            Difficulty::Medium => vec![
                Bots, Nuked, Retaliate, Assist, Betray, Hated, Afk, Traitor, Weakest, Island,
                Donate,
            ],
            Difficulty::Hard if !juicy => vec![
                Bots, Retaliate, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Victim,
                Weakest, Island, Donate,
            ],
            Difficulty::Impossible if !juicy => vec![
                Retaliate, Bots, VeryWeak, Assist, Traitor, Afk, Betray, Victim, Nuked, Hated,
                Weakest, Island, Donate,
            ],
            Difficulty::Hard => vec![
                Bots, Retaliate, Assist, Betray, Nuked, Traitor, Afk, Hated, VeryWeak, Juicy,
                Victim, Weakest, Island, Donate,
            ],
            Difficulty::Impossible => vec![
                Retaliate, Bots, VeryWeak, Betray, Assist, Victim, Traitor, Juicy, Afk, Nuked,
                Hated, Weakest, Island, Donate,
            ],
        }
    }

    pub(super) fn run_strategy(
        &mut self,
        game: &mut GameImpl,
        strategy: Strategy,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> bool {
        if Self::v2(game) {
            return self.run_strategy_v2(
                game,
                strategy,
                bordering_friends,
                bordering_enemies,
                groups,
            );
        }
        match strategy {
            Strategy::Crown => false,
            Strategy::Retaliate => match self.find_incoming_attack_player(game) {
                Some(attacker) => {
                    let r = self.send_attack(game, attacker, true);
                    Self::sent(game, r)
                }
                None => false,
            },
            Strategy::Bots => self.attack_bots(game),
            Strategy::Assist => self.assist_allies(game),
            Strategy::Traitor => match self.find_traitor(game, bordering_enemies) {
                Some(traitor) => {
                    let r = self.send_attack(game, traitor, false);
                    Self::sent(game, r)
                }
                None => false,
            },
            Strategy::Afk => {
                // Enemies are sorted by troops, so the first match is the weakest AFK enemy.
                let afk = bordering_enemies.iter().copied().find(|&enemy| {
                    game.player(enemy).is_disconnected()
                        && (!self.is_ffa(game)
                            || game.player(enemy).troops() < self.troops(game) * 3.0)
                });
                match afk {
                    Some(afk) => {
                        let r = self.send_attack(game, afk, false);
                        Self::sent(game, r)
                    }
                    None => false,
                }
            }
            Strategy::Betray => {
                self.maybe_betray_and_attack(game, bordering_friends, bordering_enemies)
            }
            Strategy::Nuked => {
                if self.is_bordering_nuked_territory(game) {
                    let r = self.send_attack(game, 0, false);
                    Self::sent(game, r)
                } else {
                    false
                }
            }
            Strategy::Victim => match self.find_victim(game, bordering_enemies) {
                Some(victim) => {
                    let r = self.send_attack(game, victim, false);
                    Self::sent(game, r)
                }
                None => false,
            },
            Strategy::Juicy => match self.find_juicy_target(game, bordering_enemies) {
                Some(target) => {
                    let r = self.send_attack(game, target, false);
                    Self::sent(game, r)
                }
                None => false,
            },
            Strategy::Hated => {
                for (other, relation) in game.player(self.player).all_relations_sorted() {
                    if relation != Relation::Hostile {
                        continue;
                    }
                    if self.is_friendly(game, other) {
                        continue;
                    }
                    if self.is_ffa(game) && game.player(other).troops() > self.troops(game) * 3.0 {
                        continue;
                    }
                    let r = self.send_attack(game, other, false);
                    return Self::sent(game, r);
                }
                false
            }
            Strategy::VeryWeak => match self.find_very_weak_enemy(game, bordering_enemies) {
                Some(enemy) => {
                    let r = self.send_attack(game, enemy, false);
                    Self::sent(game, r)
                }
                None => false,
            },
            Strategy::Weakest => {
                if let Some(&weakest) = bordering_enemies.first() {
                    // In FFA, no attack on someone with more troops than us.
                    if !self.is_ffa(game) || game.player(weakest).troops() < self.troops(game) {
                        let r = self.send_attack(game, weakest, false);
                        return Self::sent(game, r);
                    }
                }
                false
            }
            Strategy::Island => {
                if bordering_enemies.is_empty()
                    && let Some(enemy) = self.find_nearest_island_enemy(game)
                {
                    let r = self.send_attack(game, enemy, false);
                    return Self::sent(game, r);
                }
                false
            }
            Strategy::Donate => self.donate_troops(game),
        }
    }

    /// The target groups, tried in order, each sorted by troops (ascending).
    pub(super) fn target_groups(
        &mut self,
        game: &mut GameImpl,
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> Vec<Vec<SmallId>> {
        if let Some(g) = groups {
            return g.clone();
        }
        let difficulty = game.config().game_config().difficulty;
        let by_troops = |game: &GameImpl, list: &mut Vec<SmallId>| {
            sort_by_key_f64(list, |&p| game.player(p).troops());
        };
        let land_front = bordering_enemies
            .iter()
            .any(|&enemy| self.borders_by_land(game, enemy));
        let hard_or_impossible =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;
        let result = if difficulty == Difficulty::Impossible && land_front {
            let mut overseas = self.overseas_targets(game, bordering_enemies);
            by_troops(game, &mut overseas);
            vec![bordering_enemies.to_vec(), overseas]
        } else if hard_or_impossible && !land_front {
            let mut all = bordering_enemies.to_vec();
            all.extend(self.overseas_targets(game, bordering_enemies));
            by_troops(game, &mut all);
            vec![all]
        } else {
            vec![bordering_enemies.to_vec()]
        };
        *groups = Some(result.clone());
        result
    }

    /// What `finder` picks among the targets we can reach right now, from the first group where it picks anything.
    pub(super) fn pick(
        &mut self,
        game: &mut GameImpl,
        finder: Finder,
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> Option<SmallId> {
        let easy = game.config().game_config().difficulty == Difficulty::Easy;
        for group in self.target_groups(game, bordering_enemies, groups) {
            let mut pool = group;
            while let Some(target) = self.run_finder(game, finder, &pool) {
                if easy || self.can_reach(game, target) {
                    return Some(target);
                }
                pool.retain(|&enemy| enemy != target);
            }
        }
        None
    }

    pub(super) fn run_finder(
        &self,
        game: &GameImpl,
        finder: Finder,
        pool: &[SmallId],
    ) -> Option<SmallId> {
        match finder {
            Finder::Traitor => self.find_traitor(game, pool),
            Finder::Afk => pool.iter().copied().find(|&enemy| {
                game.player(enemy).is_disconnected()
                    && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 3.0)
            }),
            Finder::Victim => self.find_victim(game, pool),
            Finder::Juicy => self.find_juicy_target(game, pool),
            Finder::VeryWeak => self.find_very_weak_enemy(game, pool),
            Finder::Weakest => pool.iter().copied().find(|&enemy| {
                !self.is_ffa(game) || game.player(enemy).troops() < self.troops(game)
            }),
            Finder::First => pool.first().copied(),
        }
    }

    pub(super) fn run_strategy_v2(
        &mut self,
        game: &mut GameImpl,
        strategy: Strategy,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
        groups: &mut Option<Vec<Vec<SmallId>>>,
    ) -> bool {
        let difficulty = game.config().game_config().difficulty;
        let picked = |this: &mut Self,
                      game: &mut GameImpl,
                      f: Finder,
                      groups: &mut Option<Vec<Vec<SmallId>>>| {
            this.pick(game, f, bordering_enemies, groups)
        };
        match strategy {
            Strategy::Retaliate => self.retaliate(game),
            Strategy::Bots => self.attack_bots(game),
            Strategy::Assist => self.assist_allies(game),
            Strategy::Traitor => match picked(self, game, Finder::Traitor, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Afk => match picked(self, game, Finder::Afk, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Betray => {
                self.maybe_betray_and_attack(game, bordering_friends, bordering_enemies)
            }
            Strategy::Nuked => {
                if self.is_bordering_nuked_territory(game) {
                    self.send_attack(game, 0, false)
                } else {
                    false
                }
            }
            Strategy::Victim => match picked(self, game, Finder::Victim, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Juicy => match picked(self, game, Finder::Juicy, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Hated => {
                let relations = game.player(self.player).all_relations_sorted();
                for (other, relation) in relations {
                    if relation != Relation::Hostile {
                        continue;
                    }
                    if self.is_friendly(game, other) {
                        continue;
                    }
                    if self.is_ffa(game) && game.player(other).troops() > self.troops(game) * 3.0 {
                        continue;
                    }
                    if difficulty != Difficulty::Easy && !self.can_reach(game, other) {
                        continue;
                    }
                    return self.send_attack(game, other, false);
                }
                false
            }
            Strategy::VeryWeak => match picked(self, game, Finder::VeryWeak, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Weakest => match picked(self, game, Finder::Weakest, groups) {
                Some(t) => self.send_attack(game, t, false),
                None => false,
            },
            Strategy::Island => {
                if picked(self, game, Finder::First, groups).is_none()
                    && let Some(enemy) = self.find_nearest_island_enemy(game)
                {
                    return self.send_attack(game, enemy, false);
                }
                false
            }
            Strategy::Donate => self.donate_troops(game),
            Strategy::Crown => {
                let leader = self.find_crown_target(game, bordering_enemies);
                match leader {
                    Some(l) => {
                        self.should_attack(game, l) && self.send_land_attack(game, l, None, Some(l))
                    }
                    None => false,
                }
            }
        }
    }

    pub(super) fn has_neighboring_bot_with_structures(&self, game: &GameImpl) -> bool {
        game.player(self.player).nearby().into_iter().any(|n| {
            n != 0
                && game.player(n).player_type() == PlayerType::Bot
                && !self.is_friendly(game, n)
                && Self::owns_structures(game, n)
        })
    }

    pub(super) fn owns_structures(game: &GameImpl, player: SmallId) -> bool {
        game.player(player)
            .units()
            .iter()
            .any(|&u| game.unit_any(u).unit_type().is_structure())
    }

    pub(super) fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        self.home_troops(game) / p.max_troops() >= self.reserve_ratio
    }

    pub(super) fn has_trigger_ratio_troops(&self, game: &GameImpl, expansion: bool) -> bool {
        if Self::v2(game) {
            return self.home_troops(game) / self.reserve_basis(game, expansion)
                >= self.trigger_ratio;
        }
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.trigger_ratio
    }

    /// The max troops the troop ratios refer to. With high starting gold, cities do not hold back
    /// expanding into free land and bots.
    pub(super) fn reserve_basis(&self, game: &GameImpl, expansion: bool) -> f64 {
        let p = game.player(self.player);
        if expansion && self.expands_past_city_troops(game) {
            p.max_troops_without_cities()
        } else {
            p.max_troops()
        }
    }

    pub(super) fn expands_past_city_troops(&self, game: &GameImpl) -> bool {
        has_high_starting_gold(game, self.player)
    }
}
