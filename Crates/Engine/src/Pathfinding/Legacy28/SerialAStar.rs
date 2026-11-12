// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/SerialAStar.ts: bidirectional A* over a graph adapter, resumable across ticks.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The adapter is passed on every call instead of being owned, because the maps it reads belong to the game.

use super::FastPriorityQueue::{Entry, FastPriorityQueue};
use std::collections::HashMap;

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
    /// Neighbours of a node, in upstream's order.
    fn neighbors(&self, node: u32) -> Vec<u32>;
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
    fwd_came_from: HashMap<u32, u32>,
    bwd_came_from: HashMap<u32, u32>,
    fwd_g_score: HashMap<u32, f64>,
    bwd_g_score: HashMap<u32, f64>,
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
        let mut fwd_g_score = HashMap::new();
        for &start in sources {
            fwd_g_score.insert(start, 0.0);
            fwd_open_set.add(Entry {
                tile: start,
                f_score: heuristic(graph, start, dst),
            });
        }
        let mut bwd_open_set = FastPriorityQueue::new();
        let mut bwd_g_score = HashMap::new();
        bwd_g_score.insert(dst, 0.0);
        bwd_open_set.add(Entry {
            tile: dst,
            f_score: heuristic(graph, dst, find_closest_source(graph, sources, dst)),
        });
        Self {
            fwd_open_set,
            bwd_open_set,
            fwd_came_from: HashMap::new(),
            bwd_came_from: HashMap::new(),
            fwd_g_score,
            bwd_g_score,
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
            if self.bwd_g_score.contains_key(&fwd_current) {
                self.meeting_point = Some(fwd_current);
                self.completed = true;
                return PathFindResultType::Completed;
            }
            self.expand_node(graph, fwd_current, true);

            let bwd_current = self.bwd_open_set.poll().expect("not empty").tile;
            if self.fwd_g_score.contains_key(&bwd_current) {
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
        for neighbor in graph.neighbors(current) {
            if neighbor != goal && !graph.is_traversable(current, neighbor) {
                continue;
            }
            let (g_score, open_set, came_from) = if is_forward {
                (
                    &mut self.fwd_g_score,
                    &mut self.fwd_open_set,
                    &mut self.fwd_came_from,
                )
            } else {
                (
                    &mut self.bwd_g_score,
                    &mut self.bwd_open_set,
                    &mut self.bwd_came_from,
                )
            };
            let tentative = g_score[&current] + graph.cost(neighbor);
            let mut penalty = 0.0;
            if self.direction_change_penalty > 0.0 {
                // `if (prev)` is falsy for tile 0
                if let Some(&prev) = came_from.get(&current)
                    && prev != 0
                {
                    let prev_dir = Self::direction(graph, prev, current);
                    let new_dir = Self::direction(graph, current, neighbor);
                    if prev_dir != new_dir {
                        penalty = self.direction_change_penalty;
                    }
                }
            }
            let total_g = tentative + penalty;
            if g_score.get(&neighbor).is_none_or(|&g| total_g < g) {
                came_from.insert(neighbor, current);
                g_score.insert(neighbor, total_g);
                let f_score = total_g + heuristic(graph, neighbor, goal);
                open_set.add(Entry {
                    tile: neighbor,
                    f_score,
                });
            }
        }
    }

    /// `reconstructPath()`: sources to destination through the meeting point (empty before completion).
    pub fn reconstruct_path(&self) -> Vec<u32> {
        let Some(meeting) = self.meeting_point else {
            return Vec::new();
        };
        let mut path = vec![meeting];
        let mut current = meeting;
        while let Some(&prev) = self.fwd_came_from.get(&current) {
            current = prev;
            path.push(current);
        }
        path.reverse();
        current = meeting;
        while let Some(&next) = self.bwd_came_from.get(&current) {
            current = next;
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
