// SPDX-License-Identifier: AGPL-3.0-only
//! Port of `handleUnits` of src/core/execution/NationExecution.ts and src/core/execution/nation/structureSpawnTileValue.ts (builds before PR 3152): nations place structures from an inline build order.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::PseudoRandom::PseudoRandom;
use crate::Execution::ConstructionExecution::ConstructionExecution;
use crate::Execution::Nation::NationUtils::rand_territory_tile_array;
use crate::Execution::Nation::NationWarshipBehavior::NationWarshipBehavior;
use crate::Execution::Nation::SharedRandom;
use crate::Execution::Util::{closest_tile, closest_two_tiles};
use crate::Game::Game::{GameMode, PlayerType, Relation, SmallId, UnitType};
use crate::Game::GameImpl::GameImpl;
use crate::Game::GameMap::TileRef;

/// `handleUnits()`: the first affordable item of the build order is queued.
pub fn handle_units(
    game: &mut GameImpl,
    player: SmallId,
    random: &SharedRandom,
    warship: &mut NationWarshipBehavior,
) -> bool {
    let has_coastal_tiles = has_coastal_tiles(game, player);
    let is_team_game = game.config().game_config().game_mode == GameMode::Team;
    maybe_spawn_structure(game, player, random, UnitType::City, &|n| n)
        || maybe_spawn_structure(game, player, random, UnitType::Port, &|n| n)
        || warship.maybe_spawn_warship(game)
        || maybe_spawn_structure(game, player, random, UnitType::Factory, &|n| {
            if has_coastal_tiles { n * 3 } else { n }
        })
        || maybe_spawn_structure(game, player, random, UnitType::DefensePost, &|n| {
            (n + 2) * (n + 2)
        })
        || maybe_spawn_structure(game, player, random, UnitType::SAMLauncher, &|n| {
            if is_team_game { n } else { n * n }
        })
        || maybe_spawn_structure(game, player, random, UnitType::MissileSilo, &|n| n * n)
}

fn has_coastal_tiles(game: &GameImpl, player: SmallId) -> bool {
    game.player(player)
        .border_tiles()
        .iter()
        .any(|t| game.map.is_ocean_shore(t))
}

fn maybe_spawn_structure(
    game: &mut GameImpl,
    player: SmallId,
    random: &SharedRandom,
    unit_type: UnitType,
    multiplier: &dyn Fn(i64) -> i64,
) -> bool {
    let owned = game.player(player).units_owned(unit_type);
    let perceived_cost_multiplier = multiplier(owned + 1);
    let real_cost = game.player(player).unit_cost(unit_type, 0);
    let perceived_cost = real_cost * perceived_cost_multiplier;
    if game.player(player).gold() < perceived_cost {
        return false;
    }
    let Some(tile) = structure_spawn_tile(game, player, random, unit_type) else {
        return false;
    };
    if game
        .player(player)
        .can_build(unit_type, tile, None)
        .is_none()
    {
        return false;
    }
    game.add_execution(Box::new(ConstructionExecution::new(
        player, unit_type, tile, None, None,
    )));
    true
}

fn structure_spawn_tile(
    game: &GameImpl,
    player: SmallId,
    random: &SharedRandom,
    unit_type: UnitType,
) -> Option<TileRef> {
    let tiles = if unit_type == UnitType::Port {
        rand_coastal_tile_array(game, player, &mut random.borrow_mut(), 25)
    } else {
        rand_territory_tile_array(&mut random.borrow_mut(), game, player, 25)
    };
    if tiles.is_empty() {
        return None;
    }
    let value = TileValue::new(game, player, unit_type)?;
    let mut best_tile: Option<TileRef> = None;
    let mut best_value = 0.0;
    for t in tiles {
        let v = value.eval(game, t);
        if v <= best_value && best_tile.is_some() {
            continue;
        }
        if game.player(player).can_build(unit_type, t, None).is_none() {
            continue;
        }
        best_tile = Some(t);
        best_value = v;
    }
    best_tile
}

fn rand_coastal_tile_array(
    game: &GameImpl,
    player: SmallId,
    random: &mut PseudoRandom,
    num_tiles: usize,
) -> Vec<TileRef> {
    let tiles: Vec<TileRef> = game
        .player(player)
        .border_tiles()
        .iter()
        .filter(|&t| game.map.is_ocean_shore(t))
        .collect();
    if tiles.len() <= num_tiles {
        return tiles;
    }
    let mut remaining: crate::Compat::OrderedSet::OrderedSet<TileRef> = tiles.into_iter().collect();
    let mut out = Vec::new();
    for _ in 0..num_tiles {
        let t = *random.rand_from_set(&remaining);
        remaining.remove(&t);
        out.push(t);
    }
    out
}

/// `structureSpawnTileValue(mg, player, type)`: the scoring function of one structure type.
struct TileValue {
    unit_type: UnitType,
    border_tiles: Vec<TileRef>,
    other_tiles: Vec<TileRef>,
    protect_tiles: Vec<TileRef>,
    border_spacing: f64,
    structure_spacing: f64,
    range_squared: f64,
    player: SmallId,
}

