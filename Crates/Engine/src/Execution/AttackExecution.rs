// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/AttackExecution.ts: one land or boat-landing attack.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Client messages and the stats that never reach the simulation are not ported. The attack's
//! own `PseudoRandom(123)` and the f32 heap priorities decide the conquest order exactly.

use crate::Builds::Rules::Change;
use crate::Compat::OrderedSet::OrderedSet;
use crate::Configuration::Config::{AttackLogicAttacker, AttackLogicDefender, AttackLogicInput};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::Execution::Execution;
use crate::Execution::Utils::FlatBinaryHeap::FlatBinaryHeap;
use crate::Game::Game::{
    AttackId, Difficulty, PlayerType, Relation, SmallId, TerrainType, Tick, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

const MALUS_FOR_RETREAT: f64 = 25.0;

/// An attack of `owner` on a player or terra nullius.
pub struct AttackExecution {
    active: bool,
    to_conquer: FlatBinaryHeap,
    random: PseudoRandom,
    target: SmallId,
    attack: Option<AttackId>,
    start_troops: Option<f64>,
    owner: SmallId,
    target_id: Option<String>,
    source_tile: Option<TileRef>,
    remove_troops: bool,
    /// Pre-PR 2044: the alliance with the target is broken on the first tick.
    break_alliance: bool,
    /// Pre-PR 2044: whether the owner was allied to the target at `init`.
    was_allied_at_init: bool,
}

impl AttackExecution {
    /// `new AttackExecution(startTroops, owner, targetID, sourceTile, removeTroops)`; a `None`
    /// target id is terra nullius.
    pub fn new(
        start_troops: Option<f64>,
        owner: SmallId,
        target_id: Option<String>,
        source_tile: Option<TileRef>,
        remove_troops: bool,
    ) -> Self {
        Self {
            active: true,
            to_conquer: FlatBinaryHeap::new(),
            random: PseudoRandom::new(123.0),
            target: 0,
            attack: None,
            start_troops,
            owner,
            target_id,
            source_tile,
            remove_troops,
            break_alliance: false,
            was_allied_at_init: false,
        }
    }

    /// `targetID()`.
    pub fn target_id(&self) -> Option<&str> {
        self.target_id.as_deref()
    }

    /// `owner()`.
    pub fn owner(&self) -> SmallId {
        self.owner
    }

    fn attack_id(&self) -> AttackId {
        self.attack.expect("Attack not initialized")
    }

    fn reject_incoming_alliance_requests(&self, game: &mut GameImpl, target: SmallId) {
        let request = game
            .player(self.owner)
            .incoming_alliance_requests()
            .into_iter()
            .find(|&r| game.alliance_request(r).requestor() == target);
        if let Some(r) = request {
            game.reject_request(r);
        }
    }

    fn refresh_to_conquer(&mut self, game: &mut GameImpl) {
        let attack = self.attack_id();
        self.to_conquer.clear();
        game.attack_mut(attack).clear_border();
        let borders: Vec<TileRef> = game.player(self.owner).border_tiles().iter().collect();
        for tile in borders {
            self.add_neighbors(game, tile);
        }
    }

    fn retreat(&mut self, game: &mut GameImpl, malus_percent: f64) {
        let attack = self.attack_id();
        let deaths = game.attack(attack).troops() * (malus_percent / 100.0);
        if !self.remove_troops
            && game.has(Change::AttackRetreatStartTroops26)
            && (self.source_tile.is_none() || !game.has(Change::AttackRetreatSourceTile27))
        {
            let troops = game.attack(attack).troops() - self.start_troops.unwrap_or(0.0);
            game.set_attack_troops(attack, troops);
        }
        let survivors = game.attack(attack).troops() - deaths;
        game.player_mut(self.owner).add_troops(survivors);
        game.delete_attack(attack);
        self.active = false;
    }

    /// Cardinal neighbours in the order the build's attack code walks them (`neighbors()` before PR 3820).
    fn neighbors_of(game: &GameImpl, tile: TileRef, out: &mut [TileRef; 4]) -> usize {
        if game.has(Change::AttackForEachNeighbor3820) {
            game.map.neighbors4(tile, out)
        } else {
            game.map.neighbors4_nswe(tile, out)
        }
    }

    fn add_neighbors(&mut self, game: &mut GameImpl, tile: TileRef) {
        let attack = self.attack_id();
        let tick_now = game.ticks() as f64;
        let mut nbuf = [0; 4];
        let mut nbuf2 = [0; 4];
        let num_neighbors = Self::neighbors_of(game, tile, &mut nbuf);
        for &neighbor in &nbuf[..num_neighbors] {
            if game.map.is_water(neighbor)
                || game.map.is_impassable(neighbor)
                || game.map.owner_id(neighbor) != self.target
            {
                continue;
            }
            game.attack_mut(attack).add_border_tile(neighbor);
            let mut num_owned_by_me = 0;
            let num_inner = Self::neighbors_of(game, neighbor, &mut nbuf2);
            for &inner in &nbuf2[..num_inner] {
                if game.map.owner_id(inner) == self.owner {
                    num_owned_by_me += 1;
                }
            }
            let mag = match game.map.terrain_type(neighbor) {
                TerrainType::Plains => 1.0,
                TerrainType::Highland => 1.5,
                TerrainType::Mountain => 2.0,
                _ => 0.0,
            };
            let priority = (self.random.next_int(0.0, 7.0) + 10.0)
                * (1.0 - num_owned_by_me as f64 * 0.5 + mag / 2.0)
                + tick_now;
            self.to_conquer.enqueue(neighbor, priority);
        }
    }

    fn attack_logic_input(
        &self,
        game: &GameImpl,
        attack_troops: f64,
        tile: TileRef,
        border_size: f64,
    ) -> AttackLogicInput {
        let owner = game.player(self.owner);
        let defender = (self.target != 0).then(|| game.player(self.target));
        let defender_has_defense_post = defender.is_some()
            && game.has_unit_nearby(
                tile,
                game.config().defense_post_range(),
                UnitType::DefensePost,
                Some(self.target),
                false,
            );
        AttackLogicInput {
            terrain: game.map.terrain_type(tile),
            attack_troops,
            attacker: AttackLogicAttacker {
                player_type: owner.player_type(),
                num_tiles: owner.num_tiles_owned() as f64,
            },
            defender: defender.map(|d| AttackLogicDefender {
                player_type: d.player_type(),
                num_tiles: d.num_tiles_owned() as f64,
                troops: d.troops(),
                is_traitor: d.is_traitor(),
                is_disconnected_teammate: d.is_disconnected() && owner.is_on_same_team(self.target),
            }),
            defender_has_defense_post,
            fallout_ratio: game
                .map
                .has_fallout(tile)
                .then(|| game.num_tiles_with_fallout() / game.num_land_tiles()),
            border_size,
        }
    }

    fn handle_dead_defender(&mut self, game: &mut GameImpl) {
        if !(self.target != 0 && game.player(self.target).num_tiles_owned() < 100) {
            return;
        }
        let target = self.target;
        game.conquer_player(self.owner, target);

        let converge = game.has(Change::AnnexConvergeLoop4890);
        let max_passes: usize = if converge { 100 } else { 10 };
        for _pass in 0..max_passes {
            let mut progressed = false;
            // `target.tiles()` is iterated while tiles move away; the cursor tolerates that.
            let mut cursor = game.player_impl_mut(target).tiles.pin_cursor();
            while let Some(tile) = game.player_impl(target).tiles().next(&mut cursor) {
                let mut borders = false;
                game.map.for_each_neighbor(tile, |t| {
                    if !borders && game.map.owner_id(t) == self.owner {
                        borders = true;
                    }
                });
                if borders {
                    game.player_mut(self.owner).conquer(tile);
                    progressed = true;
                } else {
                    let mut captured_by: Option<SmallId> = None;
                    let mut nbuf = [0; 4];
                    let count = Self::neighbors_of(game, tile, &mut nbuf);
                    nbuf[..count].iter().for_each(|&neighbor| {
                        if captured_by.is_some() {
                            return;
                        }
                        let no = game.map.owner_id(neighbor);
                        if no != 0
                            && no != target
                            && (!game.has(Change::AttackFriendlyNeighbor2954)
                                || !game.player(no).is_friendly(target, false))
                        {
                            captured_by = Some(no);
                        }
                    });
                    if let Some(no) = captured_by {
                        game.player_mut(no).conquer(tile);
                        progressed = true;
                    }
                }
            }
            game.player_impl_mut(target).tiles.unpin();
            if converge && !progressed {
                break;
            }
        }
    }
}

impl Execution for AttackExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !self.active {
            return;
        }
        self.random = PseudoRandom::for_rules(game.config().rules(), 123.0);
        if let Some(id) = &self.target_id
            && !game.has_player(id)
        {
            self.active = false;
            return;
        }
        self.target = match &self.target_id {
            None => 0,
            Some(id) => game.player_id(id),
        };

        if self.owner == self.target {
            self.active = false;
            return;
        }

        if self.target != 0
            && game.has(Change::AttackAllianceBlocked2044)
            && game.player(self.owner).is_friendly(self.target, false)
        {
            self.active = false;
            return;
        }

        if self.target != 0 {
            let target_type = game.player(self.target).player_type();
            let owner_type = game.player(self.owner).player_type();
            if target_type != PlayerType::Bot && owner_type != PlayerType::Bot {
                // Bots do not embargo since they cannot trade anyway.
                game.player_mut(self.target).add_embargo(self.owner, true);
                if game.has(Change::AttackRejectsAllianceRequests1733) {
                    self.reject_incoming_alliance_requests(game, self.target);
                }
            }
        }

        if self.target != 0 && game.has(Change::ImmunityByConfig28) {
            // Nobody may attack a player before the spawn phase and the fixed immunity are over
            let immune_until =
                game.config().num_spawn_phase_turns() + game.config().spawn_immunity_duration();
            if immune_until > game.ticks() {
                self.active = false;
                return;
            }
        } else if self.target != 0
            && !game
                .player(self.owner)
                .can_attack_player(self.target, false)
        {
            self.active = false;
            return;
        }
        if self.target != 0
            && !game.has(Change::AttackAllianceBlocked2044)
            && game.player(self.owner).is_on_same_team(self.target)
        {
            self.active = false;
            return;
        }

        let owner_troops = game.player(self.owner).troops();
        let mut start_troops = match self.start_troops {
            Some(t) => t,
            None => {
                let owner_type = game.player(self.owner).player_type();
                game.config().attack_amount(owner_type, owner_troops)
            }
        };
        if self.remove_troops {
            start_troops = owner_troops.min(start_troops);
            let deducted = game.player_mut(self.owner).remove_troops(start_troops);
            if game.has(Change::AttackTroopsDeducted5111) {
                start_troops = deducted;
            }
        }
        self.start_troops = Some(start_troops);
        let attack = game.player_mut(self.owner).create_attack(
            self.target,
            start_troops,
            self.source_tile,
            OrderedSet::new(),
        );
        self.attack = Some(attack);

        match self.source_tile {
            Some(tile) => self.add_neighbors(game, tile),
            None => self.refresh_to_conquer(game),
        }

        let client_id = game.player(self.owner).client_id().map(str::to_string);
        game.stats_mut().attack(client_id.as_deref(), start_troops);

        let incoming_list = game.player(self.owner).incoming_attacks();
        for incoming in incoming_list {
            if game.attack(incoming).attacker() == self.target {
                // The target has an opposing attack: cancel them out.
                if game.attack(incoming).troops() > game.attack(attack).troops() {
                    let left = game.attack(incoming).troops() - game.attack(attack).troops();
                    game.set_attack_troops(incoming, left);
                    game.delete_attack(attack);
                    self.active = false;
                    return;
                } else {
                    let left = game.attack(attack).troops() - game.attack(incoming).troops();
                    game.set_attack_troops(attack, left);
                    game.delete_attack(incoming);
                }
            }
        }
        let outgoing_list: Vec<AttackId> = game.player(self.owner).outgoing_attacks().to_vec();
        for outgoing in outgoing_list {
            if outgoing != attack
                && game.attack(outgoing).target() == game.attack(attack).target()
                // Boat attacks (with a source tile) are not combined with other attacks.
                && game.attack(attack).source_tile().is_none()
            {
                let sum = game.attack(attack).troops() + game.attack(outgoing).troops();
                game.set_attack_troops(attack, sum);
                game.delete_attack(outgoing);
            }
        }

        // Nothing to conquer: the attack does not happen.
        if game.has(Change::AttackRetreatNoTargets5823) && self.to_conquer.size() == 0 {
            self.retreat(game, 0.0);
            return;
        }

        if self.target != 0 && !game.has(Change::AttackAllianceBlocked2044) {
            // Remember the alliance so the first tick breaks it (the attacker becomes a traitor).
            self.was_allied_at_init = game.player(self.owner).is_allied_with(self.target);
            if self.was_allied_at_init {
                self.break_alliance = true;
            }
        }

        if self.target != 0 {
            let difficulty = game.config().game_config().difficulty;
            // Hitting back at whoever just attacked us does not anger them, except on Easy.
            if game.has(Change::NationsLikeHumans5899)
                && difficulty != Difficulty::Easy
                && game.player(self.owner).relation(self.target) == Relation::Hostile
            {
                return;
            }
            let relation_change = match game.config().game_config().difficulty {
                _ if game.has(Change::RelationPenalties28) => -80.0,
                Difficulty::Easy => -60.0,
                Difficulty::Medium => -70.0,
                Difficulty::Hard => -80.0,
                Difficulty::Impossible => -100.0,
            };
            game.player_mut(self.target)
                .update_relation(self.owner, relation_change);
        }
    }

    fn tick(&mut self, game: &mut GameImpl, _ticks: Tick) {
        let attack = self.attack_id();
        let mut troop_count = game.attack(attack).troops();
        let target_is_player = self.target != 0;

        // Must come before the retreat refund: another attack's init can absorb or cancel this one
        // on the tick its retreat completes.
        if game.has(Change::AttackInactiveBeforeRetreat6089) && !game.attack(attack).is_active() {
            self.active = false;
            return;
        }

        if game.attack(attack).retreated() {
            if target_is_player {
                self.retreat(game, MALUS_FOR_RETREAT);
            } else {
                self.retreat(game, 0.0);
            }
            self.active = false;
            return;
        }
        if game.attack(attack).retreating() {
            return;
        }
        if !game.attack(attack).is_active() {
            self.active = false;
            return;
        }
        if game.has(Change::AttackAllianceBlocked2044) {
            if target_is_player && game.player(self.owner).is_friendly(self.target, false) {
                // An alliance was created after the attack started.
                self.retreat(game, 0.0);
                return;
            }
        } else if target_is_player {
            let alliance = game.player(self.owner).alliance_with(self.target);
            if self.break_alliance
                && let Some(alliance) = alliance
            {
                self.break_alliance = false;
                game.player_mut(self.owner).break_alliance(alliance);
            }
            if game.player(self.owner).is_allied_with(self.target) && !self.was_allied_at_init {
                // A new alliance appeared after the attack started: retreat, not betray.
                self.retreat(game, 0.0);
                return;
            }
        }

        let border_size = game.attack(attack).border_size() + self.random.next_int(0.0, 5.0);
        let rework = game.has(Change::AttackLogicRework5173);
        // Each tile consumes a fraction of the tick; conquer until it is spent.
        let mut tick_budget = if rework {
            1.0
        } else {
            let defender_troops = target_is_player.then(|| game.player(self.target).troops());
            game.config()
                .attack_tiles_per_tick_legacy(troop_count, defender_troops, border_size)
        };

        while tick_budget > 0.0 {
            if troop_count < 1.0 {
                game.delete_attack(attack);
                self.active = false;
                return;
            }
            if self.to_conquer.size() == 0 {
                self.refresh_to_conquer(game);
                self.retreat(game, 0.0);
                return;
            }

            let tile_to_conquer = self.to_conquer.dequeue();
            game.attack_mut(attack).remove_border_tile(tile_to_conquer);

            let mut on_border = false;
            let mut nbuf = [0; 4];
            let num_neighbors = Self::neighbors_of(game, tile_to_conquer, &mut nbuf);
            for &n in &nbuf[..num_neighbors] {
                if game.map.owner_id(n) == self.owner {
                    on_border = true;
                    break;
                }
            }
            if game.map.owner_id(tile_to_conquer) != self.target || !on_border {
                continue;
            }
            if (!game.map.is_land(tile_to_conquer) && game.has(Change::AttackSkipsWater31))
                || game.map.is_impassable(tile_to_conquer)
            {
                continue;
            }
            self.add_neighbors(game, tile_to_conquer);
            let input = self.attack_logic_input(game, troop_count, tile_to_conquer, border_size);
            let (attacker_loss, defender_loss, spent) = if rework {
                let result = game.config().attack_logic(&input);
                (
                    result.attacker_troop_loss,
                    result.defender_troop_loss,
                    result.tick_fraction,
                )
            } else {
                let result = game.config().attack_logic_legacy(&input);
                (
                    result.attacker_troop_loss,
                    result.defender_troop_loss,
                    result.tiles_per_tick_used,
                )
            };
            tick_budget -= spent;
            troop_count -= attacker_loss;
            game.set_attack_troops(attack, troop_count);
            if target_is_player {
                game.player_mut(self.target).remove_troops(defender_loss);
            }
            game.player_mut(self.owner).conquer(tile_to_conquer);
            self.handle_dead_defender(game);
        }
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
