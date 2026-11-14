// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationUtils.ts: helpers shared by the nation behaviours.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::calculate_bounding_box;
use crate::Game::Game::{Cell, Difficulty, GameMode, Gold, PlayerType, SmallId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// `randTerritoryTileArray(random, mg, player, numTiles)`.
pub fn rand_territory_tile_array(
    random: &mut PseudoRandom,
    game: &GameImpl,
    player: SmallId,
    num_tiles: usize,
) -> Vec<TileRef> {
    let bounding_box = calculate_bounding_box(&game.map, game.player(player).border_tiles().iter());
    let mut tiles = Vec::new();
    for _ in 0..num_tiles {
        if let Some(tile) = rand_territory_tile(random, game, player, bounding_box) {
            tiles.push(tile);
        }
    }
    tiles
}

fn rand_territory_tile(
    random: &mut PseudoRandom,
    game: &GameImpl,
    player: SmallId,
    bounding_box: Option<(Cell, Cell)>,
) -> Option<TileRef> {
    // Sample inside the bounding box first (fast, usually good enough).
    for _ in 0..100 {
        let Some((min, max)) = bounding_box else {
            // Upstream's infinite box yields NaN coordinates: two draws, never on the map.
            random.next();
            random.next();
            continue;
        };
        let rand_x = random.next_int(min.x as f64, max.x as f64) as i32;
        let rand_y = random.next_int(min.y as f64, max.y as f64) as i32;
        if !game.is_on_map(Cell::new(rand_x, rand_y)) {
            continue;
        }
        let rand_tile = game.tile_ref(rand_x, rand_y);
        if game.owner_id(rand_tile) == player {
            return Some(rand_tile);
        }
    }

    let owned = game.player(player).num_tiles_owned();
    if owned > 0 && owned <= 100 {
        let tiles: Vec<TileRef> = game.player(player).tiles().iter().collect();
        return Some(*random.rand_element(&tiles));
    }
    None
}

/// `findJuiciestTarget(game, candidates)`: ranks by structures, troop-cap headroom and tiles.
pub fn find_juiciest_target(game: &GameImpl, candidates: &[SmallId]) -> Option<SmallId> {
    if candidates.is_empty() {
        return None;
    }

    struct Stat {
        player: SmallId,
        structure_count: f64,
        troop_gap_ratio: f64,
        tiles: f64,
    }
    let stats: Vec<Stat> = candidates
        .iter()
        .map(|&p| {
            // Defense posts and missile silos are defensive, not a prize.
            let structure_count = game.player(p).units().iter().fold(0.0, |sum, &u| {
                let unit = game.unit_any(u);
                let t = unit.unit_type();
                if t.is_structure() && t != UnitType::DefensePost && t != UnitType::MissileSilo {
                    sum + unit.level() as f64
                } else {
                    sum
                }
            });
            let max_troops = game.player(p).max_troops();
            let troop_gap_ratio = if max_troops > 0.0 {
                1.0 - game.player(p).troops() / max_troops
            } else {
                0.0
            };
            Stat {
                player: p,
                structure_count,
                troop_gap_ratio,
                tiles: game.player(p).num_tiles_owned() as f64,
            }
        })
        .collect();

    let normalize = |value: f64, values: &[f64]| -> f64 {
        let min = values.iter().copied().fold(f64::INFINITY, f64::min);
        let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        if max > min {
            (value - min) / (max - min)
        } else {
            0.0
        }
    };
    let structure_counts: Vec<f64> = stats.iter().map(|s| s.structure_count).collect();
    let troop_gap_ratios: Vec<f64> = stats.iter().map(|s| s.troop_gap_ratio).collect();
    let tile_counts: Vec<f64> = stats.iter().map(|s| s.tiles).collect();

    let mut best: Option<SmallId> = None;
    let mut best_score = f64::NEG_INFINITY;
    for s in &stats {
        let juiciness = normalize(s.structure_count, &structure_counts)
            + normalize(s.troop_gap_ratio, &troop_gap_ratios)
            + normalize(s.tiles, &tile_counts);
        if juiciness > best_score {
            best_score = juiciness;
            best = Some(s.player);
        }
    }
    best
}

/// Starting gold from which nations place many structures at once, ballooning their max troops.
const HIGH_STARTING_GOLD: Gold = 3_000_000;

/// `hasHighStartingGold(game, player)`.
pub fn has_high_starting_gold(game: &GameImpl, player: SmallId) -> bool {
    let p = game.player(player);
    let info = p.info();
    game.config()
        .starting_gold(info.player_type, info.is_lobby_creator)
        >= HIGH_STARTING_GOLD
}

/// Most minutes of income a nation saves up for, keyed by difficulty (infinite when unbounded).
fn save_up_horizon_minutes(difficulty: Difficulty) -> f64 {
    match difficulty {
        Difficulty::Easy | Difficulty::Medium => f64::INFINITY,
        Difficulty::Hard => 5.0,
        Difficulty::Impossible => 6.0,
    }
}

/// `isSaveUpWithinReach(game, player, target)`: whether `target` gold is close enough to save up for.
pub fn is_save_up_within_reach(game: &GameImpl, player: SmallId, target: Gold) -> bool {
    let horizon = save_up_horizon_minutes(game.config().game_config().difficulty);
    let missing = (target - game.player(player).gold()) as f64;
    if horizon == f64::INFINITY || missing <= 0.0 {
        return true;
    }
    // Income grows about linearly over a game, so the current rate is about twice the average.
    let minutes = f64::max(1.0, game.ticks() as f64 / 600.0);
    let income_per_minute = (2.0 * game.player(player).gold_earned() as f64) / minutes;
    missing <= income_per_minute * horizon
}

/// How many times the runner-up's land the leader must own to count as a runaway leader.
fn runaway_lead_factor(difficulty: Difficulty) -> Option<f64> {
    match difficulty {
        Difficulty::Easy => None,
        Difficulty::Medium => Some(3.0),
        Difficulty::Hard => Some(2.0),
        Difficulty::Impossible => Some(1.5),
    }
}

/// `findRunawayLeader(game)`: the FFA player (not a bot) owning far more land than the runner-up.
pub fn find_runaway_leader(game: &GameImpl) -> Option<SmallId> {
    let config = game.config().game_config();
    if config.game_mode != GameMode::FFA {
        return None;
    }
    let factor = runaway_lead_factor(config.difficulty)?;
    let mut leader: Option<SmallId> = None;
    let mut runner_up: Option<SmallId> = None;
    let tiles = |p: SmallId| game.player(p).num_tiles_owned();
    for p in game.players() {
        if game.player(p).player_type() == PlayerType::Bot {
            continue;
        }
        if leader.is_none_or(|l| tiles(p) > tiles(l)) {
            runner_up = leader;
            leader = Some(p);
        } else if runner_up.is_none_or(|r| tiles(p) > tiles(r)) {
            runner_up = Some(p);
        }
    }
    let (leader, runner_up) = (leader?, runner_up?);
    if tiles(leader) as f64 >= tiles(runner_up) as f64 * factor {
        Some(leader)
    } else {
        None
    }
}