impl TileValue {
    /// None when the type has no valid placement (a defense post without hostile neighbours).
    fn new(game: &GameImpl, player: SmallId, unit_type: UnitType) -> Option<Self> {
        let border_tiles: Vec<TileRef> = game.player(player).border_tiles().iter().collect();
        let mut other_tiles: Vec<TileRef> = Vec::new();
        for &u in game.player(player).units_of(unit_type).iter() {
            let tile = game.unit_any(u).tile();
            if !other_tiles.contains(&tile) {
                other_tiles.push(tile);
            }
        }
        let border_spacing = game.config().nuke_magnitudes(UnitType::AtomBomb).outer;
        let structure_spacing = border_spacing * 2.0;
        let mut protect_tiles: Vec<TileRef> = Vec::new();
        let mut range_squared = 0.0;
        match unit_type {
            UnitType::City | UnitType::Factory | UnitType::MissileSilo | UnitType::Port => {}
            UnitType::DefensePost => {
                let has_hostile_neighbor = game.player(player).nearby().into_iter().any(|n| {
                    n != 0
                        && !game.player(player).is_friendly(n, false)
                        && game.player(n).player_type() != PlayerType::Bot
                });
                if !has_hostile_neighbor {
                    return None;
                }
            }
            UnitType::SAMLauncher => {
                for &u in game.player(player).units() {
                    let unit = game.unit_any(u);
                    if matches!(
                        unit.unit_type(),
                        UnitType::City | UnitType::Factory | UnitType::MissileSilo | UnitType::Port
                    ) && !protect_tiles.contains(&unit.tile())
                    {
                        protect_tiles.push(unit.tile());
                    }
                }
                let range = game.config().default_sam_range();
                range_squared = range * range;
            }
            other => panic!("Value function not implemented for {}", other.as_str()),
        }
        Some(Self {
            unit_type,
            border_tiles,
            other_tiles,
            protect_tiles,
            border_spacing,
            structure_spacing,
            range_squared,
            player,
        })
    }

    /// Manhattan distance to the closest other structure of the type, capped at the structure spacing.
    fn spacing_to_others(&self, game: &GameImpl, tile: TileRef) -> Option<f64> {
        let others = self.other_tiles.iter().copied().filter(|&t| t != tile);
        let closest = closest_two_tiles(&game.map, others, [tile])?;
        let d = game.map.manhattan_dist(closest.0, tile) as f64;
        Some(f64::min(d, self.structure_spacing))
    }

    fn eval(&self, game: &GameImpl, tile: TileRef) -> f64 {
        let mg = &game.map;
        let mut w = 0.0;
        match self.unit_type {
            UnitType::City | UnitType::Factory | UnitType::MissileSilo => {
                w += mg.magnitude(tile) as f64;
                let (_, closest_border_dist) =
                    closest_tile(mg, self.border_tiles.iter().copied(), tile);
                w += f64::min(closest_border_dist, self.border_spacing);
                if let Some(d) = self.spacing_to_others(game, tile) {
                    w += d;
                }
            }
            UnitType::Port => {
                let others = self.other_tiles.iter().copied().filter(|&t| t != tile);
                let (_, closest_other_dist) = closest_tile(mg, others, tile);
                w += f64::min(closest_other_dist, self.structure_spacing);
            }
            UnitType::DefensePost => {
                w += mg.magnitude(tile) as f64;
                let (closest, closest_border_dist) =
                    closest_tile(mg, self.border_tiles.iter().copied(), tile);
                if let Some(closest) = closest {
                    w += f64::max(
                        0.0,
                        self.border_spacing - (self.border_spacing - closest_border_dist).abs(),
                    );
                    let mut neighbors: Vec<SmallId> = Vec::new();
                    for t in mg.neighbors(closest) {
                        if !mg.is_land(t) {
                            continue;
                        }
                        let id = mg.owner_id(t);
                        if id == self.player {
                            continue;
                        }
                        if id == 0 || game.player(id).player_type() == PlayerType::Bot {
                            continue;
                        }
                        if !neighbors.contains(&id) {
                            neighbors.push(id);
                        }
                    }
                    for n in neighbors {
                        let relation = game.player(self.player).relation(n) as i32;
                        w += self.border_spacing * ((Relation::Friendly as i32 - relation) as f64);
                    }
                }
                if let Some(d) = self.spacing_to_others(game, tile) {
                    w += d;
                }
            }
            UnitType::SAMLauncher => {
                w += mg.magnitude(tile) as f64;
                if let Some(closest_border) =
                    closest_two_tiles(mg, self.border_tiles.iter().copied(), [tile])
                {
                    let d = mg.manhattan_dist(closest_border.0, tile) as f64;
                    w += f64::min(d, self.border_spacing);
                }
                if let Some(d) = self.spacing_to_others(game, tile) {
                    w += d;
                }
                for &maybe_protected in &self.protect_tiles {
                    let distance_squared = mg.euclidean_dist_squared(tile, maybe_protected) as f64;
                    if distance_squared > self.range_squared {
                        continue;
                    }
                    w += self.structure_spacing;
                }
            }
            _ => unreachable!("checked in new"),
        }
        w
    }
}
