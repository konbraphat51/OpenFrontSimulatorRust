// SPDX-License-Identifier: AGPL-3.0-only
//! Narrow interfaces through which the N2 behaviours talk to the emoji and attack behaviours of N1 and A2.
//!
//! The upstream classes are shared objects; here the N2 behaviours hold them as `Rc<RefCell<dyn ..>>`,
//! so the real behaviours and the test doubles of the unit tests plug in the same way.

use super::NationEmojiBehavior::{NationEmojiBehavior, emoji_nuke, respond_to_mirv};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::MIRVExecution::MirvExecution;
use crate::Execution::NukeExecution::NukeExecution;
use crate::Execution::UpgradeStructureExecution::UpgradeStructureExecution;
use crate::Execution::Utils::AiAttackBehavior::AiAttackBehavior;
use crate::Game::Game::{EmojiRecipient, SmallId, UnitId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// What the N2 behaviours use of `NationEmojiBehavior` (and `respondToMIRV`).
pub trait EmojiPort {
    /// `maybeSendAttackEmoji(enemy)`.
    fn maybe_send_attack_emoji(&mut self, game: &mut GameImpl, enemy: SmallId);
    /// `maybeSendEmoji(target, EMOJI_NUKE)`.
    fn maybe_send_nuke_emoji(&mut self, game: &mut GameImpl, target: SmallId);
    /// `sendEmoji(AllPlayers, EMOJI_NUKE)`.
    fn send_nuke_emoji_to_all(&mut self, game: &mut GameImpl);
    /// `respondToMIRV(game, random, enemy)`.
    fn respond_to_mirv(&mut self, game: &mut GameImpl, random: &mut PseudoRandom, enemy: SmallId);
}

impl EmojiPort for NationEmojiBehavior {
    fn maybe_send_attack_emoji(&mut self, game: &mut GameImpl, enemy: SmallId) {
        NationEmojiBehavior::maybe_send_attack_emoji(self, game, enemy);
    }
    fn maybe_send_nuke_emoji(&mut self, game: &mut GameImpl, target: SmallId) {
        self.maybe_send_emoji(game, EmojiRecipient::Player(target), &emoji_nuke());
    }
    fn send_nuke_emoji_to_all(&mut self, game: &mut GameImpl) {
        self.send_emoji(game, EmojiRecipient::All, &emoji_nuke());
    }
    fn respond_to_mirv(&mut self, game: &mut GameImpl, random: &mut PseudoRandom, enemy: SmallId) {
        respond_to_mirv(game, random, enemy);
    }
}

/// What `NationNukeBehavior` uses of `AiAttackBehavior`.
pub trait AttackPort {
    /// `shouldAttack(other)`; 0 is terra nullius.
    fn should_attack(&mut self, game: &mut GameImpl, other: SmallId) -> bool;
    /// `findIncomingAttackPlayer()`.
    fn find_incoming_attack_player(&mut self, game: &mut GameImpl) -> Option<SmallId>;
    /// `strikeTroops(target)`.
    fn strike_troops(&self, game: &GameImpl, target: SmallId) -> f64;
    /// `strike(target)`.
    fn strike(&mut self, game: &mut GameImpl, target: SmallId) -> bool;
}

impl AttackPort for AiAttackBehavior {
    fn should_attack(&mut self, game: &mut GameImpl, other: SmallId) -> bool {
        AiAttackBehavior::should_attack(self, game, other)
    }
    fn find_incoming_attack_player(&mut self, game: &mut GameImpl) -> Option<SmallId> {
        AiAttackBehavior::find_incoming_attack_player(self, game)
    }
    fn strike_troops(&self, game: &GameImpl, target: SmallId) -> f64 {
        AiAttackBehavior::strike_troops(self, game, target)
    }
    fn strike(&mut self, game: &mut GameImpl, target: SmallId) -> bool {
        AiAttackBehavior::strike(self, game, target)
    }
}

/// What a behaviour decided to queue; [`DecisionLog`] records these for tests instead of queueing.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// `new ConstructionExecution(player, type, tile)`.
    Construct {
        /// Structure type.
        unit_type: UnitType,
        /// Tile to build on.
        tile: TileRef,
    },
    /// `new UpgradeStructureExecution(player, unitId)`.
    Upgrade {
        /// Unit to upgrade.
        unit: UnitId,
        /// Levels to buy.
        amount: u32,
    },
    /// `new NukeExecution(type, player, tile, null, -1, waitTicks)`.
    Nuke {
        /// Atom or hydrogen bomb.
        nuke_type: UnitType,
        /// Target tile.
        tile: TileRef,
        /// Launch delay in ticks.
        wait_ticks: f64,
    },
    /// `new MirvExecution(player, tile)`.
    Mirv {
        /// Target tile.
        tile: TileRef,
    },
}

/// Test hook of a behaviour: in dry-run mode decisions are recorded instead of queued as executions.
#[derive(Default)]
pub struct DecisionLog {
    dry_run: bool,
    entries: Vec<Decision>,
}

impl DecisionLog {
    /// Switches dry-run mode on or off.
    pub fn set_dry_run(&mut self, dry_run: bool) {
        self.dry_run = dry_run;
    }

    /// Returns and clears the recorded decisions.
    pub fn take(&mut self) -> Vec<Decision> {
        std::mem::take(&mut self.entries)
    }

    /// Queues the execution of `decision` for `player`, or records it in dry-run mode.
    pub fn queue(&mut self, game: &mut GameImpl, player: SmallId, decision: Decision) {
        if self.dry_run {
            self.entries.push(decision);
            return;
        }
        let exec: Box<dyn Execution> = match decision {
            Decision::Construct { unit_type, tile } => Box::new(ConstructionExecution::new(
                player, unit_type, tile, None, None,
            )),
            Decision::Upgrade { unit, amount } => {
                Box::new(UpgradeStructureExecution::new(player, unit, Some(amount)))
            }
            Decision::Nuke {
                nuke_type,
                tile,
                wait_ticks,
            } => Box::new(NukeExecution::new(
                nuke_type,
                player,
                tile,
                None,
                -1.0,
                wait_ticks,
                Some(true),
            )),
            Decision::Mirv { tile } => Box::new(MirvExecution::new(player, tile)),
        };
        game.add_execution(exec);
    }
}
