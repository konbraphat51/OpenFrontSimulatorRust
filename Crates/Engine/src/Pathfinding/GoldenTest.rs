// SPDX-License-Identifier: AGPL-3.0-only
//! Golden tests of the pathfinding port against upstream results on real maps (Tests/Golden/Pathfinding.json).
//!
//! Needs the baseline worktree's map files (`OFSIM_MAPS` or `Cache/Worktrees/4e837520e886/resources/maps`
//! found by walking up from the crate); a test is skipped when they are absent.

use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Game::WaterManager::WaterManager;
use crate::Pathfinding::Algorithms::AStarWater::AStarWater;
use crate::Pathfinding::Algorithms::AStarWaterBounded::{AStarWaterBounded, SearchBounds};
use crate::Pathfinding::Algorithms::AStarWaterHierarchical::AStarWaterHierarchical;
use crate::Pathfinding::Algorithms::AbstractGraph::{AbstractGraph, AbstractGraphBuilder};
use crate::Pathfinding::Algorithms::BfsGrid::BfsGrid;
use crate::Pathfinding::Algorithms::ConnectedComponents::ConnectedComponents;
use crate::Pathfinding::Line::DistanceBasedBezierCurve;
use crate::Pathfinding::PathFinder::{
    AirStepperEnv, WaterPathFinder, WaterStepperEnv, find_water_path,
};
use crate::Pathfinding::PathFinderAir::AirPathFinder;
use crate::Pathfinding::PathFinderParabola::{
    ParabolaOptions, ParabolaUniversalPathFinder, get_parabola_control_points,
};
use crate::Pathfinding::PathFinderStepper::PathFinderStepper;
use crate::Pathfinding::PathGameView::{PathGameView, SmallId};
use crate::Pathfinding::Spatial::SpatialQuery::{DEFAULT_MAX_DIST, SpatialQuery};
use crate::Pathfinding::Types::{PathResult, Source};
use ofsim_map::{MapData, MapSize, TerrainMap};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const GOLDEN: &str = include_str!("../../Tests/Golden/Pathfinding.json");

/// Finds the baseline worktree's `resources/maps` directory.
fn find_maps() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OFSIM_MAPS") {
        return Some(PathBuf::from(path));
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|ancestor| {
            [
                ancestor.join("Cache/Worktrees/4e837520e886/resources/maps"),
                ancestor.join("Worktrees/4e837520e886/resources/maps"),
            ]
            .into_iter()
            .find(|candidate| candidate.is_dir())
        })
}

fn fnv(values: impl IntoIterator<Item = u32>) -> u32 {
    let mut h: u32 = 0x811c9dc5;
    for v in values {
        for b in 0..4 {
            h ^= (v >> (8 * b)) & 0xff;
            h = h.wrapping_mul(16777619);
        }
    }
    h
}

fn digest(path: &Option<Vec<TileRef>>) -> (i64, u32) {
    match path {
        None => (-1, 0),
        Some(p) => (p.len() as i64, fnv(p.iter().copied())),
    }
}

fn expect_digest(v: &Value) -> (i64, u32) {
    (v[0].as_i64().unwrap(), v[1].as_u64().unwrap() as u32)
}

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
}

fn uvec(v: &Value) -> Vec<u32> {
    v.as_array().unwrap().iter().map(u).collect()
}

/// `T | T[]` of the JSON as a list (the plain-number form is a one-element list).
fn owned_source(v: &Value) -> Vec<u32> {
    if v.is_array() { uvec(v) } else { vec![u(v)] }
}

fn map_of(terrain: &TerrainMap) -> GameMapImpl {
    GameMapImpl::new(
        terrain.width,
        terrain.height,
        terrain.data.clone(),
        terrain.num_land_tiles as i64,
    )
    .unwrap()
}

