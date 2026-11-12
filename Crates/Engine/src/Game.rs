// SPDX-License-Identifier: AGPL-3.0-only
//! Game state types: terrain map, tile sets, shared enums and value types, and the game itself.

pub mod AllianceImpl;
pub mod AllianceRequestImpl;
pub mod AttackImpl;
#[allow(clippy::module_inception)]
pub mod Game;
pub mod GameImpl;
pub mod GameMap;
pub mod GameViews;
pub mod NationCreation;
pub mod PlayerImpl;
pub mod RailNetwork;
pub mod Railroad;
pub mod RailroadSpatialGrid;
pub mod StatsImpl;
pub mod TeamAssignment;
pub mod TerraNulliusImpl;
pub mod TileSet;
pub mod TrainStation;
pub mod TransportShipUtils;
pub mod UnitGrid;
pub mod UnitImpl;
pub mod Veterancy;
pub mod WaterManager;
