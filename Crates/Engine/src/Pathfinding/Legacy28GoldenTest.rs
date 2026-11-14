// SPDX-License-Identifier: AGPL-3.0-only
//! Golden tests of the v0.28.4 pathfinding port against the upstream engine (Tests/Golden/Legacy28.json).
//!
//! Needs the maps of the v0.28.4 worktree (`OFSIM_MAPS_28` or `Cache/Worktrees/793efe4dd1ba/resources/maps`
//! found by walking up from the crate); a test is skipped when they are absent.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::Legacy28::MiniAStar::MiniAStar;
use crate::Pathfinding::Legacy28::PathFinding::{
    AirPathFinder, ParabolaPathFinder, PathFinder, StepResult,
};
use crate::Pathfinding::Legacy28::SerialAStar::PathFindResultType;
use ofsim_map::{MapData, MapSize, TerrainMap};
use serde_json::Value;
use std::path::{Path, PathBuf};

const GOLDEN: &str = include_str!("../../Tests/Golden/Legacy28.json");

fn find_maps() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("OFSIM_MAPS_28") {
        return Some(PathBuf::from(path));
    }
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find_map(|ancestor| {
            [
                ancestor.join("Cache/Worktrees/793efe4dd1ba/resources/maps"),
                ancestor.join("Worktrees/793efe4dd1ba/resources/maps"),
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

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap() as u32
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

fn result_code(r: PathFindResultType) -> u32 {
    match r {
        PathFindResultType::NextTile => 0,
        PathFindResultType::Pending => 1,
        PathFindResultType::Completed => 2,
        PathFindResultType::PathNotFound => 3,
    }
}

#[test]
fn legacy28_pathfinding_matches_upstream() {
    let Some(maps) = find_maps() else {
        eprintln!("skipped: no v0.28.4 maps");
        return;
    };
    let golden: Value = serde_json::from_str(GOLDEN).unwrap();
    for (name, want) in golden["maps"].as_object().unwrap() {
        let data = MapData::load(&maps, name, MapSize::Normal).unwrap();
        let map = map_of(&data.game_map);
        let mini = map_of(&data.mini_game_map);

        for q in want["mini"].as_array().unwrap() {
            let (src, dst) = (u(&q["src"]), u(&q["dst"]));
            let mut a = MiniAStar::new(
                &map,
                &mini,
                &[src],
                false,
                dst,
                q["iterations"].as_f64().unwrap(),
                q["maxTries"].as_f64().unwrap(),
                true,
                0.0,
                true,
            );
            let mut seq = Vec::new();
            let mut calls = 0;
            let mut r;
            loop {
                r = a.compute(&mini);
                seq.push(result_code(r));
                calls += 1;
                if r != PathFindResultType::Pending || calls >= 200 {
                    break;
                }
            }
            let p: Vec<TileRef> = if r == PathFindResultType::Completed {
                a.reconstruct_path(&map, &mini)
            } else {
                Vec::new()
            };
            let label = format!("{name} mini {src}->{dst}");
            assert_eq!(calls, u(&q["calls"]), "{label} calls");
            assert_eq!(result_code(r), u(&q["last"]), "{label} last");
            assert_eq!(p.len() as u32, u(&q["len"]), "{label} len");
            assert_eq!(fnv(p), u(&q["hash"]), "{label} hash");
            assert_eq!(fnv(seq), u(&q["seq"]), "{label} seq");
        }

        for q in want["land"].as_array().unwrap() {
            let (src, dst) = (u(&q["src"]), u(&q["dst"]));
            let mut a = MiniAStar::new(
                &map,
                &mini,
                &[src],
                false,
                dst,
                5000.0,
                20.0,
                false,
                3.0,
                true,
            );
            let mut calls = 0;
            let mut r;
            loop {
                r = a.compute(&mini);
                calls += 1;
                if r != PathFindResultType::Pending || calls >= 50 {
                    break;
                }
            }
            let p = if r == PathFindResultType::Completed {
                a.reconstruct_path(&map, &mini)
            } else {
                Vec::new()
            };
            let label = format!("{name} land {src}->{dst}");
            assert_eq!(calls, u(&q["calls"]), "{label} calls");
            assert_eq!(result_code(r), u(&q["last"]), "{label} last");
            assert_eq!(p.len() as u32, u(&q["len"]), "{label} len");
            assert_eq!(fnv(p), u(&q["hash"]), "{label} hash");
        }

        for q in want["multi"].as_array().unwrap() {
            let srcs: Vec<u32> = q["srcs"].as_array().unwrap().iter().map(u).collect();
            let dst = u(&q["dst"]);
            let mut a = MiniAStar::new(
                &map,
                &mini,
                &srcs,
                true,
                dst,
                1_000_000.0,
                1.0,
                true,
                0.0,
                true,
            );
            let r = a.compute(&mini);
            let p = if r == PathFindResultType::Completed {
                a.reconstruct_path(&map, &mini)
            } else {
                Vec::new()
            };
            let label = format!("{name} multi ->{dst}");
            assert_eq!(result_code(r), u(&q["last"]), "{label} last");
            assert_eq!(p.len() as u32, u(&q["len"]), "{label} len");
            assert_eq!(fnv(p), u(&q["hash"]), "{label} hash");
        }

        check_stepper(name, want, &map, &mini);

        for q in want["air"].as_array().unwrap() {
            let (src, dst) = (u(&q["src"]), u(&q["dst"]));
            let mut ap =
                AirPathFinder::new(PseudoRandom::new_seedrandom(q["seed"].as_f64().unwrap()));
            let mut cur = src;
            let mut seq = Vec::new();
            for _ in 0..4000 {
                match ap.next_tile(&map, cur, dst) {
                    None => {
                        seq.push(0xfffffffe);
                        break;
                    }
                    Some(t) => {
                        seq.push(t);
                        cur = t;
                    }
                }
            }
            let label = format!("{name} air {src}->{dst}");
            assert_eq!(seq.len() as u32, u(&q["len"]), "{label} len");
            assert_eq!(fnv(seq), u(&q["hash"]), "{label} hash");
        }

        for q in want["parabola"].as_array().unwrap() {
            let (src, dst) = (u(&q["src"]), u(&q["dst"]));
            let mut pp = ParabolaPathFinder::new();
            pp.compute_control_points(
                &map,
                src,
                dst,
                q["increment"].as_f64().unwrap(),
                q["based"].as_bool().unwrap(),
            );
            let all = pp.all_tiles(&map);
            let mut seq = Vec::new();
            let speed = q["speed"].as_f64().unwrap();
            for _ in 0..3000 {
                match pp.next_tile(&map, speed) {
                    None => {
                        seq.push(0xfffffffe);
                        break;
                    }
                    Some(t) => {
                        seq.push(t);
                        seq.push(pp.current_index() as u32);
                    }
                }
            }
            let label = format!("{name} parabola {src}->{dst}");
            assert_eq!(all.len() as u32, u(&q["all"][0]), "{label} all len");
            assert_eq!(fnv(all), u(&q["all"][1]), "{label} all");
            assert_eq!(seq.len() as u32, u(&q["steps"]), "{label} steps");
            assert_eq!(fnv(seq), u(&q["hash"]), "{label} hash");
        }
    }
}

fn check_stepper(name: &str, want: &Value, map: &GameMapImpl, mini: &GameMapImpl) {
    for q in want["stepper"].as_array().unwrap() {
        let src = u(&q["src"]);
        let (dst0, dst1) = (u(&q["dst0"]), u(&q["dst1"]));
        let mut pf = PathFinder::mini(
            q["iterations"].as_f64().unwrap(),
            true,
            q["maxTries"].as_f64().unwrap(),
        );
        let dist = u(&q["dist"]);
        let mut cur = src;
        let mut dst = dst0;
        let mut seq: Vec<u32> = Vec::new();
        let mut steps = 0;
        while steps < 6000 {
            if steps == 400 {
                dst = dst1;
            }
            let r = pf.next_tile(map, mini, cur, dst, dist);
            seq.push(result_code(r.result_type()));
            seq.push(match r {
                StepResult::NextTile(t) | StepResult::Completed(t) => t,
                _ => 0xffffffff,
            });
            match r {
                StepResult::NextTile(t) => cur = t,
                StepResult::Completed(_) | StepResult::PathNotFound => break,
                StepResult::Pending => {}
            }
            steps += 1;
        }
        let label = format!("{name} stepper {src}->{dst0}");
        assert_eq!(steps, u(&q["steps"]), "{label} steps");
        assert_eq!(cur, u(&q["end"]), "{label} end");
        assert_eq!(fnv(seq), u(&q["hash"]), "{label} hash");
    }
}
