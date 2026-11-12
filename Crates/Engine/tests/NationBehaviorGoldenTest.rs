// SPDX-License-Identifier: AGPL-3.0-only
//! Behaviour-level goldens of NationStructureBehavior, NationNukeBehavior and NationMIRVBehavior.
//!
//! Each `Golden/NationBehaviors/<name>.spec.json` is a scripted scenario (players, territory,
//! structures, behaviour calls); `<name>.golden.ndjson` holds what the upstream engine decided, made
//! by `Reference/NationBehaviorDump.ts`. This test runs the same spec on the Rust engine and
//! compares every step (return value, decisions, state hash). Skipped without the local cache
//! (`Cache/Worktrees/<commit>/resources/maps`).

#![allow(non_snake_case)]

use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Compat::OrderedSet::OrderedSet;
use ofsim_engine::Core::PseudoRandom::PseudoRandom;
use ofsim_engine::Execution::Nation::NationBehaviorPorts::{AttackPort, Decision, EmojiPort};
use ofsim_engine::Execution::Nation::NationEmojiBehavior::respond_to_mirv;
use ofsim_engine::Execution::Nation::NationMIRVBehavior::NationMIRVBehavior;
use ofsim_engine::Execution::Nation::NationNukeBehavior::NationNukeBehavior;
use ofsim_engine::Execution::Nation::NationStructureBehavior::NationStructureBehavior;
use ofsim_engine::Game::Game::{PlayerInfo, PlayerType, SmallId, UnitParams, UnitType};
use ofsim_engine::Game::GameImpl::GameImpl;
use ofsim_engine::Runner::GameRunner::create_game_runner;
use ofsim_record::StartInfo::GameStartInfo;
use serde_json::{Map, Value, json};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const BASELINE: &str = "4e837520e886";

fn find_maps() -> Option<PathBuf> {
    let cache = if let Some(path) = std::env::var_os("OFSIM_CACHE") {
        PathBuf::from(path)
    } else {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .find_map(|a| {
                let nested = a.join("Cache");
                if nested.join("Worktrees").is_dir() {
                    Some(nested)
                } else if a.join("Worktrees").is_dir() && a.join("Records").is_dir() {
                    Some(a.to_path_buf())
                } else {
                    None
                }
            })?
    };
    let maps = cache
        .join("Worktrees")
        .join(BASELINE)
        .join("resources/maps");
    maps.is_dir().then_some(maps)
}

/// Emoji behaviour double: sends nothing, but MIRV responses are the real function.
struct EmojiDouble;

impl EmojiPort for EmojiDouble {
    fn maybe_send_attack_emoji(&mut self, _game: &mut GameImpl, _enemy: SmallId) {}
    fn maybe_send_nuke_emoji(&mut self, _game: &mut GameImpl, _target: SmallId) {}
    fn send_nuke_emoji_to_all(&mut self, _game: &mut GameImpl) {}
    fn respond_to_mirv(&mut self, game: &mut GameImpl, random: &mut PseudoRandom, enemy: SmallId) {
        respond_to_mirv(game, random, enemy);
    }
}

/// Attack behaviour double with scripted answers.
struct AttackDouble {
    incoming: Option<SmallId>,
    should_attack: bool,
}

impl AttackPort for AttackDouble {
    fn should_attack(&mut self, _game: &mut GameImpl, _other: SmallId) -> bool {
        self.should_attack
    }
    fn find_incoming_attack_player(&mut self, _game: &mut GameImpl) -> Option<SmallId> {
        self.incoming
    }
}

enum Behavior {
    Structure(NationStructureBehavior),
    Nuke(NationNukeBehavior),
    Mirv(NationMIRVBehavior),
}

fn decision_json(d: &Decision) -> Value {
    match d {
        Decision::Construct { unit_type, tile } => json!(["construct", unit_type.as_str(), tile]),
        Decision::Upgrade { unit } => json!(["upgrade", unit]),
        Decision::Nuke {
            nuke_type,
            tile,
            wait_ticks,
        } => json!(["nuke", nuke_type.as_str(), tile, *wait_ticks as i64]),
        Decision::Mirv { tile } => json!(["mirv", tile]),
    }
}

fn start_info(spec: &Value) -> GameStartInfo {
    let mut config = json!({
        "gameMap": "World", "gameMapSize": "Normal", "gameMode": "Free For All",
        "gameType": "Singleplayer", "difficulty": "Medium", "nations": "disabled",
        "donateGold": false, "donateTroops": false, "bots": 0, "infiniteGold": false,
        "infiniteTroops": false, "instantBuild": false, "randomSpawn": false,
        "spawnImmunityDuration": 0
    });
    for (k, v) in spec["config"].as_object().unwrap() {
        config[k] = v.clone();
    }
    serde_json::from_value(json!({
        "gameID": spec["gameID"], "lobbyCreatedAt": 0, "config": config, "players": []
    }))
    .unwrap()
}

