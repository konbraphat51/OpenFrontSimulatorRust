// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/types.ts: path results, the `PathFinder` trait and the search context.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream finders hold references to the map(s) they search; Rust finders receive them per call
//! through [`PathCtx`], because `GameImpl` owns the maps and mutates them between ticks.

use crate::Game::GameMap::{GameMapImpl, TileRef};

/// Upstream `PathStatus` (values 0, 2 and 3 are kept for traces).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathStatus {
    /// A next node was found.
    Next = 0,
    /// The destination is reached.
    Complete = 2,
    /// No path exists.
    NotFound = 3,
}

/// Upstream `PathResult<T>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathResult<T> {
    /// Step to this node next.
    Next(T),
    /// Arrived; the node is the destination (or the current node for distance exits).
    Complete(T),
    /// No path.
    NotFound,
}

impl<T> PathResult<T> {
    /// The status tag of this result.
    pub fn status(&self) -> PathStatus {
        match self {
            Self::Next(_) => PathStatus::Next,
            Self::Complete(_) => PathStatus::Complete,
            Self::NotFound => PathStatus::NotFound,
        }
    }
}

/// Upstream `T | T[]` argument: a lone source differs from a one-element array in several finders
/// (the hierarchical finder takes its multi-source branch for any array), so the distinction is kept.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source<'a, T = TileRef> {
    /// A plain value.
    Single(T),
    /// An array of values.
    Multi(&'a [T]),
}

impl<'a, T> Source<'a, T> {
    /// The sources as a slice (a lone value is a one-element slice).
    pub fn as_slice(&self) -> &[T] {
        match self {
            Self::Single(value) => std::slice::from_ref(value),
            Self::Multi(values) => values,
        }
    }

    /// True for the array form.
    pub fn is_multi(&self) -> bool {
        matches!(self, Self::Multi(_))
    }
}

/// The maps a tile path finder runs on: the game map and its 2x down-scaled mini map.
#[derive(Clone, Copy)]
pub struct PathCtx<'a> {
    /// Full-resolution game map.
    pub map: &'a GameMapImpl,
    /// Mini map; water algorithms search it.
    pub mini_map: &'a GameMapImpl,
}

/// Upstream `PathFinder<number>`: finds tile paths between tiles.
pub trait PathFinder {
    /// Path from `from` (any source) to `to`, or `None`. Water algorithms search `ctx.mini_map`.
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>>;
}