fn graph_digest(graph: &AbstractGraph) -> [u32; 6] {
    let mut node_vals = Vec::new();
    for n in graph.get_all_nodes() {
        node_vals.extend([n.id, n.x, n.y, n.tile, n.component_id]);
    }
    let mut edge_vals = Vec::new();
    for e in graph.get_all_edges() {
        edge_vals.extend([e.id, e.node_a, e.node_b, e.cost, e.cluster_x, e.cluster_y]);
    }
    let mut cluster_vals = Vec::new();
    for cy in 0..graph.clusters_y {
        for cx in 0..graph.clusters_x {
            let c = graph.get_cluster(cx as i64, cy as i64).unwrap();
            cluster_vals.push(c.node_ids.len() as u32);
            cluster_vals.extend(c.node_ids.iter().copied());
        }
    }
    let mut adj = Vec::new();
    for n in graph.get_all_nodes() {
        let es: Vec<u32> = graph.get_node_edges(n.id).map(|e| e.id).collect();
        adj.push(es.len() as u32);
        adj.extend(es);
    }
    [
        graph.node_count() as u32,
        graph.edge_count() as u32,
        fnv(node_vals),
        fnv(edge_vals),
        fnv(cluster_vals),
        fnv(adj),
    ]
}

fn check_graph(label: &str, graph: &AbstractGraph, expect: &Value) {
    let got = graph_digest(graph);
    let want = [
        u(&expect["nodeCount"]),
        u(&expect["edgeCount"]),
        u(&expect["nodesHash"]),
        u(&expect["edgesHash"]),
        u(&expect["clustersHash"]),
        u(&expect["adjacencyHash"]),
    ];
    assert_eq!(
        got, want,
        "{label}: graph digest (nodes, edges, node/edge/cluster/adjacency hashes)"
    );
}

/// Compares a query list: each entry has `d` (digest) and optionally `full`.
fn check_query(label: &str, index: usize, entry: &Value, got: &Option<Vec<TileRef>>) {
    assert_eq!(
        digest(got),
        expect_digest(&entry["d"]),
        "{label} #{index} from {} to {}",
        entry["from"],
        entry["to"]
    );
    if let Some(full) = entry.get("full").filter(|f| !f.is_null()) {
        assert_eq!(
            got.as_ref().unwrap(),
            &uvec(full),
            "{label} #{index} full path"
        );
    }
}