/// Runs a spec and returns one JSON line per step.
fn run_spec(spec: &Value, maps: &Path) -> Vec<Value> {
    let start = start_info(spec);
    let mut runner =
        create_game_runner(&start, maps, Rules::for_commit(BASELINE).unwrap()).unwrap();
    let game = &mut runner.game;
    game.end_spawn_phase();
    let mut behaviors: HashMap<String, Behavior> = HashMap::new();
    let mut lines = Vec::new();
    let pid = |game: &GameImpl, id: &Value| game.find_player(id.as_str().unwrap()).unwrap();
    for (index, step) in spec["steps"].as_array().unwrap().iter().enumerate() {
        let op = step["op"].as_str().unwrap();
        let mut ret = Value::Null;
        let mut decisions: Vec<Value> = Vec::new();
        match op {
            "addPlayer" => {
                let id = step["id"].as_str().unwrap().to_string();
                let t = match step["type"].as_str().unwrap() {
                    "NATION" => PlayerType::Nation,
                    "HUMAN" => PlayerType::Human,
                    _ => PlayerType::Bot,
                };
                let info =
                    PlayerInfo::new(id.clone(), t, None, id, false, None, vec![], None, None);
                game.add_player(info, step["team"].as_str().map(str::to_string));
            }
            "conquerSquare" => {
                let p = pid(game, &step["player"]);
                let (x0, y0, r) = (
                    step["x"].as_i64().unwrap() as i32,
                    step["y"].as_i64().unwrap() as i32,
                    step["r"].as_i64().unwrap() as i32,
                );
                for x in (x0 - r)..=(x0 + r) {
                    for y in (y0 - r)..=(y0 + r) {
                        if !game.is_valid_coord(x, y) {
                            continue;
                        }
                        let t = game.tile_ref(x, y);
                        if game.map.is_land(t)
                            && !game.map.is_impassable(t)
                            && game.owner_id(t) == 0
                        {
                            game.player_mut(p).conquer(t);
                        }
                    }
                }
            }
            "gold" => {
                let p = pid(game, &step["player"]);
                let amount: i64 = step["amount"].as_str().unwrap().parse().unwrap();
                game.player_mut(p).add_gold(amount);
            }
            "troops" => {
                let p = pid(game, &step["player"]);
                game.player_mut(p)
                    .set_troops(step["amount"].as_f64().unwrap());
            }
            "build" => {
                let p = pid(game, &step["player"]);
                let t = UnitType::from_str(step["type"].as_str().unwrap()).unwrap();
                let mut params = UnitParams::default();
                if let Some(tx) = step["targetX"].as_i64() {
                    params.target_tile =
                        Some(game.tile_ref(tx as i32, step["targetY"].as_i64().unwrap() as i32));
                }
                let tile = game.tile_ref(
                    step["x"].as_i64().unwrap() as i32,
                    step["y"].as_i64().unwrap() as i32,
                );
                let unit = game.player_mut(p).build_unit(t, tile, params);
                for _ in 1..step["level"].as_i64().unwrap_or(1) {
                    game.unit_mut(unit).increase_level();
                }
                if step["construction"].as_bool().unwrap_or(false) {
                    game.unit_mut(unit).set_under_construction(true);
                }
                ret = json!(unit);
            }
            "relation" => {
                let (a, b) = (pid(game, &step["a"]), pid(game, &step["b"]));
                game.player_mut(a)
                    .update_relation(b, step["delta"].as_f64().unwrap());
            }
            "attack" => {
                let (a, b) = (pid(game, &step["attacker"]), pid(game, &step["target"]));
                let source = step["x"]
                    .as_i64()
                    .map(|x| game.tile_ref(x as i32, step["y"].as_i64().unwrap() as i32));
                game.player_mut(a).create_attack(
                    b,
                    step["troops"].as_f64().unwrap(),
                    source,
                    OrderedSet::new(),
                );
            }
            "tick" => {
                for _ in 0..step["n"].as_u64().unwrap() {
                    game.execute_next_tick();
                }
            }
            "structure.new" => {
                let p = pid(game, &step["player"]);
                let random = Rc::new(RefCell::new(PseudoRandom::new(
                    step["seed"].as_f64().unwrap(),
                )));
                let mut b = NationStructureBehavior::new(random, game, p);
                b.log.set_dry_run(true);
                behaviors.insert(
                    step["id"].as_str().unwrap().to_string(),
                    Behavior::Structure(b),
                );
            }
            "structure.call" => {
                let p = pid(game, &step["player"]);
                let Some(Behavior::Structure(b)) = behaviors.get_mut(step["id"].as_str().unwrap())
                else {
                    panic!("no structure behaviour")
                };
                ret = json!(b.handle_structures(game));
                let taken = b.log.take();
                if step["apply"].as_bool().unwrap_or(false) {
                    for d in &taken {
                        match d {
                            Decision::Construct { unit_type, tile } => {
                                game.player_mut(p).build_unit(
                                    *unit_type,
                                    *tile,
                                    UnitParams::default(),
                                );
                            }
                            Decision::Upgrade { unit } => game.player_mut(p).upgrade_unit(*unit),
                            _ => {}
                        }
                    }
                }
                decisions = taken.iter().map(decision_json).collect();
            }
            "nuke.new" => {
                let p = pid(game, &step["player"]);
                let incoming = step["incoming"]
                    .as_str()
                    .map(|s| game.find_player(s).unwrap());
                let attack: Rc<RefCell<dyn AttackPort>> = Rc::new(RefCell::new(AttackDouble {
                    incoming,
                    should_attack: step["shouldAttack"].as_bool().unwrap_or(true),
                }));
                let emoji: Rc<RefCell<dyn EmojiPort>> = Rc::new(RefCell::new(EmojiDouble));
                let random = Rc::new(RefCell::new(PseudoRandom::new(
                    step["seed"].as_f64().unwrap(),
                )));
                let mut b = NationNukeBehavior::new(random, game, p, attack, emoji);
                b.log.set_dry_run(true);
                behaviors.insert(step["id"].as_str().unwrap().to_string(), Behavior::Nuke(b));
            }
            "nuke.call" => {
                let Some(Behavior::Nuke(b)) = behaviors.get_mut(step["id"].as_str().unwrap())
                else {
                    panic!("no nuke behaviour")
                };
                b.maybe_send_nuke(game);
                decisions = b.log.take().iter().map(decision_json).collect();
            }
            "nuke.target" => {
                let Some(Behavior::Nuke(b)) = behaviors.get_mut(step["id"].as_str().unwrap())
                else {
                    panic!("no nuke behaviour")
                };
                ret = match b.find_best_nuke_target(game) {
                    None => Value::Null,
                    Some(t) => json!(game.player(t).id()),
                };
            }
            "mirv.new" => {
                let p = pid(game, &step["player"]);
                let emoji: Rc<RefCell<dyn EmojiPort>> = Rc::new(RefCell::new(EmojiDouble));
                let random = Rc::new(RefCell::new(PseudoRandom::new(
                    step["seed"].as_f64().unwrap(),
                )));
                let mut b = NationMIRVBehavior::new(random, game, p, emoji);
                b.log.set_dry_run(true);
                behaviors.insert(step["id"].as_str().unwrap().to_string(), Behavior::Mirv(b));
            }
            "mirv.call" => {
                let Some(Behavior::Mirv(b)) = behaviors.get_mut(step["id"].as_str().unwrap())
                else {
                    panic!("no mirv behaviour")
                };
                ret = json!(b.consider_mirv(game));
                decisions = b.log.take().iter().map(decision_json).collect();
            }
            other => panic!("unknown step {other}"),
        }
        let mut line = Map::new();
        line.insert("i".into(), json!(index));
        line.insert("op".into(), json!(op));
        line.insert("ret".into(), ret);
        line.insert("decisions".into(), Value::Array(decisions));
        line.insert("hash".into(), json!(game.hash()));
        lines.push(Value::Object(line));
    }
    lines
}

