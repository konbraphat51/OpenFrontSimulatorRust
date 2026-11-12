// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/NationExecution.ts: the AI player of a map nation.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::SpawnExecution::SpawnExecution;
use crate::Builds::Rules::{Change, Rules};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use crate::Execution::Execution::Execution;
use crate::Execution::Nation::NationAllianceBehavior::NationAllianceBehavior;
use crate::Execution::Nation::NationEmojiBehavior::NationEmojiBehavior;
use crate::Execution::Nation::NationMIRVBehavior::NationMIRVBehavior;
use crate::Execution::Nation::NationNukeBehavior::NationNukeBehavior;
use crate::Execution::Nation::NationStructureBehavior::NationStructureBehavior;
use crate::Execution::Nation::NationWarshipBehavior::NationWarshipBehavior;
use crate::Execution::Nation::TutorialNation::{
    tutorial_collapse, tutorial_silo, tutorial_warship,
};
use crate::Execution::Nation::{Shared, SharedRandom};
use crate::Execution::Utils::AiAttackBehavior::AiAttackBehavior;
use crate::Game::Game::{
    Difficulty, GameMode, Nation, PlayerType, Relation, SmallId, TerrainType, Tick, UnitType,
};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// The behaviours a nation creates once it has landed on the map.
struct Behaviors {
    emoji: Shared<NationEmojiBehavior>,
    mirv: NationMIRVBehavior,
    attack: Shared<AiAttackBehavior>,
    alliance: Shared<NationAllianceBehavior>,
    warship: NationWarshipBehavior,
    nuke: NationNukeBehavior,
    structure: NationStructureBehavior,
}

/// Drives one nation: spawning, then periodic decisions through its behaviours.
pub struct NationExecution {
    active: bool,
    random: SharedRandom,
    behaviors: Option<Behaviors>,
    spawn_exec_added: bool,
    player: Option<SmallId>,
    attack_rate: u32,
    attack_tick: u32,
    trigger_ratio: f64,
    reserve_ratio: f64,
    expand_ratio: f64,
    embargo_malus_applied: HashSet<SmallId>,
    game_id: String,
    nation: Nation,
    rules: Rules,
}

impl NationExecution {
    /// `new NationExecution(gameID, nation)`.
    pub fn new(game_id: &str, nation: Nation, rules: &Rules) -> Self {
        let seed = simple_hash(&nation.player_info.id) as f64 + simple_hash(game_id) as f64;
        let mut random = PseudoRandom::for_rules(rules, seed);
        let trigger_ratio = random.next_int(50.0, 60.0) / 100.0;
        let reserve_ratio = random.next_int(30.0, 40.0) / 100.0;
        let expand_ratio = random.next_int(10.0, 20.0) / 100.0;
        Self {
            active: true,
            random: Rc::new(RefCell::new(random)),
            behaviors: None,
            spawn_exec_added: false,
            player: None,
            attack_rate: 0,
            attack_tick: 0,
            trigger_ratio,
            reserve_ratio,
            expand_ratio,
            embargo_malus_applied: HashSet::new(),
            game_id: game_id.to_string(),
            nation,
            rules: rules.clone(),
        }
    }

    fn get_attack_rate(&self, game: &GameImpl) -> u32 {
        let mut random = self.random.borrow_mut();
        let (lo, hi) = match game.config().game_config().difficulty {
            Difficulty::Easy if game.config().rules().has(Change::NationExecutionTweaks31) => {
                (65.0, 100.0)
            }
            Difficulty::Easy => (65.0, 80.0),
            Difficulty::Medium => (55.0, 70.0),
            Difficulty::Hard => (45.0, 60.0),
            Difficulty::Impossible => (30.0, 50.0),
        };
        random.next_int(lo, hi) as u32
    }

    fn spawn_exec(&self, tile: Option<TileRef>) -> Box<dyn Execution> {
        Box::new(SpawnExecution::new(
            &self.game_id,
            self.nation.player_info.clone(),
            tile,
            false,
            &self.rules,
        ))
    }

    fn initialize_behaviors(&mut self, game: &GameImpl) {
        let player = self.player.expect("Player not initialized");
        let random = &self.random;
        let emoji = Rc::new(RefCell::new(NationEmojiBehavior::new(
            random.clone(),
            game,
            player,
        )));
        let mirv = NationMIRVBehavior::new(random.clone(), game, player, emoji.clone());
        let alliance = Rc::new(RefCell::new(NationAllianceBehavior::new(
            random.clone(),
            game,
            player,
            emoji.clone(),
        )));
        let warship = NationWarshipBehavior::new(random.clone(), game, player, emoji.clone());
        let attack = Rc::new(RefCell::new(AiAttackBehavior::new(
            random.clone(),
            player,
            self.trigger_ratio,
            self.reserve_ratio,
            self.expand_ratio,
            Some(alliance.clone()),
            Some(emoji.clone()),
        )));
        let nuke =
            NationNukeBehavior::new(random.clone(), game, player, attack.clone(), emoji.clone());
        let structure = NationStructureBehavior::new(random.clone(), game, player);
        self.behaviors = Some(Behaviors {
            emoji,
            mirv,
            attack,
            alliance,
            warship,
            nuke,
            structure,
        });
    }

