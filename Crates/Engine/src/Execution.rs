// SPDX-License-Identifier: AGPL-3.0-only
//! The `Execution` trait and the ported executions (one file per upstream execution).

pub mod ConstructionExecution;
pub mod EmojiExecution;
#[allow(clippy::module_inception)]
pub mod Execution;
#[allow(clippy::upper_case_acronyms)]
pub mod MIRVExecution;
pub mod MarkDisconnectedExecution;
pub mod Nation;
pub mod NoOpExecution;
pub mod NotPorted;
pub mod NukeExecution;
pub mod PauseExecution;
pub mod PlayerExecution;
pub mod QuickChatExecution;
pub mod RecomputeRailClusterExecution;
pub mod SpawnExecution;
pub mod SpawnTimerExecution;
pub mod TribeSpawner;
pub mod UpgradeStructureExecution;
pub mod Util;
pub mod Utils;
pub mod WinCheckExecution;
