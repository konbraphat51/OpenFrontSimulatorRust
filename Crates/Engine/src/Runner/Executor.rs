// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/ExecutionManager.ts: turns intents into executions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Executions that are not ported yet are created as [`NotPorted`], which panics with
//! `not ported: <Name>` the moment it is created. Replace the arm of the matching intent when an
//! execution lands. `Executor.random` is seeded but never drawn from upstream, so it is not kept.

use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Execution::Execution;
use crate::Execution::MarkDisconnectedExecution::MarkDisconnectedExecution;
use crate::Execution::NoOpExecution::NoOpExecution;
use crate::Execution::NotPorted::NotPorted;
use crate::Execution::PauseExecution::PauseExecution;
use crate::Execution::QuickChatExecution::QuickChatExecution;
use crate::Execution::SpawnExecution::SpawnExecution;
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
            Intent::Attack { .. } => NotPorted::at_creation("AttackExecution"),
            Intent::CancelAttack { .. } => NotPorted::at_creation("RetreatExecution"),
            Intent::CancelBoat { .. } => NotPorted::at_creation("BoatRetreatExecution"),
            Intent::MoveWarship { .. } => NotPorted::at_creation("MoveWarshipExecution"),
            Intent::Spawn { tile } => Box::new(SpawnExecution::new(
                &self.game_id,
                game.player(player).info().clone(),
                Some(*tile),
                true,
            )),
            Intent::Boat { .. } => NotPorted::at_creation("TransportShipExecution"),
            Intent::AllianceRequest { .. } => NotPorted::at_creation("AllianceRequestExecution"),
            Intent::AllianceReject { .. } => NotPorted::at_creation("AllianceRejectExecution"),
            Intent::BreakAlliance { .. } => NotPorted::at_creation("BreakAllianceExecution"),
            Intent::TargetPlayer { .. } => NotPorted::at_creation("TargetPlayerExecution"),
            Intent::Emoji { recipient, emoji } => Box::new(EmojiExecution::new(
                player,
                recipient.clone(),
                *emoji as f64,
            )),
            Intent::DonateTroops { .. } => NotPorted::at_creation("DonateTroopsExecution"),
            Intent::DonateGold { .. } => NotPorted::at_creation("DonateGoldExecution"),
            Intent::Embargo { .. } => NotPorted::at_creation("EmbargoExecution"),
            Intent::EmbargoAll { .. } => NotPorted::at_creation("EmbargoAllExecution"),
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
            Intent::AllianceExtension { .. } => {
                NotPorted::at_creation("AllianceExtensionExecution")
            }
            Intent::UpgradeStructure {
                unit_id, amount, ..
            } => Box::new(UpgradeStructureExecution::new(player, *unit_id, *amount)),
            Intent::DeleteUnit { .. } => NotPorted::at_creation("DeleteUnitExecution"),
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

    /// `nationExecutions()`: one `NationExecution` per nation (not ported: panics when initialised).
    pub fn nation_executions(&self, game: &GameImpl) -> Vec<Box<dyn Execution>> {
        game.nations()
            .iter()
            .map(|_| NotPorted::at_init("NationExecution", true))
            .collect()
    }
}
