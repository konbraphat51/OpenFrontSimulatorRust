// SPDX-License-Identifier: AGPL-3.0-only
//! The `Execution` trait and the ported executions (one file per upstream execution).

pub mod Alliance;
pub mod AttackExecution;
pub mod BoatRetreatExecution;
pub mod ConstructionExecution;
pub mod EmojiExecution;
#[allow(clippy::module_inception)]
pub mod Execution;
pub mod MarkDisconnectedExecution;
pub mod MoveWarshipExecution;
pub mod Nation;
pub mod NationExecution;
pub mod NoOpExecution;
pub mod NotPorted;
pub mod PauseExecution;
pub mod PlayerExecution;
pub mod QuickChatExecution;
pub mod RecomputeRailClusterExecution;
pub mod ShellExecution;
pub mod SpawnExecution;
pub mod SpawnTimerExecution;
pub mod TradeShipExecution;
pub mod TransportShipExecution;
pub mod TribeSpawner;
pub mod Util;
pub mod Utils;
pub mod WarshipExecution;
pub mod WinCheckExecution;