fn run_map(entry: &Value) {
    let Some(maps) = find_maps() else {
        eprintln!("skipped: no maps directory");
        return;
    };
    let name = entry["name"].as_str().unwrap();
    let size = MapSize::from_wire(entry["size"].as_str().unwrap()).unwrap();
    let data = MapData::load(&maps, name, size).unwrap();
    let map = map_of(&data.game_map);
    let mini = map_of(&data.mini_game_map);
    assert_eq!(
        (map.width(), map.height(), mini.width(), mini.height()),
        (
            u(&entry["width"]),
            u(&entry["height"]),
            u(&entry["miniWidth"]),
            u(&entry["miniHeight"])
        )
    );
    let label = format!("{name} {}", entry["size"].as_str().unwrap());

    // Water manager on the pristine maps: graph and components
    let mut water = WaterManager::new(mini.clone(), false);
    let graph = Rc::clone(water.mini_water_graph().unwrap());
    check_graph(&label, &graph, &entry["graph"]);
    for c in entry["components"].as_array().unwrap() {
        assert_eq!(
            graph.get_component_id(u(&c[0])),
            u(&c[1]),
            "{label} component of mini tile {}",
            c[0]
        );
    }
    for c in entry["componentSizes"].as_array().unwrap() {
        assert_eq!(
            graph.get_component_size(u(&c[0])),
            u(&c[1]),
            "{label} size of component {}",
            c[0]
        );
    }
    let mut cc = ConnectedComponents::new(&mini);
    cc.initialize(&mini);
    for c in entry["componentsDirect"].as_array().unwrap() {
        assert_eq!(
            cc.get_component_id(u(&c[0])),
            u(&c[1]),
            "{label} direct component {}",
            c[0]
        );
    }
    for c in entry["waterComponent"].as_array().unwrap() {
        assert_eq!(
            water.get_water_component(&map, u(&c[0])),
            c[1].as_u64().map(|v| v as u32),
            "{label} getWaterComponent {}",
            c[0]
        );
    }
    for c in entry["hasWaterComponent"].as_array().unwrap() {
        assert_eq!(
            water.has_water_component(&map, u(&c[0]), u(&c[1])),
            c[2].as_bool().unwrap(),
            "{label} hasWaterComponent {}",
            c[0]
        );
    }
    for c in entry["waterComponentSize"].as_array().unwrap() {
        assert_eq!(
            water.get_water_component_size(&map, u(&c[0])),
            c[1].as_u64().map(|v| v as u32),
            "{label} getWaterComponentSize {}",
            c[0]
        );
    }

    // Whole-mini-map water A*
    let mut astar = AStarWater::new(&mini, None, None);
    for (i, q) in entry["astarWater"].as_array().unwrap().iter().enumerate() {
        let from = owned_source(&q["from"]);
        let got = astar.find_path_on(&mini, &from, u(&q["to"]));
        check_query(&format!("{label} AStarWater"), i, q, &got);
    }

    // Bounded water A*
    let mut bounded = AStarWaterBounded::new(&mini, 100 * 100, None, None);
    for (i, q) in entry["bounded"].as_array().unwrap().iter().enumerate() {
        let from = owned_source(&q["from"]);
        let b = &q["bounds"];
        let bounds = SearchBounds {
            min_x: b["minX"].as_i64().unwrap(),
            max_x: b["maxX"].as_i64().unwrap(),
            min_y: b["minY"].as_i64().unwrap(),
            max_y: b["maxY"].as_i64().unwrap(),
        };
        let got = bounded.search_bounded(&mini, &from, u(&q["to"]), bounds);
        check_query(&format!("{label} bounded"), i, q, &got);
        if let Some(d2) = q.get("d2").filter(|v| !v.is_null()) {
            let got2 = bounded.find_path_on(&mini, &from, u(&q["to"]));
            assert_eq!(
                digest(&got2),
                expect_digest(d2),
                "{label} bounded findPath #{i}"
            );
        }
    }

    // HPA* directly (own graph so the manager's chain state stays untouched)
    {
        let mut cc2 = ConnectedComponents::new(&mini);
        cc2.initialize(&mini);
        let mut bfs = BfsGrid::new(mini.width() as usize * mini.height() as usize);
        let own_graph = Rc::new(
            AbstractGraphBuilder::new(
                &mini,
                AbstractGraphBuilder::CLUSTER_SIZE,
                None,
                None,
                Some(Rc::new(std::cell::RefCell::new(cc2))),
                &mut bfs,
            )
            .build(),
        );
        check_graph(&format!("{label} own graph"), &own_graph, &entry["graph"]);
        let mut hpa = AStarWaterHierarchical::new(&mini, own_graph, true);
        for (i, q) in entry["hpaSingle"].as_array().unwrap().iter().enumerate() {
            let got = hpa.find_path_from(&mini, Source::Single(u(&q["from"])), u(&q["to"]));
            check_query(&format!("{label} HPA single"), i, q, &got);
        }
        for (i, q) in entry["hpaMulti"].as_array().unwrap().iter().enumerate() {
            let from = uvec(&q["from"]);
            let got = hpa.find_path_from(&mini, Source::Multi(&from), u(&q["to"]));
            check_query(&format!("{label} HPA multi"), i, q, &got);
        }
    }

    // Shared chain: transport / trade / warship paths
    for (i, q) in entry["chain"].as_array().unwrap().iter().enumerate() {
        let got = find_water_path(&mut water, &map, Source::Single(u(&q["from"])), u(&q["to"]));
        check_query(&format!("{label} chain"), i, q, &got);
    }
    for (i, q) in entry["chainMulti"].as_array().unwrap().iter().enumerate() {
        let from = uvec(&q["from"]);
        let got = find_water_path(&mut water, &map, Source::Multi(&from), u(&q["to"]));
        check_query(&format!("{label} chain multi"), i, q, &got);
    }
    for (i, q) in entry["chainSimple"].as_array().unwrap().iter().enumerate() {
        let got = water.find_water_path_simple(&map, Source::Single(u(&q["from"])), u(&q["to"]));
        check_query(&format!("{label} simple chain"), i, q, &got);
    }
    for (i, q) in entry["chainSimpleMulti"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let from = uvec(&q["from"]);
        let got = water.find_water_path_simple(&map, Source::Multi(&from), u(&q["to"]));
        check_query(&format!("{label} simple chain multi"), i, q, &got);
    }

    // Stepping walks
    for (i, q) in entry["walks"].as_array().unwrap().iter().enumerate() {
        let to = u(&q["to"]);
        let dist = q["dist"].as_f64();
        let mut wpf = WaterPathFinder::new(&water, 0);
        let mut cur = u(&q["from"]);
        let mut seq = Vec::new();
        let mut last = -1;
        let mut trav: Option<Vec<TileRef>> = None;
        for s in 0..200_000 {
            let r = wpf.next(
                &mut WaterStepperEnv {
                    water: &mut water,
                    map: &map,
                },
                cur,
                to,
                dist,
            );
            last = r.status() as i64;
            match r {
                PathResult::Next(node) => {
                    if s == 0 {
                        trav = Some(wpf.path_for_traversal(
                            &mut WaterStepperEnv {
                                water: &mut water,
                                map: &map,
                            },
                            cur,
                            to,
                        ));
                    }
                    cur = node;
                    seq.push(cur);
                }
                PathResult::Complete(node) => {
                    seq.push(node);
                    break;
                }
                PathResult::NotFound => break,
            }
        }
        assert_eq!(
            digest(&Some(seq)),
            expect_digest(&q["d"]),
            "{label} walk #{i}"
        );
        assert_eq!(
            last,
            q["last"].as_i64().unwrap(),
            "{label} walk #{i} last status"
        );
        assert_eq!(
            trav.as_ref().map(|t| digest(&Some(t.clone()))),
            q["trav"].as_array().map(|_| expect_digest(&q["trav"])),
            "{label} walk #{i} traversal"
        );
    }

    check_spatial(&label, entry, map.clone(), &mut water);
    check_parabola(&label, entry, &map);
    check_air(&label, entry, &map);
    check_water_manager(&label, entry, &map, &mini);
}

