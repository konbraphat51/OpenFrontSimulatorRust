// SPDX-License-Identifier: AGPL-3.0-only
//! The `Execution` trait and the ported executions (one file per upstream execution).

pub mod CityExecution;
pub mod ConstructionExecution;
pub mod DefensePostExecution;
pub mod EmojiExecution;
#[allow(clippy::module_inception)]
pub mod Execution;
pub mod FactoryExecution;
pub mod MIRVExecution;
pub mod MarkDisconnectedExecution;
pub mod MissileSiloExecution;
pub mod Nation;
pub mod NoOpExecution;
pub mod NotPorted;
pub mod NukeExecution;
pub mod PauseExecution;
pub mod PlayerExecution;
pub mod PortExecution;
pub mod QuickChatExecution;
pub mod RecomputeRailClusterExecution;
pub mod SAMLauncherExecution;
pub mod ShellExecution;
pub mod SpawnExecution;
pub mod SpawnTimerExecution;
pub mod TradeShipExecution;
pub mod TrainExecution;
pub mod TrainStationExecution;
pub mod TribeSpawner;
pub mod UpgradeStructureExecution;
pub mod Util;
pub mod Utils;
pub mod WarshipExecution;
pub mod WinCheckExecution;
