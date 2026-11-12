// SPDX-License-Identifier: AGPL-3.0-only
//! The `Execution` trait and the ported executions (one file per upstream execution).

pub mod Alliance;
pub mod AttackExecution;
pub mod DeleteUnitExecution;
pub mod DonateGoldExecution;
pub mod DonateTroopExecution;
pub mod EmbargoAllExecution;
pub mod EmbargoExecution;
pub mod EmojiExecution;
#[allow(clippy::module_inception)]
pub mod Execution;
pub mod MarkDisconnectedExecution;
pub mod Nation;
pub mod NoOpExecution;
pub mod NotPorted;
pub mod PauseExecution;
pub mod PlayerExecution;
pub mod QuickChatExecution;
pub mod RecomputeRailClusterExecution;
pub mod RetreatExecution;
pub mod SpawnExecution;
pub mod SpawnTimerExecution;
pub mod TargetPlayerExecution;
pub mod TransportShipExecution;
pub mod TribeExecution;
pub mod TribeSpawner;
pub mod Util;
pub mod Utils;
pub mod WinCheckExecution;
