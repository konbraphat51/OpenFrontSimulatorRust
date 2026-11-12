// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts: attack decisions shared by tribes and nations.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Players are small ids with `0` standing for terra nullius. Upstream shares one `PseudoRandom`
//! and the nation behaviours between the owner execution and this object, hence the `Rc`s.

use crate::Builds::Rules::Change;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::{bounding_box_center, calculate_bounding_box_center};
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::DonateTroopExecution::DonateTroopsExecution;
use crate::Execution::Nation::NationAllianceBehavior::NationAllianceBehavior;
use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::NationUtils::find_juiciest_target;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::Util::closest_two_tiles;
use crate::Execution::Utils::EmojiIds::{
    emoji_assist_accept, emoji_assist_relation_too_low, emoji_assist_target_ally,
    emoji_assist_target_me,
};
use crate::Game::Game::{
    Difficulty, EmojiRecipient, GameMode, GameType, PlayerType, Relation, SmallId, TeamCountConfig,
    UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use crate::Game::TransportShipUtils::can_build_transport_ship;
use std::cell::RefCell;
use std::cmp::Ordering;
use std::rc::Rc;

/// The attack strategies of `getAttackStrategies`.
#[derive(Clone, Copy)]
enum Strategy {
    Retaliate,
    Bots,
    Assist,
    Traitor,
    Afk,
    Betray,
    Nuked,
    Victim,
    Juicy,
    Hated,
    VeryWeak,
    Weakest,
    Island,
    Donate,
}

/// Attack decisions of one AI player.
pub struct AiAttackBehavior {
    random: Rc<RefCell<PseudoRandom>>,
    player: SmallId,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    alliance_behavior: Option<Rc<RefCell<NationAllianceBehavior>>>,
    emoji_behavior: Option<Rc<RefCell<NationEmojiBehavior>>>,
    bot_attack_troops_sent: f64,
}

/// `canBuildTransportShip(...)` used as a JS condition: a landing tile 0 would be falsy.
fn can_build_truthy(game: &GameImpl, player: SmallId, tile: TileRef) -> bool {
    can_build_transport_ship(game, player, tile).is_some_and(|t| t != 0)
}

/// Stable ascending sort by a key, treating incomparable keys as equal (like a NaN comparator).
fn sort_by_key_f64<T>(items: &mut [T], key: impl Fn(&T) -> f64) {
    items.sort_by(|a, b| key(a).partial_cmp(&key(b)).unwrap_or(Ordering::Equal));
}

impl AiAttackBehavior {
    /// `new AiAttackBehavior(random, game, player, triggerRatio, reserveRatio, expandRatio, allianceBehavior?, emojiBehavior?)`.
    pub fn new(
        random: Rc<RefCell<PseudoRandom>>,
        player: SmallId,
        trigger_ratio: f64,
        reserve_ratio: f64,
        expand_ratio: f64,
        alliance_behavior: Option<Rc<RefCell<NationAllianceBehavior>>>,
        emoji_behavior: Option<Rc<RefCell<NationEmojiBehavior>>>,
    ) -> Self {
        Self {
            random,
            player,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            alliance_behavior,
            emoji_behavior,
            bot_attack_troops_sent: 0.0,
        }
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn troops(&self, game: &GameImpl) -> f64 {
        game.player(self.player).troops()
    }

    fn is_friendly(&self, game: &GameImpl, other: SmallId) -> bool {
        game.player(self.player).is_friendly(other, false)
    }

    /// `maybeAttack()`: the nation entry point.
    pub fn maybe_attack(&mut self, game: &mut GameImpl) {
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
        for &n in &player_neighbors {
            if n != 0 {
                bordering_player_set.insert(n);
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
        let has_non_nuked_terra_nullius =
            border_has_non_nuked_terra_nullius || player_neighbors.contains(&0);
        if has_non_nuked_terra_nullius && self.send_attack(game, 0, false) {
            return;
        }

        if bordering_enemies.is_empty() {
            if self.chance(5.0) {
                self.attack_with_random_boat(game, &[]);
            }
        } else {
            if self.chance(10.0) {
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

    fn attack_with_random_boat(&mut self, game: &mut GameImpl, bordering_enemies: &[SmallId]) {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return;
        }
        // The maximum number of transport ships may already be out.
        if game.player(self.player).unit_count(UnitType::TransportShip)
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
        let mut dst = self.find_random_boat_target(game, src, bordering_enemies, true);
        if dst.is_none() {
            dst = self.find_random_boat_target(game, src, bordering_enemies, false);
            if dst.is_none() {
                return;
            }
        }
        let dst = dst.expect("checked");

        let owner = game.owner_id(dst);
        let cap = if owner != 0 {
            self.troop_send_cap(game)
        } else if game.config().rules().has(Change::NationExpansionCap5290) {
            self.troop_send_cap_for_expansion(game)
        } else {
            f64::INFINITY
        };
        let troops = (self.troops(game) / 5.0).min(cap);
        if troops < 1.0 {
            return;
        }
        // Hard and Impossible: skip when sending under 20% of the target's troops.
        if owner != 0 && self.is_attack_too_weak(game, troops, owner) {
            return;
        }
        game.add_execution(Box::new(TransportShipExecution::new(
            self.player,
            dst,
            Some(troops),
        )));
    }

    fn find_random_boat_target(
        &mut self,
        game: &GameImpl,
        tile: TileRef,
        bordering_enemies: &[SmallId],
        high_interest_only: bool,
    ) -> Option<TileRef> {
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
            if can_build_transport_ship(game, self.player, rand_tile).is_none() {
                if owner != 0 {
                    unreachable_players.push(owner);
                }
                continue;
            }
            return Some(rand_tile);
        }
        None
    }

    // `attackBestTarget` is called with friends and enemies sorted by troops (ascending).
    fn attack_best_target(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) {
        // With high starting gold nations expand slowly and bots steal their structures:
        // attack bots before the ratio checks.
        if self.has_neighboring_bot_with_structures(game) && self.attack_bots(game) {
            return;
        }
        // Save up troops until the reserve ratio is reached.
        if !self.has_reserve_ratio_troops(game) {
            return;
        }
        // Maybe save up until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game) && !self.chance(10.0) {
            return;
        }
        for strategy in Self::attack_strategies(game) {
            if self.run_strategy(game, strategy, bordering_friends, bordering_enemies) {
                return;
            }
        }
    }

    fn attack_strategies(game: &GameImpl) -> Vec<Strategy> {
        use Strategy::*;
        let juicy = game.config().rules().has(Change::NationJuicy5294);
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

    fn run_strategy(
        &mut self,
        game: &mut GameImpl,
        strategy: Strategy,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        match strategy {
            Strategy::Retaliate => match self.find_incoming_attack_player(game) {
                Some(attacker) => self.send_attack(game, attacker, true),
                None => false,
            },
            Strategy::Bots => self.attack_bots(game),
            Strategy::Assist => self.assist_allies(game),
            Strategy::Traitor => match self.find_traitor(game, bordering_enemies) {
                Some(traitor) => self.send_attack(game, traitor, false),
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
                    Some(afk) => self.send_attack(game, afk, false),
                    None => false,
                }
            }
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
            Strategy::Victim => match self.find_victim(game, bordering_enemies) {
                Some(victim) => self.send_attack(game, victim, false),
                None => false,
            },
            Strategy::Juicy => match self.find_juicy_target(game, bordering_enemies) {
                Some(target) => self.send_attack(game, target, false),
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
                    return self.send_attack(game, other, false);
                }
                false
            }
            Strategy::VeryWeak => match self.find_very_weak_enemy(game, bordering_enemies) {
                Some(enemy) => self.send_attack(game, enemy, false),
                None => false,
            },
            Strategy::Weakest => {
                if let Some(&weakest) = bordering_enemies.first() {
                    // In FFA, no attack on someone with more troops than us.
                    if !self.is_ffa(game) || game.player(weakest).troops() < self.troops(game) {
                        return self.send_attack(game, weakest, false);
                    }
                }
                false
            }
            Strategy::Island => {
                if bordering_enemies.is_empty()
                    && let Some(enemy) = self.find_nearest_island_enemy(game)
                {
                    return self.send_attack(game, enemy, false);
                }
                false
            }
            Strategy::Donate => self.donate_troops(game),
        }
    }

    fn has_neighboring_bot_with_structures(&self, game: &GameImpl) -> bool {
        game.player(self.player).nearby().into_iter().any(|n| {
            n != 0
                && game.player(n).player_type() == PlayerType::Bot
                && !self.is_friendly(game, n)
                && Self::owns_structures(game, n)
        })
    }

    fn owns_structures(game: &GameImpl, player: SmallId) -> bool {
        game.player(player)
            .units()
            .iter()
            .any(|&u| game.unit_any(u).unit_type().is_structure())
    }

    fn has_reserve_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.reserve_ratio
    }

    fn has_trigger_ratio_troops(&self, game: &GameImpl) -> bool {
        let p = game.player(self.player);
        p.troops() / p.max_troops() >= self.trigger_ratio
    }

    /// `findIncomingAttackPlayer()`: the attacker of the largest unfriendly incoming attack.
    pub fn find_incoming_attack_player(&self, game: &GameImpl) -> Option<SmallId> {
        let me = game.player(self.player);
        let mut incoming: Vec<_> = me
            .incoming_attacks()
            .into_iter()
            .filter(|&a| !self.is_friendly(game, game.attack(a).attacker()))
            .collect();
        // Bot attacks are ignored unless we are a bot.
        if me.player_type() != PlayerType::Bot {
            incoming.retain(|&a| {
                game.player(game.attack(a).attacker()).player_type() != PlayerType::Bot
            });
        }
        let mut largest_attack = 0.0;
        let mut largest_attacker = None;
        for a in incoming {
            let attack = game.attack(a);
            if attack.troops() <= largest_attack {
                continue;
            }
            largest_attack = attack.troops();
            largest_attacker = Some(attack.attacker());
        }
        largest_attacker
    }

    // Attacks many neighbouring bots in parallel, densest-last; `send_attack` does nothing when
    // the reserve is used up. Bots with structures come first.
    fn attack_bots(&mut self, game: &mut GameImpl) -> bool {
        let bots: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| {
                n != 0
                    && !self.is_friendly(game, n)
                    && game.player(n).player_type() == PlayerType::Bot
            })
            .collect();
        if bots.is_empty() {
            return false;
        }
        self.bot_attack_troops_sent = 0.0;

        let density = |p: SmallId| {
            let pl = game.player(p);
            pl.troops() / pl.num_tiles_owned() as f64
        };
        let mut sorted_bots = bots;
        sorted_bots.sort_by(|&a, &b| {
            let a_has = Self::owns_structures(game, a);
            let b_has = Self::owns_structures(game, b);
            if a_has != b_has {
                return if a_has {
                    Ordering::Less
                } else {
                    Ordering::Greater
                };
            }
            (density(a) - density(b))
                .partial_cmp(&0.0)
                .unwrap_or(Ordering::Equal)
        });
        let parallelism = self.bot_attack_max_parallelism(game);
        for &bot in sorted_bots.iter().take(parallelism) {
            self.send_attack(game, bot, false);
        }
        // Short-circuit the pipeline only when troops were actually allocated to bot attacks.
        self.bot_attack_troops_sent > 0.0
    }

    fn bot_attack_max_parallelism(&self, game: &GameImpl) -> usize {
        match game.config().game_config().difficulty {
            Difficulty::Easy => 1,
            Difficulty::Medium => {
                if self.chance(2.0) {
                    1
                } else {
                    2
                }
            }
            Difficulty::Hard => 3,
            Difficulty::Impossible => 100,
        }
    }

    fn assist_allies(&mut self, game: &mut GameImpl) -> bool {
        let emoji = self.emoji_behavior.clone().expect("not initialized");
        if game.config().disable_alliances() {
            return false;
        }
        for ally in game.player(self.player).allies() {
            if game.player(ally).targets().is_empty() {
                continue;
            }
            if (game.player(self.player).relation(ally) as u8) < Relation::Friendly as u8 {
                emoji.borrow_mut().send_emoji(
                    game,
                    EmojiRecipient::Player(ally),
                    &emoji_assist_relation_too_low(),
                );
                continue;
            }
            for target in game.player(ally).targets() {
                if target == self.player {
                    emoji.borrow_mut().send_emoji(
                        game,
                        EmojiRecipient::Player(ally),
                        &emoji_assist_target_me(),
                    );
                    continue;
                }
                if self.is_friendly(game, target) {
                    emoji.borrow_mut().send_emoji(
                        game,
                        EmojiRecipient::Player(ally),
                        &emoji_assist_target_ally(),
                    );
                    continue;
                }
                if !self.send_attack(game, target, false) {
                    continue;
                }
                game.player_mut(self.player).update_relation(ally, -20.0);
                emoji.borrow_mut().send_emoji(
                    game,
                    EmojiRecipient::Player(ally),
                    &emoji_assist_accept(),
                );
                return true;
            }
        }
        false
    }

    // A traitor who is not significantly stronger than us.
    fn find_traitor(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
        if game.config().disable_alliances() {
            return None;
        }
        bordering_enemies.iter().copied().find(|&enemy| {
            game.player(enemy).is_traitor()
                && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 1.2)
        })
    }

    fn maybe_betray_and_attack(
        &mut self,
        game: &mut GameImpl,
        bordering_friends: &[SmallId],
        bordering_enemies: &[SmallId],
    ) -> bool {
        let behavior = self.alliance_behavior.clone().expect("not initialized");
        if game.config().disable_alliances() {
            return false;
        }
        if !bordering_friends.is_empty() {
            // Computed once, it does not depend on which friend is looked at.
            let juiciest_ally = behavior
                .borrow_mut()
                .find_juiciest_ally(game, bordering_friends);
            for &friend in bordering_friends {
                if behavior.borrow_mut().maybe_betray(
                    game,
                    friend,
                    juiciest_ally,
                    bordering_friends,
                    bordering_enemies,
                ) {
                    return self.send_attack(game, friend, true);
                }
            }
        }
        false
    }

    fn is_bordering_nuked_territory(&self, game: &GameImpl) -> bool {
        if game.config().is_unit_disabled(UnitType::MissileSilo) {
            return false;
        }
        let mut nbuf = [0; 4];
        for tile in game.player(self.player).border_tiles().iter() {
            let n = game.map.neighbors4_nswe(tile, &mut nbuf);
            for &neighbor in &nbuf[..n] {
                if game.map.is_land(neighbor)
                    && !game.map.has_owner(neighbor)
                    && game.map.has_fallout(neighbor)
                {
                    return true;
                }
            }
        }
        false
    }

    // Someone not much stronger than us who is under big attack (50%+ of their troops incoming).
    fn find_victim(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
        bordering_enemies.iter().copied().find(|&enemy| {
            if self.is_ffa(game) && game.player(enemy).troops() > self.troops(game) * 1.2 {
                return false;
            }
            let total_incoming_troops = game
                .player(enemy)
                .incoming_attacks()
                .into_iter()
                .fold(0.0, |sum, a| sum + game.attack(a).troops());
            total_incoming_troops > game.player(enemy).troops() * 0.5
        })
    }

    // Very weak (under 15% of their max troops) enemies without many more troops than us.
    fn find_very_weak_enemy(
        &self,
        game: &GameImpl,
        bordering_enemies: &[SmallId],
    ) -> Option<SmallId> {
        bordering_enemies.iter().copied().find(|&enemy| {
            let enemy_max_troops = game.player(enemy).max_troops();
            game.player(enemy).troops() < enemy_max_troops * 0.15
                && (!self.is_ffa(game) || game.player(enemy).troops() < self.troops(game) * 1.2)
        })
    }

    // The juiciest enemy we could plausibly beat (troops at most 75% of ours).
    fn find_juicy_target(&self, game: &GameImpl, bordering_enemies: &[SmallId]) -> Option<SmallId> {
        let candidates: Vec<SmallId> = bordering_enemies
            .iter()
            .copied()
            .filter(|&enemy| game.player(enemy).troops() <= self.troops(game) * 0.75)
            .collect();
        find_juiciest_target(game, &candidates)
    }

    fn find_nearest_island_enemy(&mut self, game: &GameImpl) -> Option<SmallId> {
        if game.config().is_unit_disabled(UnitType::TransportShip) {
            return None;
        }
        if game.player(self.player).unit_count(UnitType::TransportShip)
            >= game.config().boat_max_number() as i64
        {
            return None;
        }
        let has_shore = game
            .player(self.player)
            .border_tiles()
            .iter()
            .any(|t| game.map.is_shore(t));
        if !has_shore {
            return None;
        }

        let filtered_players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                if p == self.player {
                    return false;
                }
                if self.is_friendly(game, p) {
                    return false;
                }
                // In FFA, no boats to players with more troops.
                !self.is_ffa(game) || game.player(p).troops() < self.troops(game)
            })
            .collect();
        if filtered_players.is_empty() {
            return None;
        }

        let player_center = self.player_center(game, self.player);
        let mut sorted_players: Vec<(SmallId, u32)> = filtered_players
            .into_iter()
            .map(|p| {
                let center = self.player_center(game, p);
                let a = game.tile_ref(player_center.x, player_center.y);
                let b = game.tile_ref(center.x, center.y);
                (p, game.manhattan_dist(a, b))
            })
            .collect();
        sorted_players.sort_by_key(|&(_, d)| d);

        let my_shore = self.shore_tiles(game, self.player);
        let mut reachable_players: Vec<SmallId> = Vec::new();
        for (p, _) in sorted_players {
            let their_shore = self.shore_tiles(game, p);
            let Some((_, y)) = closest_two_tiles(
                &game.map,
                my_shore.iter().copied(),
                their_shore.iter().copied(),
            ) else {
                continue;
            };
            if can_build_truthy(game, self.player, y) {
                reachable_players.push(p);
                // Up to two reachable candidates are enough.
                if reachable_players.len() >= 2 {
                    break;
                }
            }
        }
        if reachable_players.is_empty() {
            return None;
        }
        // 33% chance to pick the second-nearest player.
        if reachable_players.len() >= 2 && self.chance(3.0) {
            return Some(reachable_players[1]);
        }
        Some(reachable_players[0])
    }

    // In team games nations attack and boat into stronger enemies, relying on donations; in FFA
    // going after someone much stronger usually loses.
    fn is_ffa(&self, game: &GameImpl) -> bool {
        game.config().game_config().game_mode == GameMode::FFA
    }

    fn player_center(&self, game: &GameImpl, player: SmallId) -> crate::Game::Game::Cell {
        if let Some((min, max)) = game.player(player).largest_cluster_bounding_box {
            return bounding_box_center(min, max);
        }
        calculate_bounding_box_center(&game.map, game.player(player).border_tiles().iter())
            .expect("player has border tiles")
    }

    /// `attackRandomTarget()`.
    pub fn attack_random_target(&mut self, game: &mut GameImpl) {
        // Save up troops until the trigger ratio is reached.
        if !self.has_trigger_ratio_troops(game) {
            return;
        }
        // Retaliate against incoming attacks.
        if let Some(incoming) = self.find_incoming_attack_player(game)
            && self.send_attack(game, incoming, true)
        {
            return;
        }
        // Select a traitor as an enemy.
        if let Some(to_attack) = self.get_neighbor_traitor_to_attack(game)
            && self.chance(3.0)
            && self.send_attack(game, to_attack, false)
        {
            return;
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
            if self.send_attack(game, neighbor, false) {
                return;
            }
        }
    }

    /// `getNeighborTraitorToAttack()`.
    pub fn get_neighbor_traitor_to_attack(&mut self, game: &GameImpl) -> Option<SmallId> {
        if game.config().disable_alliances() {
            return None;
        }
        let traitors: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&n| n != 0 && !self.is_friendly(game, n) && game.player(n).is_traitor())
            .collect();
        if traitors.is_empty() {
            None
        } else {
            Some(*self.random.borrow_mut().rand_element(&traitors))
        }
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

    fn target_id(game: &GameImpl, target: SmallId) -> Option<String> {
        (target != 0).then(|| game.player(target).id().to_string())
    }

    /// `sendAttack(target, force)`: true when an attack or boat was launched.
    pub fn send_attack(&mut self, game: &mut GameImpl, target: SmallId, force: bool) -> bool {
        if !force && !self.should_attack(game, target) {
            return false;
        }
        if target != 0 {
            if game.player(self.player).shares_border_with(target) {
                self.send_land_attack(game, target)
            } else {
                self.send_boat_attack(game, target)
            }
        } else if self.has_land_border_with_terra_nullius(game) {
            // `sharesBorderWith(TerraNullius)` counts water as terra nullius, so land is checked.
            self.send_land_attack(game, target)
        } else {
            self.send_boat_attack_to_nearby_terra_nullius(game)
        }
    }

    fn has_land_border_with_terra_nullius(&self, game: &GameImpl) -> bool {
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
    fn shore_tiles(&self, game: &GameImpl, player: SmallId) -> Vec<TileRef> {
        game.player(player)
            .border_tiles()
            .iter()
            .filter(|&t| game.map.is_shore(t))
            .collect()
    }

    // Scans shore border tiles (every 10th) for unowned land 5 tiles away across water in each
    // cardinal direction and sends a transport ship to the first match.
    fn send_boat_attack_to_nearby_terra_nullius(&mut self, game: &mut GameImpl) -> bool {
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
                if !can_build_truthy(game, self.player, tile) {
                    continue;
                }
                let troops = if game.config().rules().has(Change::NationExpansionCap5290) {
                    (self.troops(game) / 5.0).min(self.troop_send_cap_for_expansion(game))
                } else {
                    self.troops(game) / 5.0
                };
                if troops < 1.0 {
                    return false;
                }
                game.add_execution(Box::new(TransportShipExecution::new(
                    self.player,
                    tile,
                    Some(troops),
                )));
                return true;
            }
            i += 10;
        }
        false
    }

    /// `shouldAttack(other)`.
    pub fn should_attack(&mut self, game: &GameImpl, other: SmallId) -> bool {
        // Always attack terra nullius, non-humans and traitors; bots and HvN games always attack.
        if other == 0
            || game.player(other).player_type() != PlayerType::Human
            || game.player(other).is_traitor()
            || game.player(self.player).player_type() == PlayerType::Bot
            || game.config().player_teams() == TeamCountConfig::HumansVsNations
        {
            return true;
        }
        // Lower difficulties are prevented from attacking humans.
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Easy && self.random.borrow_mut().next_int(0.0, 4.0) != 0.0 {
            return false;
        }
        if difficulty == Difficulty::Medium && self.chance(4.0) {
            return false;
        }
        true
    }

    // Hard and Impossible nations in FFA: true when `troops` is under 20% of the target's troops.
    fn is_attack_too_weak(&self, game: &GameImpl, troops: f64, target: SmallId) -> bool {
        if game.player(self.player).player_type() == PlayerType::Bot {
            return false;
        }
        if game.config().game_config().game_mode == GameMode::Team {
            return false;
        }
        // Nations under attack may retaliate freely.
        if !game.player(self.player).incoming_attacks().is_empty() {
            return false;
        }
        let difficulty = game.config().game_config().difficulty;
        (difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible)
            && troops < game.player(target).troops() * 0.2
    }

    // Hard and Impossible nations in FFA: the most troops that can be sent without dropping below
    // a fraction of the strongest non-allied, non-bot neighbour (Hard 75%, Impossible 90%).
    fn troop_send_cap(&self, game: &GameImpl) -> f64 {
        let me = game.player(self.player);
        if me.player_type() == PlayerType::Bot {
            return f64::INFINITY;
        }
        if game.config().game_config().game_mode == GameMode::Team {
            return f64::INFINITY;
        }
        let retain_fraction = match game.config().game_config().difficulty {
            Difficulty::Hard => 0.75,
            Difficulty::Impossible => 0.9,
            _ => return f64::INFINITY,
        };
        let mut max_neighbor_troops = 0.0;
        for n in me.nearby() {
            if n != 0
                && !self.is_friendly(game, n)
                && game.player(n).player_type() != PlayerType::Bot
                && game.player(n).troops() > max_neighbor_troops
            {
                max_neighbor_troops = game.player(n).troops();
            }
        }
        let mut cap = if max_neighbor_troops == 0.0 {
            f64::INFINITY
        } else {
            let min_retained = (max_neighbor_troops * retain_fraction).ceil();
            f64::max(0.0, me.troops() - min_retained)
        };
        // Nations under attack may retaliate with at least the incoming troops.
        let incoming = me.incoming_attacks();
        if !incoming.is_empty() {
            let total_incoming = incoming
                .into_iter()
                .fold(0.0, |sum, a| sum + game.attack(a).troops());
            cap = f64::max(cap, total_incoming);
        }
        cap
    }

    // Like `troop_send_cap`, but above 0: terra nullius cannot fight back, so it is throttled.
    fn troop_send_cap_for_expansion(&self, game: &GameImpl) -> f64 {
        let cap = self.troop_send_cap(game);
        if cap > 0.0 {
            return cap;
        }
        (self.troops(game) * 0.05).ceil()
    }

    fn calculate_attack_troops(
        &mut self,
        game: &mut GameImpl,
        target: SmallId,
        non_bot_troops: impl FnOnce(&Self, &GameImpl, f64) -> f64,
    ) -> Option<f64> {
        let max_troops = game.player(self.player).max_troops();
        let target_is_player = target != 0;
        let bot_with_structures = target_is_player
            && game.player(target).player_type() == PlayerType::Bot
            && Self::owns_structures(game, target);
        // Attacking a bot with structures uses the expand ratio: they must be recaptured fast.
        let use_reserve = target_is_player && !bot_with_structures;
        let reserve_ratio = if use_reserve {
            self.reserve_ratio
        } else {
            self.expand_ratio
        };
        let target_troops = max_troops * reserve_ratio;

        let is_bot_attack = target_is_player
            && game.player(target).player_type() == PlayerType::Bot
            && game.player(self.player).player_type() != PlayerType::Bot;
        let mut troops = if is_bot_attack {
            self.calculate_bot_attack_troops(
                game,
                target,
                self.troops(game) - target_troops - self.bot_attack_troops_sent,
            )
        } else {
            non_bot_troops(self, game, target_troops)
        };

        // Hard and Impossible: do not drop below the neighbour troop threshold.
        if target_is_player {
            troops = troops.min(self.troop_send_cap(game));
        } else if game.config().rules().has(Change::NationExpansionCap5290) {
            troops = troops.min(self.troop_send_cap_for_expansion(game));
        }
        if troops < 1.0 {
            return None;
        }
        if target_is_player && self.is_attack_too_weak(game, troops, target) {
            return None;
        }
        if target_is_player && game.player(self.player).player_type() == PlayerType::Nation {
            let emoji = self.emoji_behavior.clone().expect("not initialized");
            emoji.borrow_mut().maybe_send_attack_emoji(game, target);
        }
        // Only troops that will actually be sent, after the cap, are counted.
        if is_bot_attack && game.config().rules().has(Change::NationExpansionCap5290) {
            self.bot_attack_troops_sent += troops;
        }
        Some(troops)
    }

    fn send_land_attack(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        let Some(troops) = self.calculate_attack_troops(game, target, |this, g, target_troops| {
            this.troops(g) - target_troops
        }) else {
            return false;
        };
        let target_id = Self::target_id(game, target);
        game.add_execution(Box::new(AttackExecution::new(
            Some(troops),
            self.player,
            target_id,
            None,
            true,
        )));
        true
    }

    fn send_boat_attack(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
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
        if !can_build_truthy(game, self.player, closest_y) {
            return false;
        }
        let troops = if game.config().rules().has(Change::AiBoatTroopsUnified4297) {
            self.calculate_attack_troops(game, target, |this, g, _| this.troops(g) / 5.0)
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

    /// Boat troops before PR 4297: no reserve subtraction for bot targets and the cap always applies.
    fn legacy_boat_troops(&mut self, game: &mut GameImpl, target: SmallId) -> Option<f64> {
        let fifth = self.troops(game) / 5.0;
        let mut troops = if game.player(target).player_type() == PlayerType::Bot {
            self.calculate_bot_attack_troops(game, target, fifth)
        } else {
            fifth
        };
        troops = troops.min(self.troop_send_cap(game));
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

    fn calculate_bot_attack_troops(
        &mut self,
        game: &GameImpl,
        target: SmallId,
        max_troops: f64,
    ) -> f64 {
        let old_accounting = !game.config().rules().has(Change::NationExpansionCap5290);
        if game.config().game_config().difficulty == Difficulty::Easy {
            if old_accounting {
                self.bot_attack_troops_sent += max_troops;
            }
            return max_troops;
        }
        let target_troops = game.player(target).troops();
        let mut troops = target_troops * 4.0;
        // Keep the reserve: do not send more than `max_troops`.
        if troops > max_troops {
            // Too few troops left for a big enough bot attack: skip it.
            if max_troops < target_troops * 2.0 {
                troops = 0.0;
            } else {
                troops = max_troops;
            }
        }
        if old_accounting {
            self.bot_attack_troops_sent += troops;
        }
        troops
    }

    fn donate_troops(&mut self, game: &mut GameImpl) -> bool {
        // Only in team games.
        if game.config().game_config().game_mode != GameMode::Team {
            return false;
        }
        // Not in public games (to balance HvN).
        if game.config().game_config().game_type == GameType::Public {
            return false;
        }
        if !game.config().donate_troops() {
            return false;
        }
        if game.winner().is_some() {
            return false;
        }
        match game.config().game_config().difficulty {
            Difficulty::Easy => return false,
            Difficulty::Medium => {
                if !self.chance(4.0) {
                    return false;
                }
            }
            Difficulty::Hard => {
                if !self.chance(2.0) {
                    return false;
                }
            }
            Difficulty::Impossible => {}
        }

        // Teammates currently in combat.
        let teammates: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(self.player).is_on_same_team(p))
            .filter(|&p| {
                !game.player(p).incoming_attacks().is_empty()
                    || !game.player(p).outgoing_attacks().is_empty()
            })
            .collect();
        if teammates.is_empty() {
            return false;
        }

        // Lowest troop percentage first.
        let mut with_percentage: Vec<(SmallId, f64)> = teammates
            .into_iter()
            .map(|t| {
                let max_troops = game.player(t).max_troops();
                (t, game.player(t).troops() / f64::max(max_troops, 1.0))
            })
            .collect();
        sort_by_key_f64(&mut with_percentage, |&(_, p)| p);

        let selected = with_percentage
            .iter()
            .map(|&(t, _)| t)
            .find(|&t| game.player(self.player).can_donate_troops(t));
        let Some(selected) = selected else {
            return false;
        };

        // Donate a portion of our troops, keeping the reserve.
        let max_troops = game.player(self.player).max_troops();
        let troops_to_keep = max_troops * self.reserve_ratio;
        let available_troops = self.troops(game) - troops_to_keep;
        if available_troops < 1.0 {
            return false;
        }
        let selected_id = game.player(selected).id().to_string();
        game.add_execution(Box::new(DonateTroopsExecution::new(
            self.player,
            selected_id,
            Some(available_troops),
        )));
        true
    }
}
