// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/SerialAStar.ts: bidirectional A* over a graph adapter, resumable across ticks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The adapter is passed on every call instead of being owned, because the maps it reads belong to the game.

use super::FastPriorityQueue::{Entry, FastPriorityQueue};
use crate::Compat::OrderedMap::FxHashMap;
use std::collections::hash_map::Entry as MapEntry;

/// `prev` of a search root (it has a g score but no `cameFrom`).
const NO_PREV: u32 = u32::MAX;

/// Best known cost to a node and the node before it (`gScore` and `cameFrom` of upstream, one map each way).
#[derive(Clone, Copy, Debug)]
struct Reached {
    g_score: f64,
    prev: u32,
}

/// Upstream `PathFindResultType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PathFindResultType {
    /// The stepper has a next tile.
    NextTile,
    /// The search needs more ticks.
    Pending,
    /// The search or the walk is complete.
    Completed,
    /// No path exists.
    PathNotFound,
}

/// Upstream `GraphAdapter`: the graph the search runs on (nodes are tile refs or station ids).
pub trait GraphAdapter {
    /// Appends the neighbours of a node to `out`, in upstream's order.
    fn neighbors(&self, node: u32, out: &mut Vec<u32>);
    /// Cost of entering a node.
    fn cost(&self, node: u32) -> f64;
    /// Position used by the heuristic.
    fn position(&self, node: u32) -> (f64, f64);
    /// Whether the step `from` to `to` may be taken.
    fn is_traversable(&self, from: u32, to: u32) -> bool;
}

/// Bidirectional A* (`SerialAStar`).
#[derive(Clone, Debug)]
pub struct SerialAStar {
    fwd_open_set: FastPriorityQueue,
    bwd_open_set: FastPriorityQueue,
    fwd_reached: FxHashMap<u32, Reached>,
    bwd_reached: FxHashMap<u32, Reached>,
    neighbor_buffer: Vec<u32>,
    meeting_point: Option<u32>,
    completed: bool,
    sources: Vec<u32>,
    closest_source: u32,
    dst: u32,
    iterations: f64,
    max_tries: f64,
    direction_change_penalty: f64,
}

fn heuristic(graph: &impl GraphAdapter, a: u32, b: u32) -> f64 {
    let (ax, ay) = graph.position(a);
    let (bx, by) = graph.position(b);
    2.0 * ((ax - bx).abs() + (ay - by).abs())
}

fn find_closest_source(graph: &impl GraphAdapter, sources: &[u32], tile: u32) -> u32 {
    let mut closest = sources[0];
    for &source in &sources[1..] {
        if heuristic(graph, tile, source) < heuristic(graph, tile, closest) {
            closest = source;
        }
    }
    closest
}

impl SerialAStar {
    /// `new SerialAStar(src, dst, iterations, maxTries, graph, directionChangePenalty)`.
    pub fn new(
        sources: &[u32],
        dst: u32,
        iterations: f64,
        max_tries: f64,
        graph: &impl GraphAdapter,
        direction_change_penalty: f64,
    ) -> Self {
        let closest_source = find_closest_source(graph, sources, dst);
        let mut fwd_open_set = FastPriorityQueue::new();
        let mut fwd_reached = FxHashMap::default();
        for &start in sources {
            fwd_reached.insert(
                start,
                Reached {
                    g_score: 0.0,
                    prev: NO_PREV,
                },
            );
            fwd_open_set.add(Entry {
                tile: start,
                f_score: heuristic(graph, start, dst),
            });
        }
        let mut bwd_open_set = FastPriorityQueue::new();
        let mut bwd_reached = FxHashMap::default();
        bwd_reached.insert(
            dst,
            Reached {
                g_score: 0.0,
                prev: NO_PREV,
            },
        );
        bwd_open_set.add(Entry {
            tile: dst,
            f_score: heuristic(graph, dst, find_closest_source(graph, sources, dst)),
        });
        Self {
            fwd_open_set,
            bwd_open_set,
            fwd_reached,
            bwd_reached,
            neighbor_buffer: Vec::new(),
            meeting_point: None,
            completed: false,
            sources: sources.to_vec(),
            closest_source,
            dst,
            iterations,
            max_tries,
            direction_change_penalty,
        }
    }

