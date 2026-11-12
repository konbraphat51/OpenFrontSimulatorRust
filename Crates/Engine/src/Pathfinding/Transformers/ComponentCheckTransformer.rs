// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/transformers/ComponentCheckTransformer.ts: fails fast when no source shares the destination's water component.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Game::GameMap::TileRef;
use crate::Pathfinding::Algorithms::AbstractGraph::AbstractGraph;
use crate::Pathfinding::Types::{PathCtx, PathFinder, Source};
use std::rc::Rc;

/// Skips the inner search when source and destination are in different water components.
pub struct ComponentCheckTransformer<P> {
    inner: P,
    graph: Rc<AbstractGraph>,
}

impl<P> ComponentCheckTransformer<P> {
    /// Wraps `inner`; components are read from `graph`'s shared labeling.
    pub fn new(inner: P, graph: Rc<AbstractGraph>) -> Self {
        Self { inner, graph }
    }

    /// The wrapped finder.
    pub fn inner_mut(&mut self) -> &mut P {
        &mut self.inner
    }

    /// Replaces the graph used for component lookups.
    pub fn set_graph(&mut self, graph: Rc<AbstractGraph>) {
        self.graph = graph;
    }
}

impl<P: PathFinder> PathFinder for ComponentCheckTransformer<P> {
    fn find_path(&mut self, ctx: &PathCtx, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        let to_component = self.graph.get_component_id(to);

        // At least one source must match the destination component
        let valid_sources: Vec<TileRef> = from
            .as_slice()
            .iter()
            .copied()
            .filter(|&f| self.graph.get_component_id(f) == to_component)
            .collect();

        if valid_sources.is_empty() {
            return None;
        }

        if valid_sources.len() == 1 {
            self.inner
                .find_path(ctx, Source::Single(valid_sources[0]), to)
        } else {
            self.inner.find_path(ctx, Source::Multi(&valid_sources), to)
        }
    }
}
