// SPDX-License-Identifier: AGPL-3.0-only
//! Water executions on a small synthetic map (two continents, a channel and an inland lake):
//! boat retreat, trade ship payout, warship patrol and move orders, and the boat spawn checks.
//!
//! Landings are not covered here: they start an `AttackExecution`, which another module owns. The
//! differential scenario (`ofsim naval-scenario`) covers them against the upstream engine.

#![allow(non_snake_case)]

use ofsim_engine::Builds::Rules::Rules;
use ofsim_engine::Configuration::Config::{Config, GameConfig, NationsSetting};
use ofsim_engine::Execution::BoatRetreatExecution::BoatRetreatExecution;
use ofsim_engine::Execution::MoveWarshipExecution::MoveWarshipExecution;
use ofsim_engine::Execution::TradeShipExecution::TradeShipExecution;
use ofsim_engine::Execution::TransportShipExecution::TransportShipExecution;
use ofsim_engine::Execution::WarshipExecution::WarshipExecution;
use ofsim_engine::Game::Game::{
    Difficulty, GameMapSize, GameMapType, GameMode, GameType, PlayerInfo, PlayerType, SmallId,
    UnitId, UnitParams, UnitType,
};
use ofsim_engine::Game::GameImpl::GameImpl;
use ofsim_engine::Game::GameMap::{GameMapImpl, TileRef};
use ofsim_engine::Game::TransportShipUtils::{can_build_transport_ship, target_transport_tile};

const W: u32 = 40;
const H: u32 = 20;
const LAND: u8 = 0x80;
const SHORE: u8 = 0x40;
const OCEAN: u8 = 0x20;

/// True inside the inland lake (base resolution coordinates).
fn is_lake(x: i32, y: i32) -> bool {
    (3..7).contains(&x) && (5..9).contains(&y)
}

/// Land of the west continent (without the lake) and the east continent (base resolution).
fn is_land(x: i32, y: i32) -> bool {
    (x < 10 && !is_lake(x, y)) || x >= 30
}

/// Terrain byte of tile (x, y) of a map at `scale` times lower resolution than the base map.
fn terrain_at(x: u32, y: u32, scale: u32) -> u8 {
    let s = scale as i32;
    let (xi, yi) = (x as i32, y as i32);
    let here = is_land(xi * s, yi * s);
    let (w, h) = ((W / scale) as i32, (H / scale) as i32);
    let touches_other = [(0, 1), (0, -1), (1, 0), (-1, 0)].iter().any(|&(dx, dy)| {
        let (nx, ny) = (xi + dx, yi + dy);
        nx >= 0 && ny >= 0 && nx < w && ny < h && is_land(nx * s, ny * s) != here
    });
    let mut byte = if here { LAND | 5 } else { 10 };
    if touches_other {
        byte |= SHORE;
    }
    if !here && !is_lake(xi * s, yi * s) {
        byte |= OCEAN;
    }
    byte
}

fn make_map(scale: u32) -> GameMapImpl {
    let (w, h) = (W / scale, H / scale);
    let mut terrain = Vec::new();
    for y in 0..h {
        for x in 0..w {
            terrain.push(terrain_at(x, y, scale));
        }
    }
    let land = terrain.iter().filter(|&&b| b & LAND != 0).count() as i64;
    GameMapImpl::new(w, h, terrain, land).unwrap()
}