    fn random_spawn_land(&self, game: &GameImpl) -> Option<TileRef> {
        let cell = self.nation.spawn_cell.expect("not initialized");
        let delta = 25;
        let mut random = self.random.borrow_mut();
        let mut tries = 0;
        while tries < 50 {
            tries += 1;
            let x = random.next_int((cell.x - delta) as f64, (cell.x + delta) as f64) as i32;
            let y = random.next_int((cell.y - delta) as f64, (cell.y + delta) as f64) as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let tile = game.tile_ref(x, y);
            if game.is_land(tile) && !game.has_owner(tile) && !game.map.is_impassable(tile) {
                if game.map.terrain_type(tile) == TerrainType::Mountain && random.chance(2.0) {
                    continue;
                }
                return Some(tile);
            }
        }
        None
    }

    fn update_relations_from_embargos(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        for other in others {
            let embargo_malus = -20.0;
            let has = if game.config().rules().has(Change::NationsLikeHumans5899) {
                game.player(other).has_manual_embargo_against(player)
            } else {
                game.player(other).has_embargo_against(player)
            };
            let applied = self.embargo_malus_applied.contains(&other);
            if has && !applied {
                game.player_mut(player)
                    .update_relation(other, embargo_malus);
                self.embargo_malus_applied.insert(other);
            } else if !has && applied {
                game.player_mut(player)
                    .update_relation(other, -embargo_malus);
                self.embargo_malus_applied.remove(&other);
            }
        }
    }

    #[allow(clippy::if_same_then_else)]
    fn handle_embargoes_to_hostile_nations(&mut self, game: &mut GameImpl) {
        let Some(player) = self.player else { return };
        let others: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| p != player)
            .collect();
        let difficulty = game.config().game_config().difficulty;
        let is_higher_difficulty =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;
        let team_game = game.config().game_config().game_mode == GameMode::Team
            && game.config().rules().has(Change::NationExecutionTweaks31);

        for other in others {
            if team_game
                && is_higher_difficulty
                && game.player(other).player_type() != PlayerType::Bot
                && !game.player(player).is_on_same_team(other)
            {
                if !game.player(player).has_embargo_against(other) {
                    game.player_mut(player).add_embargo(other, false);
                }
                continue;
            }

            let relation = game.player(player).relation(other);
            let has_embargo = game.player(player).has_embargo_against(other);
            if relation <= Relation::Hostile
                && !has_embargo
                && !game.player(player).is_on_same_team(other)
            {
                game.player_mut(player).add_embargo(other, false);
            } else if relation >= Relation::Neutral
                && has_embargo
                && difficulty != Difficulty::Hard
                && difficulty != Difficulty::Impossible
            {
                game.player_mut(player).stop_embargo(other);
            } else if relation >= Relation::Friendly
                && has_embargo
                && difficulty != Difficulty::Impossible
            {
                game.player_mut(player).stop_embargo(other);
            }
        }
    }
}

