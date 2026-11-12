// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/pathfinding/algorithms/AStar.AbstractGraph.ts: A* over the abstract water graph (single and multi source).
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! g-scores live in an `f32` array like upstream's `Float32Array`: the candidate g stays `f64`
//! until stored, and comparisons read the rounded stored value.

use super::AbstractGraph::AbstractGraph;
use super::PriorityQueue::{MinHeap, PriorityQueue};
use std::rc::Rc;

/// A* on an [`AbstractGraph`]; ids are node ids.
#[derive(Debug)]
pub struct AbstractGraphAStar {
    stamp: u32,
    closed_stamp: Vec<u32>,
    g_score_stamp: Vec<u32>,
    g_score: Vec<f32>,
    came_from: Vec<i32>,
    start_node: Vec<i32>,
    queue: MinHeap,
    graph: Rc<AbstractGraph>,
    heuristic_weight: f64,
    max_iterations: i64,
}

impl AbstractGraphAStar {
    /// Creates a finder for `graph`; defaults are weight 1 and 100,000 iterations.
    pub fn new(
        graph: Rc<AbstractGraph>,
        heuristic_weight: Option<f64>,
        max_iterations: Option<i64>,
    ) -> Self {
        let num_nodes = graph.node_count();
        let queue = MinHeap::new(num_nodes + graph.edge_count() * 2);
        Self {
            stamp: 1,
            closed_stamp: vec![0; num_nodes],
            g_score_stamp: vec![0; num_nodes],
            g_score: vec![0.0; num_nodes],
            came_from: vec![0; num_nodes],
            start_node: vec![0; num_nodes],
            queue,
            graph,
            heuristic_weight: heuristic_weight.unwrap_or(1.0),
            max_iterations: max_iterations.unwrap_or(100_000),
        }
    }

    /// Path from one node to another (node ids).
    pub fn find_path(&mut self, start: u32, goal: u32) -> Option<Vec<u32>> {
        self.find_path_single(start, goal)
    }

    /// Path from the best of several start nodes; `None` for an empty list. One start falls back to the single search.
    pub fn find_path_multi(&mut self, starts: &[u32], goal: u32) -> Option<Vec<u32>> {
        if starts.is_empty() {
            return None;
        }
        if starts.len() == 1 {
            return self.find_path_single(starts[0], goal);
        }
        self.find_path_multi_source(starts, goal)
    }

    fn next_stamp(&mut self) -> u32 {
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.closed_stamp.fill(0);
            self.g_score_stamp.fill(0);
            self.stamp = 1;
        }
        self.stamp
    }

    fn find_path_single(&mut self, start_id: u32, goal_id: u32) -> Option<Vec<u32>> {
        let stamp = self.next_stamp();
        let graph = Rc::clone(&self.graph);
        let weight = self.heuristic_weight;

        let goal_node = graph.get_node(goal_id)?;
        let (goal_x, goal_y) = (goal_node.x as f64, goal_node.y as f64);
        let start_node = graph.get_node(start_id)?;

        self.queue.clear();
        self.g_score[start_id as usize] = 0.0;
        self.g_score_stamp[start_id as usize] = stamp;
        self.came_from[start_id as usize] = -1;

        let start_h =
            weight * ((start_node.x as f64 - goal_x).abs() + (start_node.y as f64 - goal_y).abs());
        self.queue.push(start_id, start_h);

        self.run(&graph, stamp, goal_id, goal_x, goal_y, false)
    }

    fn find_path_multi_source(&mut self, start_ids: &[u32], goal_id: u32) -> Option<Vec<u32>> {
        let stamp = self.next_stamp();
        let graph = Rc::clone(&self.graph);
        let weight = self.heuristic_weight;

        let goal_node = graph.get_node(goal_id)?;
        let (goal_x, goal_y) = (goal_node.x as f64, goal_node.y as f64);

        self.queue.clear();
        for &start_id in start_ids {
            let Some(node) = graph.get_node(start_id) else {
                continue;
            };
            self.g_score[start_id as usize] = 0.0;
            self.g_score_stamp[start_id as usize] = stamp;
            self.came_from[start_id as usize] = -1;
            self.start_node[start_id as usize] = start_id as i32; // each start is its own origin

            let h = weight * ((node.x as f64 - goal_x).abs() + (node.y as f64 - goal_y).abs());
            self.queue.push(start_id, h);
        }

        self.run(&graph, stamp, goal_id, goal_x, goal_y, true)
    }

    /// Shared main loop; `track_origin` propagates the start node (multi-source only).
    fn run(
        &mut self,
        graph: &AbstractGraph,
        stamp: u32,
        goal_id: u32,
        goal_x: f64,
        goal_y: f64,
        track_origin: bool,
    ) -> Option<Vec<u32>> {
        let weight = self.heuristic_weight;
        let mut iterations = self.max_iterations;

        while !self.queue.is_empty() {
            iterations -= 1;
            if iterations <= 0 {
                return None;
            }

            let current = self.queue.pop();
            if self.closed_stamp[current as usize] == stamp {
                continue;
            }
            self.closed_stamp[current as usize] = stamp;

            if current == goal_id {
                return self.build_path_from_goal(goal_id);
            }

            let current_g = self.g_score[current as usize] as f64;
            let current_start = self.start_node[current as usize];

            for edge in graph.get_node_edges(current) {
                let neighbor = graph.get_other_node(edge, current);
                if self.closed_stamp[neighbor as usize] == stamp {
                    continue;
                }

                let tentative_g = current_g + edge.cost as f64;
                if self.g_score_stamp[neighbor as usize] != stamp
                    || tentative_g < self.g_score[neighbor as usize] as f64
                {
                    self.came_from[neighbor as usize] = current as i32;
                    self.g_score[neighbor as usize] = tentative_g as f32;
                    self.g_score_stamp[neighbor as usize] = stamp;
                    if track_origin {
                        self.start_node[neighbor as usize] = current_start;
                    }

                    if let Some(neighbor_node) = graph.get_node(neighbor) {
                        let h = weight
                            * ((neighbor_node.x as f64 - goal_x).abs()
                                + (neighbor_node.y as f64 - goal_y).abs());
                        self.queue.push(neighbor, tentative_g + h);
                    }
                }
            }
        }
        None
    }

    fn build_path_from_goal(&self, goal_id: u32) -> Option<Vec<u32>> {
        let mut path = Vec::new();
        let mut current = goal_id as i64;
        let max_len = self.came_from.len();
        while current != -1 {
            if current < 0 || current >= max_len as i64 {
                return None;
            }
            path.push(current as u32);
            if path.len() > max_len {
                return None;
            }
            current = self.came_from[current as usize] as i64;
        }
        path.reverse();
        Some(path)
    }
}