#[test]
fn nation_behaviors_match_upstream_goldens() {
    let Some(maps) = find_maps() else {
        eprintln!("skipped: no baseline maps in the local cache");
        return;
    };
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/Golden/NationBehaviors");
    let mut specs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.to_string_lossy().ends_with(".spec.json"))
        .collect();
    specs.sort();
    assert!(!specs.is_empty(), "no specs in {}", dir.display());
    let mut failures = Vec::new();
    for spec_path in specs {
        let name = spec_path
            .file_name()
            .unwrap()
            .to_string_lossy()
            .replace(".spec.json", "");
        let golden_path = dir.join(format!("{name}.golden.ndjson"));
        let spec: Value =
            serde_json::from_str(&std::fs::read_to_string(&spec_path).unwrap()).unwrap();
        let golden: Vec<Value> = std::fs::read_to_string(&golden_path)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        let actual = run_spec(&spec, &maps);
        assert_eq!(actual.len(), golden.len(), "{name}: step count");
        for (a, g) in actual.iter().zip(&golden) {
            let same = a["i"] == g["i"]
                && a["op"] == g["op"]
                && a["ret"] == g["ret"]
                && a["decisions"] == g["decisions"]
                && a["hash"].as_f64() == g["hash"].as_f64();
            if !same {
                failures.push(format!("{name} step {}: rust {a} upstream {g}", g["i"]));
                break;
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