fn make_game() -> (GameImpl, SmallId, SmallId) {
    let config = GameConfig {
        game_map: GameMapType::AmazonRiver,
        difficulty: Difficulty::Medium,
        donate_gold: false,
        donate_troops: false,
        game_type: GameType::Singleplayer,
        game_mode: GameMode::FFA,
        ranked_type: None,
        game_map_size: GameMapSize::Normal,
        doomsday_clock: None,
        overtime: None,
        nations: NationsSetting::Disabled,
        bots: 0,
        infinite_gold: false,
        infinite_troops: false,
        instant_build: false,
        disable_nav_mesh: None,
        disable_alliances: None,
        water_nukes: None,
        random_spawn: false,
        max_players: None,
        max_timer_value: None,
        custom_alliance_duration: None,
        start_delay: None,
        spawn_immunity_duration: Some(0),
        disabled_units: None,
        player_teams: None,
        gold_multiplier: None,
        starting_gold: None,
        host_cheats: None,
    };
    let humans: Vec<PlayerInfo> = ["a", "b"]
        .iter()
        .map(|n| {
            PlayerInfo::new(
                n.to_string(),
                PlayerType::Human,
                Some(format!("client-{n}")),
                format!("id-{n}"),
                false,
                None,
                vec![],
                None,
                None,
            )
        })
        .collect();
    let mut game = GameImpl::new(
        humans,
        vec![],
        make_map(1),
        make_map(2),
        Config::new(config, false, Rules::baseline()),
        None,
    );
    game.end_spawn_phase();
    let (a, b) = (1, 2);
    for y in 5..15 {
        for x in 7..10 {
            let t = game.tile_ref(x, y);
            game.player_mut(a).conquer(t);
        }
        for x in 30..33 {
            let t = game.tile_ref(x, y);
            game.player_mut(b).conquer(t);
        }
    }
    for p in [a, b] {
        game.player_mut(p).set_troops(1000.0);
        game.player_mut(p).add_gold(10_000_000);
    }
    (game, a, b)
}

fn run(game: &mut GameImpl, ticks: u32) {
    for _ in 0..ticks {
        game.execute_next_tick();
    }
}

fn build_port(game: &mut GameImpl, owner: SmallId, x: i32, y: i32) -> UnitId {
    let tile = game.tile_ref(x, y);
    game.player_mut(owner)
        .build_unit(UnitType::Port, tile, UnitParams::default())
}

#[test]
fn boat_can_be_built_towards_the_other_shore_only() {
    let (game, a, b) = make_game();
    let enemy_shore = game.tile_ref(30, 10);
    let own_shore = game.tile_ref(9, 10);
    let lake_shore = game.tile_ref(2, 6);
    assert!(target_transport_tile(&game, a, enemy_shore).is_some());
    assert!(can_build_transport_ship(&game, a, enemy_shore).is_some());
    assert!(can_build_transport_ship(&game, a, own_shore).is_none());
    // A shore on a disconnected lake is never a landing target: the next reachable shore is chosen
    let landing = target_transport_tile(&game, b, lake_shore).expect("a reachable shore exists");
    let lake_component = game
        .water_manager
        .get_water_component(&game.map, game.tile_ref(4, 6));
    let landing_component = game.water_manager.get_water_component(&game.map, landing);
    assert_ne!(landing_component, lake_component);
}

#[test]
fn retreating_boat_returns_three_quarters_of_its_troops() {
    let (mut game, a, b) = make_game();
    let target = game.tile_ref(30, 10);
    let before = game.player(a).troops();
    game.add_execution(Box::new(TransportShipExecution::new(
        a,
        target,
        Some(100.0),
    )));
    run(&mut game, 5);
    assert_eq!(game.player(a).unit_count(UnitType::TransportShip), 1);
    assert_eq!(game.player(a).troops(), before - 100.0);
    let boat = game.player(a).units_of(UnitType::TransportShip)[0];
    assert_eq!(game.player(b).unit_count(UnitType::TransportShip), 0);

    game.add_execution(Box::new(BoatRetreatExecution::new(a, boat)));
    run(&mut game, 300);
    assert_eq!(game.player(a).unit_count(UnitType::TransportShip), 0);
    assert_eq!(game.player(a).troops(), before - 100.0 + 75.0);
}

#[test]
fn boat_to_own_territory_is_refused() {
    let (mut game, a, _) = make_game();
    let target = game.tile_ref(9, 10);
    game.add_execution(Box::new(TransportShipExecution::new(a, target, Some(50.0))));
    run(&mut game, 3);
    assert_eq!(game.player(a).unit_count(UnitType::TransportShip), 0);
    assert_eq!(game.player(a).troops(), 1000.0);
}