    /// `compute()`: runs up to `iterations` steps.
    pub fn compute(&mut self, graph: &impl GraphAdapter) -> PathFindResultType {
        if self.completed {
            return PathFindResultType::Completed;
        }
        self.max_tries -= 1.0;
        let mut iterations = self.iterations;
        while !self.fwd_open_set.is_empty() && !self.bwd_open_set.is_empty() {
            iterations -= 1.0;
            if iterations <= 0.0 {
                if self.max_tries <= 0.0 {
                    return PathFindResultType::PathNotFound;
                }
                return PathFindResultType::Pending;
            }
            let fwd_current = self.fwd_open_set.poll().expect("not empty").tile;
            if self.bwd_reached.contains_key(&fwd_current) {
                self.meeting_point = Some(fwd_current);
                self.completed = true;
                return PathFindResultType::Completed;
            }
            self.expand_node(graph, fwd_current, true);

            let bwd_current = self.bwd_open_set.poll().expect("not empty").tile;
            if self.fwd_reached.contains_key(&bwd_current) {
                self.meeting_point = Some(bwd_current);
                self.completed = true;
                return PathFindResultType::Completed;
            }
            self.expand_node(graph, bwd_current, false);
        }
        if self.completed {
            PathFindResultType::Completed
        } else {
            PathFindResultType::PathNotFound
        }
    }

    fn direction(graph: &impl GraphAdapter, from: u32, to: u32) -> (i8, i8) {
        let (fx, fy) = graph.position(from);
        let (tx, ty) = graph.position(to);
        (sign(tx - fx), sign(ty - fy))
    }

    fn expand_node(&mut self, graph: &impl GraphAdapter, current: u32, is_forward: bool) {
        let goal = if is_forward {
            self.dst
        } else {
            self.closest_source
        };
        let (reached, open_set) = if is_forward {
            (&mut self.fwd_reached, &mut self.fwd_open_set)
        } else {
            (&mut self.bwd_reached, &mut self.bwd_open_set)
        };
        // `current` is never its own neighbour, so its entry cannot change inside the loop.
        let from = reached[&current];
        let mut neighbors = std::mem::take(&mut self.neighbor_buffer);
        neighbors.clear();
        graph.neighbors(current, &mut neighbors);
        for &neighbor in &neighbors {
            if neighbor != goal && !graph.is_traversable(current, neighbor) {
                continue;
            }
            let tentative = from.g_score + graph.cost(neighbor);
            let mut penalty = 0.0;
            // `if (prev)` is falsy for tile 0
            if self.direction_change_penalty > 0.0 && from.prev != NO_PREV && from.prev != 0 {
                let prev_dir = Self::direction(graph, from.prev, current);
                let new_dir = Self::direction(graph, current, neighbor);
                if prev_dir != new_dir {
                    penalty = self.direction_change_penalty;
                }
            }
            let total_g = tentative + penalty;
            let node = Reached {
                g_score: total_g,
                prev: current,
            };
            match reached.entry(neighbor) {
                MapEntry::Occupied(mut e) if total_g < e.get().g_score => {
                    e.insert(node);
                }
                MapEntry::Occupied(_) => continue,
                MapEntry::Vacant(e) => {
                    e.insert(node);
                }
            }
            let f_score = total_g + heuristic(graph, neighbor, goal);
            open_set.add(Entry {
                tile: neighbor,
                f_score,
            });
        }
        self.neighbor_buffer = neighbors;
    }

    /// `reconstructPath()`: sources to destination through the meeting point (empty before completion).
    pub fn reconstruct_path(&self) -> Vec<u32> {
        let Some(meeting) = self.meeting_point else {
            return Vec::new();
        };
        let mut path = vec![meeting];
        let mut current = meeting;
        while let Some(&Reached { prev, .. }) = self.fwd_reached.get(&current)
            && prev != NO_PREV
        {
            current = prev;
            path.push(current);
        }
        path.reverse();
        current = meeting;
        while let Some(&Reached { prev, .. }) = self.bwd_reached.get(&current)
            && prev != NO_PREV
        {
            current = prev;
            path.push(current);
        }
        path
    }

    /// The source nodes the search started from.
    pub fn sources(&self) -> &[u32] {
        &self.sources
    }
}

fn sign(x: f64) -> i8 {
    if x > 0.0 {
        1
    } else if x < 0.0 {
        -1
    } else {
        0
    }
}