impl Execution for NationExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        self.attack_rate = self.get_attack_rate(game);
        self.attack_tick = self
            .random
            .borrow_mut()
            .next_int(0.0, self.attack_rate as f64) as u32;

        let id = &self.nation.player_info.id;
        self.player = Some(if !game.has_player(id) {
            game.add_player(self.nation.player_info.clone(), None)
        } else {
            game.player_id(id)
        });
    }

    fn tick(&mut self, game: &mut GameImpl, ticks: Tick) {
        // Ship tracking
        if let (Some(b), Some(player)) = (self.behaviors.as_mut(), self.player)
            && game.player(player).is_alive()
            && game.config().game_config().difficulty != Difficulty::Easy
            && !(game.config().rules().has(Change::TutorialMode6204) && game.config().tutorial())
            && (game.player(player).units_constructed(UnitType::Port) != 0
                || !game.config().rules().has(Change::NationExecutionTweaks31))
            && !game.config().is_unit_disabled(UnitType::Warship)
        {
            b.warship.track_ships_and_retaliate(game);
        }

        let Some(player) = self.player else { return };

        let tracks_spawn = game.config().rules().has(Change::NationSpawnRetry4117);
        if game.in_spawn_phase() {
            if !tracks_spawn {
                if ticks % self.attack_rate != self.attack_tick {
                    return;
                }
            } else if game.player(player).has_spawned() {
                // Re-spawn periodically so the nation hops during the spawn phase.
                if ticks % self.attack_rate != self.attack_tick {
                    return;
                }
            } else if self.spawn_exec_added {
                return;
            }
            // Nations without a spawn cell are placed randomly by SpawnExecution.
            let Some(cell) = self.nation.spawn_cell else {
                game.add_execution(self.spawn_exec(None));
                self.spawn_exec_added = true;
                return;
            };

            let team = game.player(player).team().cloned();
            if let Some(team) = team
                && let Some(area) = game.team_spawn_area(&team)
            {
                let (cx, cy) = (cell.x as i64, cell.y as i64);
                let in_area = cx >= area.x
                    && cx < area.x + area.width
                    && cy >= area.y
                    && cy < area.y + area.height;
                if !in_area {
                    game.add_execution(self.spawn_exec(None));
                    self.spawn_exec_added = true;
                    return;
                }
            }

            let Some(rl) = self.random_spawn_land(game) else {
                return;
            };
            game.add_execution(self.spawn_exec(Some(rl)));
            self.spawn_exec_added = true;
            return;
        }

        // Spawn phase ended but our SpawnExecution has not fired yet.
        if tracks_spawn && self.spawn_exec_added && !game.player(player).has_spawned() {
            return;
        }

        if !game.player(player).is_alive() {
            self.active = false;
            return;
        }

        if self.behaviors.is_none() {
            self.initialize_behaviors(game);
            let attack = self.behaviors.as_ref().expect("initialized").attack.clone();
            attack.borrow_mut().force_send_attack(game, 0);
            return;
        }

        // The tutorial's nations only grow into free land (and, from v0.35.9, script a silo and a warship).
        if game.config().rules().has(Change::TutorialMode6204) && game.config().tutorial() {
            let attack = self.behaviors.as_ref().expect("initialized").attack.clone();
            let attack_tick = ticks % self.attack_rate == self.attack_tick;
            if game.config().rules().has(Change::TutorialSilo6242) {
                tutorial_collapse(game, player);
                if !game.player(player).is_alive() {
                    return;
                }
                if attack_tick {
                    attack.borrow_mut().expand_into_free_land(game);
                }
                tutorial_silo(game, player, attack_tick);
                tutorial_warship(game, player, ticks);
            } else if attack_tick {
                attack.borrow_mut().expand_into_free_land(game);
            }
            return;
        }
        let attack_now = self.behaviors.as_ref().expect("initialized").attack.clone();
        attack_now.borrow_mut().follow_up_landings(game);
        self.behaviors
            .as_mut()
            .expect("initialized")
            .nuke
            .follow_up_strikes(game);
        if ticks % self.attack_rate != self.attack_tick {
            // Call handleStructures at 1/3 and 2/3 of the interval between attack ticks.
            if game.player(player).is_alive() {
                let offset = ticks % self.attack_rate;
                let one_third = (self.attack_tick + self.attack_rate / 3) % self.attack_rate;
                let two_thirds = (self.attack_tick + (self.attack_rate * 2) / 3) % self.attack_rate;
                if offset == one_third || offset == two_thirds {
                    self.behaviors
                        .as_mut()
                        .expect("initialized")
                        .structure
                        .handle_structures(game);
                }
            }
            return;
        }

        let emoji = self.behaviors.as_ref().expect("initialized").emoji.clone();
        let alliance = self
            .behaviors
            .as_ref()
            .expect("initialized")
            .alliance
            .clone();
        let attack = self.behaviors.as_ref().expect("initialized").attack.clone();

        emoji.borrow_mut().maybe_send_casual_emoji(game);
        self.update_relations_from_embargos(game);
        alliance.borrow_mut().handle_alliance_requests(game);
        alliance
            .borrow_mut()
            .handle_alliance_extension_requests(game);
        let b = self.behaviors.as_mut().expect("initialized");
        b.mirv.consider_mirv(game);
        b.structure.handle_structures(game);
        b.warship.maybe_spawn_warship(game);
        self.handle_embargoes_to_hostile_nations(game);
        let blocker = attack.borrow_mut().maybe_attack(game);
        let b = self.behaviors.as_mut().expect("initialized");
        if let Some(blocker) = blocker {
            b.warship.clear_sea_lane(game, blocker);
        }
        b.warship.counter_warship_infestation(game);
        b.nuke.maybe_send_nuke(game);
    }

    fn is_active(&self) -> bool {
        self.active
    }

    fn active_during_spawn_phase(&self) -> bool {
        true
    }
}