#[test]
fn trade_ship_pays_both_port_owners() {
    let (mut game, a, b) = make_game();
    let port_a = build_port(&mut game, a, 9, 10);
    let port_b = build_port(&mut game, b, 30, 10);
    let (gold_a, gold_b) = (game.player(a).gold(), game.player(b).gold());
    game.add_execution(Box::new(TradeShipExecution::new(a, port_a, port_b)));
    run(&mut game, 2);
    assert_eq!(game.player(a).unit_count(UnitType::TradeShip), 1);
    run(&mut game, 200);
    assert_eq!(game.player(a).unit_count(UnitType::TradeShip), 0);
    let (paid_a, paid_b) = (
        game.player(a).gold() - gold_a,
        game.player(b).gold() - gold_b,
    );
    assert!(paid_a > 0);
    assert_eq!(paid_a, paid_b);
    assert_eq!(game.player(a).trade_gold(), paid_a);
    assert_eq!(game.player(b).trade_gold(), paid_b);
}

#[test]
fn trade_ship_is_deleted_when_its_destination_port_is_lost() {
    let (mut game, a, b) = make_game();
    let port_a = build_port(&mut game, a, 9, 10);
    let port_b = build_port(&mut game, b, 30, 10);
    game.add_execution(Box::new(TradeShipExecution::new(a, port_a, port_b)));
    run(&mut game, 5);
    assert_eq!(game.player(a).unit_count(UnitType::TradeShip), 1);
    // Embargo: the owner can no longer trade with the destination
    game.player_mut(b).add_embargo(a, false);
    run(&mut game, 2);
    assert_eq!(game.player(a).unit_count(UnitType::TradeShip), 0);
    assert_eq!(game.player(a).trade_gold(), 0);
}

fn spawn_warship(game: &mut GameImpl, owner: SmallId, patrol: TileRef) -> UnitId {
    game.add_execution(Box::new(WarshipExecution::new(owner, patrol)));
    run(game, 2);
    game.player(owner).units_of(UnitType::Warship)[0]
}

#[test]
fn warship_needs_a_port_and_then_patrols() {
    let (mut game, a, _) = make_game();
    let patrol = game.tile_ref(12, 10);
    // No port: nothing is built
    game.add_execution(Box::new(WarshipExecution::new(a, patrol)));
    run(&mut game, 2);
    assert_eq!(game.player(a).unit_count(UnitType::Warship), 0);

    build_port(&mut game, a, 9, 10);
    let warship = spawn_warship(&mut game, a, patrol);
    let start = game.unit_any(warship).tile();
    run(&mut game, 60);
    assert_ne!(game.unit_any(warship).tile(), start);
    let tile = game.unit_any(warship).tile();
    assert!(game.map.is_water(tile));
}

#[test]
fn move_warship_respects_water_components() {
    let (mut game, a, _) = make_game();
    build_port(&mut game, a, 9, 10);
    let patrol = game.tile_ref(12, 10);
    let warship = spawn_warship(&mut game, a, patrol);
    let original = game.unit_any(warship).warship_state().patrol_tile;

    // The inland lake is a different water component: the order is ignored
    let lake = game.tile_ref(4, 6);
    assert!(game.map.is_water(lake));
    game.add_execution(Box::new(MoveWarshipExecution::new(
        a,
        vec![warship as i64],
        lake,
    )));
    run(&mut game, 1);
    assert_eq!(game.unit_any(warship).warship_state().patrol_tile, original);

    // A channel tile moves the patrol point and clears the current target
    let channel = game.tile_ref(20, 3);
    game.add_execution(Box::new(MoveWarshipExecution::new(
        a,
        vec![warship as i64, warship as i64, 9999],
        channel,
    )));
    run(&mut game, 1);
    assert_eq!(
        game.unit_any(warship).warship_state().patrol_tile,
        Some(channel)
    );
}