/// Test game view: the pristine map with the golden's ownership pattern, plus the water manager.
struct TestView<'a> {
    map: GameMapImpl,
    water: &'a mut WaterManager,
    borders: HashMap<SmallId, Vec<TileRef>>,
}

impl PathGameView for TestView<'_> {
    fn map(&self) -> &GameMapImpl {
        &self.map
    }
    fn water_component(&self, tile: TileRef) -> Option<u32> {
        self.water.get_water_component(&self.map, tile)
    }
    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
        for &t in self.borders.get(&player).map(Vec::as_slice).unwrap_or(&[]) {
            f(t);
        }
    }
    fn tile_change_version(&self, _player: SmallId) -> u32 {
        1
    }
    fn find_water_path(&mut self, from: Source, to: TileRef) -> Option<Vec<TileRef>> {
        find_water_path(self.water, &self.map, from, to)
    }
}

fn check_spatial(label: &str, entry: &Value, mut map: GameMapImpl, water: &mut WaterManager) {
    let w = map.width();
    for t in 0..w * map.height() {
        if map.is_land(t) && !map.is_impassable(t) {
            let (x, y) = (map.x(t), map.y(t));
            map.set_owner_id(t, ((x >> 6) + 2 * (y >> 6)) % 5);
        }
    }
    let mut borders: HashMap<SmallId, Vec<TileRef>> = HashMap::new();
    for t in 0..w * map.height() {
        let o = map.owner_id(t);
        if o != 0 && map.is_border(t) {
            borders.entry(o).or_default().push(t);
        }
    }
    let mut view = TestView {
        map,
        water,
        borders,
    };
    let mut sq = SpatialQuery::new();
    let md = |v: &Value| v.as_f64().unwrap_or(DEFAULT_MAX_DIST);
    let res = |r: Option<TileRef>| r.map_or(-1, |t| t as i64);

    for (i, q) in entry["closestShore"].as_array().unwrap().iter().enumerate() {
        let got = sq.closest_shore(&view, u(&q[0]) as SmallId, u(&q[1]), md(&q[2]));
        assert_eq!(
            res(got),
            q[3].as_i64().unwrap(),
            "{label} closestShore #{i} {q}"
        );
    }
    for (i, q) in entry["closestReachableShore"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let got = sq.closest_reachable_shore(
            &view,
            u(&q[0]) as SmallId,
            u(&q[1]) as SmallId,
            u(&q[2]),
            md(&q[3]),
        );
        assert_eq!(
            res(got),
            q[4].as_i64().unwrap(),
            "{label} closestReachableShore #{i} {q}"
        );
    }
    for (i, q) in entry["closestShoreByWater"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let got = sq.closest_shore_by_water(&mut view, u(&q[0]) as SmallId, u(&q[1]));
        assert_eq!(
            res(got),
            q[2].as_i64().unwrap(),
            "{label} closestShoreByWater #{i} {q}"
        );
    }
}

