// SPDX-License-Identifier: AGPL-3.0-only
//! Pathfinding: priority queues, grid searches, the water hierarchy, path transformers and steppers.

pub mod Algorithms;
pub mod Legacy28;
pub mod Line;
pub mod LineV33;
pub mod PathFinder;
pub mod PathFinderAir;
pub mod PathFinderParabola;
pub mod PathFinderStation;
pub mod PathFinderStepper;
pub mod PathGameView;
pub mod Spatial;
pub mod Transformers;
pub mod Types;

#[cfg(test)]
mod GoldenTest;
#[cfg(test)]
mod Legacy28GoldenTest;
