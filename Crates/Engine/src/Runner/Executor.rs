// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ExecutionManager.ts: turns intents into executions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Executions that are not ported yet are created as [`NotPorted`], which panics with
//! `not ported: <Name>` the moment it is created. Replace the arm of the matching intent when an
//! execution lands. `Executor.random` is seeded but never drawn from upstream, so it is not kept.

use crate::Execution::Alliance::AllianceExtensionExecution::AllianceExtensionExecution;
use crate::Execution::Alliance::AllianceRejectExecution::AllianceRejectExecution;
use crate::Execution::Alliance::AllianceRequestExecution::AllianceRequestExecution;
use crate::Execution::Alliance::BreakAllianceExecution::BreakAllianceExecution;
use crate::Execution::AttackExecution::AttackExecution;
use crate::Execution::BoatRetreatExecution::BoatRetreatExecution;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::DeleteUnitExecution::DeleteUnitExecution;
use crate::Execution::DonateGoldExecution::DonateGoldExecution;
use crate::Execution::DonateTroopExecution::DonateTroopsExecution;
use crate::Execution::EmbargoAllExecution::EmbargoAllExecution;
use crate::Execution::EmbargoExecution::EmbargoExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::MarkDisconnectedExecution::MarkDisconnectedExecution;
use crate::Execution::MoveWarshipExecution::MoveWarshipExecution;
use crate::Execution::NationExecution::NationExecution;
use crate::Execution::NoOpExecution::NoOpExecution;
use crate::Execution::NotPorted::NotPorted;
use crate::Execution::PauseExecution::PauseExecution;
use crate::Execution::QuickChatExecution::QuickChatExecution;
use crate::Execution::RetreatExecution::RetreatExecution;
use crate::Execution::SpawnExecution::SpawnExecution;
use crate::Execution::TargetPlayerExecution::TargetPlayerExecution;
use crate::Execution::TransportShipExecution::TransportShipExecution;
use crate::Execution::TribeSpawner::TribeSpawner;
use crate::Execution::UpgradeStructureExecution::UpgradeStructureExecution;
use crate::Execution::Utils::PlayerSpawner::spawn_players;
use crate::Game::Game::{Cell, UnitType};
use crate::Game::GameImpl::GameImpl;
use ofsim_record::Intents::{Intent, StampedIntent};
use ofsim_record::Records::Turn;

/// Creates executions from turns and the runner's initial executions.
pub struct Executor {
    game_id: String,
    purchased_tribe_names: Vec<String>,
}

impl Executor {
    /// `new Executor(mg, gameID, clientID, purchasedTribeNames)`.
    pub fn new(game_id: String, purchased_tribe_names: Vec<String>) -> Self {
        Self {
            game_id,
            purchased_tribe_names,
        }
    }

    /// `createExecs(turn)`.
    pub fn create_execs(&self, game: &GameImpl, turn: &Turn) -> Vec<Box<dyn Execution>> {
        turn.intents
            .iter()
            .map(|i| self.create_exec(game, i))
            .collect()
    }

    /// `createExec(intent)`.
    pub fn create_exec(&self, game: &GameImpl, intent: &StampedIntent) -> Box<dyn Execution> {
        let Some(player) = game.player_by_client_id(&intent.client_id) else {
            return Box::new(NoOpExecution);
        };
        match &intent.intent {
            Intent::Attack { target_id, troops } => Box::new(AttackExecution::new(
                *troops,
                player,
                target_id.clone(),
                None,
                true,
            )),
            Intent::CancelAttack { attack_id } => {
                Box::new(RetreatExecution::new(player, attack_id.clone()))
            }
            Intent::CancelBoat { unit_id } => Box::new(BoatRetreatExecution::new(player, *unit_id)),
            Intent::MoveWarship { unit_ids, tile } => {
                Box::new(MoveWarshipExecution::new(player, unit_ids.clone(), *tile))
            }
            Intent::Spawn { tile } => Box::new(SpawnExecution::new(
                &self.game_id,
                game.player(player).info().clone(),
                Some(*tile),
                true,
            )),
            Intent::Boat { troops, dst } => {
                Box::new(TransportShipExecution::new(player, *dst, Some(*troops)))
            }
            Intent::AllianceRequest { recipient } => {
                Box::new(AllianceRequestExecution::new(player, recipient.clone()))
            }
            Intent::AllianceReject { requestor } => {
                Box::new(AllianceRejectExecution::new(requestor.clone(), player))
            }
            Intent::BreakAlliance { recipient } => {
                Box::new(BreakAllianceExecution::new(player, recipient.clone()))
            }
            Intent::TargetPlayer { target } => {
                Box::new(TargetPlayerExecution::new(player, target.clone()))
            }
            Intent::Emoji { recipient, emoji } => Box::new(EmojiExecution::new(
                player,
                recipient.clone(),
                *emoji as f64,
            )),
            Intent::DonateTroops { recipient, troops } => Box::new(DonateTroopsExecution::new(
                player,
                recipient.clone(),
                *troops,
            )),
            Intent::DonateGold { recipient, gold } => {
                Box::new(DonateGoldExecution::new(player, recipient.clone(), *gold))
            }
            Intent::Embargo { target_id, action } => Box::new(EmbargoExecution::new(
                player,
                target_id.clone(),
                action.clone(),
            )),
            Intent::EmbargoAll { action } => {
                Box::new(EmbargoAllExecution::new(player, action.clone()))
            }
            Intent::BuildUnit {
                unit,
                tile,
                rocket_direction_up,
                amount,
            } => Box::new(ConstructionExecution::new(
                player,
                UnitType::from_str(unit).unwrap_or_else(|| panic!("unknown unit type {unit}")),
                *tile,
                *rocket_direction_up,
                *amount,
            )),
            Intent::AllianceExtension { recipient } => {
                Box::new(AllianceExtensionExecution::new(player, recipient.clone()))
            }
            Intent::UpgradeStructure {
                unit_id, amount, ..
            } => Box::new(UpgradeStructureExecution::new(player, *unit_id, *amount)),
            Intent::DeleteUnit { unit_id } => Box::new(DeleteUnitExecution::new(player, *unit_id)),
            Intent::QuickChat { recipient, .. } => {
                Box::new(QuickChatExecution::new(player, recipient.clone()))
            }
            Intent::MarkDisconnected { is_disconnected } => {
                Box::new(MarkDisconnectedExecution::new(player, *is_disconnected))
            }
            Intent::TogglePause { paused } => Box::new(PauseExecution::new(player, *paused)),
            other => panic!("intent type {} not found", other.type_name()),
        }
    }

    /// `spawnTribes(numTribes)`.
    pub fn spawn_tribes(&self, game: &GameImpl, num_tribes: usize) -> Vec<SpawnExecution> {
        let nation_cells: Vec<Cell> = game.nations().iter().filter_map(|n| n.spawn_cell).collect();
        TribeSpawner::new(game, &self.game_id, &nation_cells)
            .spawn_tribes(num_tribes, &self.purchased_tribe_names)
    }

    /// `spawnPlayers()`.
    pub fn spawn_players(&self, game: &GameImpl) -> Vec<SpawnExecution> {
        spawn_players(game, &self.game_id)
    }

    /// `nationExecutions()`: one `NationExecution` per nation.
    pub fn nation_executions(&self, game: &GameImpl) -> Vec<Box<dyn Execution>> {
        game.nations()
            .iter()
            .map(|n| Box::new(NationExecution::new(&self.game_id, n.clone())) as Box<dyn Execution>)
            .collect()
    }
}