fn bits_hex(v: f64) -> String {
    format!("{:016x}", v.to_bits())
}

fn check_parabola(label: &str, entry: &Value, map: &GameMapImpl) {
    for (i, q) in entry["parabola"].as_array().unwrap().iter().enumerate() {
        let (from, to) = (u(&q["from"]), u(&q["to"]));
        let o = &q["opt"];
        let options = ParabolaOptions {
            increment: o["increment"].as_f64(),
            distance_based_height: o["distanceBasedHeight"].as_bool(),
            direction_up: o["directionUp"].as_bool(),
            ignore_map_bounds: o["ignoreMapBounds"].as_bool(),
            legacy_curve: false,
        };
        let cps = get_parabola_control_points(map, from, to, &options);
        let cp: Vec<String> = cps
            .iter()
            .flat_map(|p| [bits_hex(p.x), bits_hex(p.y)])
            .collect();
        let want: Vec<String> = q["cp"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect();
        assert_eq!(cp, want, "{label} parabola #{i} control points");
        let len = DistanceBasedBezierCurve::get_length(cps[0], cps[1], cps[2], cps[3]);
        assert_eq!(
            bits_hex(len),
            q["len"].as_str().unwrap(),
            "{label} parabola #{i} length"
        );

        // Upstream threw while converting the arc to tiles (it left the map)
        if q["d"].is_null() {
            continue;
        }
        let pf = ParabolaUniversalPathFinder::new(options);
        let path = pf.find_path(map, from, to);
        assert_eq!(
            digest(&Some(path.clone())),
            expect_digest(&q["d"]),
            "{label} parabola #{i} path"
        );
        if let Some(full) = q.get("full").filter(|f| !f.is_null()) {
            assert_eq!(path, uvec(full), "{label} parabola #{i} full");
        }

        if q["walk"].is_null() {
            continue;
        }
        let mut pf = ParabolaUniversalPathFinder::new(options);
        let speed = q["speed"].as_f64();
        let mut cur = from;
        let mut seq = Vec::new();
        let mut last = -1;
        for _ in 0..100_000 {
            let r = pf.next(map, cur, to, speed);
            last = r.status() as i64;
            if let PathResult::Next(node) = r {
                cur = node;
                seq.push(cur);
            } else {
                break;
            }
        }
        assert_eq!(
            digest(&Some(seq)),
            expect_digest(&q["walk"]),
            "{label} parabola #{i} walk"
        );
        assert_eq!(
            last,
            q["last"].as_i64().unwrap(),
            "{label} parabola #{i} last"
        );
        assert_eq!(
            pf.current_index() as i64,
            q["idx"].as_i64().unwrap(),
            "{label} parabola #{i} index"
        );
    }
}

fn check_air(label: &str, entry: &Value, map: &GameMapImpl) {
    for (i, q) in entry["air"].as_array().unwrap().iter().enumerate() {
        let (from, to) = (u(&q["from"]), u(&q["to"]));
        let finder = AirPathFinder::new(u(&q["ticks"]));
        let full = finder.find_path(map, from, to);
        assert_eq!(
            digest(&Some(full.clone())),
            expect_digest(&q["d"]),
            "{label} air #{i}"
        );
        if let Some(f) = q.get("full").filter(|f| !f.is_null()) {
            assert_eq!(full, uvec(f), "{label} air #{i} full");
        }
        let mut stepper: PathFinderStepper<TileRef> = PathFinderStepper::new();
        let mut cur = from;
        let mut seq = Vec::new();
        for _ in 0..100_000 {
            match stepper.next(
                &mut AirStepperEnv {
                    finder: &finder,
                    map,
                },
                cur,
                to,
                None,
            ) {
                PathResult::Next(node) => {
                    cur = node;
                    seq.push(cur);
                }
                PathResult::Complete(node) => {
                    seq.push(node);
                    break;
                }
                PathResult::NotFound => break,
            }
        }
        assert_eq!(
            digest(&Some(seq)),
            expect_digest(&q["walk"]),
            "{label} air #{i} walk"
        );
    }
}

fn terrain_hash(map: &GameMapImpl) -> u32 {
    fnv((0..map.width() * map.height()).map(|t| map.terrain_byte(t) as u32))
}

fn check_water_manager(label: &str, entry: &Value, map: &GameMapImpl, mini: &GameMapImpl) {
    let wm_entry = &entry["waterManager"];
    let mut m2 = map.clone();
    let mut water = WaterManager::new(mini.clone(), false);
    let (mw, mh) = (mini.width(), mini.height());
    let mini_total = (mw * mh) as u64;

    let owned = u(&wm_entry["ownedTile"]);
    let fallout = u(&wm_entry["falloutTile"]);
    m2.set_owner_id(owned, 3);
    m2.set_fallout(fallout, true);

    let batches = wm_entry["batches"].as_array().unwrap();
    let snaps = wm_entry["snaps"].as_array().unwrap();
    let mut snap_index = 0;
    let mut last_snap_version = 0u32;

    let check_snap = |m2: &GameMapImpl,
                      water: &WaterManager,
                      tick: u32,
                      changed: Option<&Vec<TileRef>>,
                      s: &Value| {
        assert_eq!(u(&s["tick"]), tick);
        let l = format!("{label} water manager tick {tick}");
        match changed {
            None => assert!(s["changed"].is_null(), "{l}"),
            Some(c) => {
                assert_eq!(
                    digest(&Some(c.clone())),
                    expect_digest(&s["changed"]),
                    "{l}: changed tiles"
                );
                if let Some(full) = s.get("changedFull").filter(|f| !f.is_null()) {
                    assert_eq!(c, &uvec(full), "{l}: changed tiles (full)");
                }
            }
        }
        assert_eq!(terrain_hash(m2), u(&s["terrain"]), "{l}: map terrain");
        assert_eq!(
            terrain_hash(water.mini_map()),
            u(&s["miniTerrain"]),
            "{l}: mini terrain"
        );
        assert_eq!(
            m2.num_land_tiles(),
            s["landTiles"].as_i64().unwrap(),
            "{l}: land tiles"
        );
        assert_eq!(
            water.mini_map().num_land_tiles(),
            s["miniLandTiles"].as_i64().unwrap(),
            "{l}: mini land tiles"
        );
        assert_eq!(
            m2.num_tiles_with_fallout(),
            u(&s["fallout"]),
            "{l}: fallout"
        );
        assert_eq!(
            m2.water_version(),
            u(&s["waterVersion"]),
            "{l}: water version"
        );
        assert_eq!(
            water.water_graph_version(),
            u(&s["graphVersion"]),
            "{l}: graph version"
        );
        let graph = water.mini_water_graph().unwrap();
        check_graph(&l, graph, &s["graph"]);
        let mut cc_hash: u32 = 7;
        for i in 0..200u64 {
            let t = ((i * 2654435761) % mini_total) as u32;
            cc_hash = cc_hash
                .wrapping_mul(31)
                .wrapping_add(graph.get_component_id(t));
        }
        assert_eq!(cc_hash, u(&s["ccSample"]), "{l}: component sample");
    };

    check_snap(&m2, &water, 0, None, &snaps[snap_index]);
    snap_index += 1;
    for t in 1..=130u32 {
        for b in batches.iter().filter(|b| u(&b["tick"]) == t) {
            for c in b["centers"].as_array().unwrap() {
                let (cx, cy, r) = (
                    c[0].as_i64().unwrap() as i32,
                    c[1].as_i64().unwrap() as i32,
                    c[2].as_i64().unwrap() as i32,
                );
                for x in cx - r..=cx + r {
                    for y in cy - r..=cy + r {
                        if m2.is_valid_coord(x, y) && (x - cx).pow(2) + (y - cy).pow(2) <= r * r {
                            water.queue_tile(m2.tile_ref(x, y));
                        }
                    }
                }
            }
            if t == 5 {
                water.queue_tile(owned);
                water.queue_tile(fallout);
            }
        }
        let changed = water.tick(&mut m2, t);
        let version = water.water_graph_version();
        let wanted = !changed.is_empty()
            || [25, 26, 50, 54, 55, 56, 110, 130].contains(&t)
            || version != last_snap_version;
        let expected_here = snaps.get(snap_index).is_some_and(|s| u(&s["tick"]) == t);
        assert_eq!(
            wanted, expected_here,
            "{label} water manager: snapshot tick set differs at tick {t}"
        );
        if wanted {
            check_snap(&m2, &water, t, Some(&changed), &snaps[snap_index]);
            snap_index += 1;
            last_snap_version = version;
        }
    }
    assert_eq!(snap_index, snaps.len());

    for (i, q) in wm_entry["queries"].as_array().unwrap().iter().enumerate() {
        let (from, to) = (u(&q["from"]), u(&q["to"]));
        let got = find_water_path(&mut water, &m2, Source::Single(from), to);
        check_query(&format!("{label} post-crater chain"), i, q, &got);
        if let Some(ds) = q.get("ds").filter(|v| !v.is_null()) {
            let got = water.find_water_path_simple(&m2, Source::Single(from), to);
            assert_eq!(
                digest(&got),
                expect_digest(ds),
                "{label} post-crater simple chain #{i}"
            );
        }
    }
    for c in wm_entry["waterComponent"].as_array().unwrap() {
        let tile = u(&c[0]);
        assert_eq!(
            water.get_water_component(&m2, tile),
            c[1].as_u64().map(|v| v as u32),
            "{label} post-crater component {tile}"
        );
        assert_eq!(
            water.get_water_component_size(&m2, tile),
            c[2].as_u64().map(|v| v as u32),
            "{label} post-crater component size {tile}"
        );
    }
}

fn golden_entry(name: &str) -> Value {
    let all: Value = serde_json::from_str(GOLDEN).unwrap();
    all.as_array()
        .unwrap()
        .iter()
        .find(|e| e["name"] == name)
        .unwrap()
        .clone()
}

#[test]
fn halkidiki_compact() {
    run_map(&golden_entry("Halkidiki"));
}

#[test]
fn world() {
    run_map(&golden_entry("World"));
}

#[test]
fn lemnos() {
    run_map(&golden_entry("Lemnos"));
}

#[test]
fn giant_world_map() {
    run_map(&golden_entry("Giant World Map"));
}

/// Mean time of the golden's shore-to-shore chain queries (transport and trade ships) on the large map.
/// Run with `cargo test --release -- --ignored --nocapture timing`; the generator prints the tsx figure to stderr.
#[test]
#[ignore]
fn giant_world_map_timing() {
    let Some(maps) = find_maps() else {
        return;
    };
    let entry = golden_entry("Giant World Map");
    let data = MapData::load(&maps, "Giant World Map", MapSize::Normal).unwrap();
    let map = map_of(&data.game_map);
    let built = std::time::Instant::now();
    let mut water = WaterManager::new(map_of(&data.mini_game_map), false);
    eprintln!(
        "Giant World Map: water manager (components, graph, HPA) built in {:.1} ms",
        built.elapsed().as_secs_f64() * 1000.0
    );
    // The first pass starts with empty edge path caches (like the tsx figure), the second is warm
    for pass in ["cold", "warm"] {
        let mut total = std::time::Duration::ZERO;
        let mut timed = 0;
        for (i, q) in entry["chain"].as_array().unwrap().iter().enumerate() {
            let started = std::time::Instant::now();
            let _ = find_water_path(&mut water, &map, Source::Single(u(&q["from"])), u(&q["to"]));
            if i % 4 != 2 && i % 4 != 3 {
                total += started.elapsed();
                timed += 1;
            }
        }
        eprintln!(
            "Giant World Map ({pass}): mean {:.3} ms over {timed} queries",
            total.as_secs_f64() * 1000.0 / timed as f64
        );
    }
}
