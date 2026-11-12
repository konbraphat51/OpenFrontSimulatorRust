// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts (part: placement): structure tile scoring.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationStructureBehavior {
    pub(super) fn structure_spawn_tile(&mut self, game: &GameImpl, t: UnitType) -> Option<TileRef> {
        let tiles = if t == UnitType::Port {
            self.rand_coastal_tile_array(game, 25)
        } else {
            rand_territory_tile_array(&mut self.rnd(), game, self.player, 25)
        };
        if tiles.is_empty() {
            return None;
        }
        let value_function = self.structure_spawn_tile_value(game, t)?;
        let mut best_tile: Option<TileRef> = None;
        let mut best_value = 0.0;
        for tile in tiles {
            let v = self.eval_value(game, &value_function, tile);
            if v <= best_value && best_tile.is_some() {
                continue;
            }
            if game.player(self.player).can_build(t, tile, None).is_none() {
                continue;
            }
            // Found a better tile.
            best_tile = Some(tile);
            best_value = v;
        }
        best_tile
    }

    /// Samples shore tiles adjacent to water reachable by another player (so trading is possible).
    pub(super) fn rand_coastal_tile_array(
        &self,
        game: &GameImpl,
        num_tiles: usize,
    ) -> Vec<TileRef> {
        if Self::legacy31(game) {
            let tiles: Vec<TileRef> = game
                .player(self.player)
                .border_tiles()
                .iter()
                .filter(|&t| game.map.is_ocean_shore(t))
                .collect();
            return self.array_sampler(tiles, num_tiles);
        }
        let shared = self.shared_water_components.as_ref();
        let difficulty = Self::difficulty(game);
        let tiles: Vec<TileRef> = game
            .player(self.player)
            .border_tiles()
            .iter()
            .filter(|&t| {
                if !game.map.is_shore(t) {
                    return false;
                }
                let Some(shared) = shared else {
                    return false;
                };
                for neighbor in game.map.neighbors(t) {
                    if !game.map.is_water(neighbor) {
                        continue;
                    }
                    // Ocean is always shared: any ocean neighbour makes the tile a valid port site.
                    if game.map.is_ocean(neighbor) {
                        return true;
                    }
                    let Some(comp) = game.water_manager.get_water_component(&game.map, neighbor)
                    else {
                        continue;
                    };
                    if !shared.contains(&(comp as i64)) {
                        continue;
                    }
                    // Skip tiny lakes (not on Easy).
                    if difficulty != Difficulty::Easy && game.has(Change::NationSmallLakePorts4429)
                    {
                        let size = game
                            .water_manager
                            .get_water_component_size(&game.map, neighbor);
                        if size.is_some_and(|s| s < MIN_PORT_WATER_COMPONENT_SIZE) {
                            continue;
                        }
                    }
                    return true;
                }
                false
            })
            .collect();
        self.array_sampler(tiles, num_tiles)
    }

    /// `arraySampler`: all elements, or `sample_size` random ones drawn without replacement.
    pub(super) fn array_sampler<T>(&self, mut a: Vec<T>, sample_size: usize) -> Vec<T> {
        if a.len() <= sample_size {
            return a;
        }
        let mut out = Vec::with_capacity(sample_size);
        for _ in 0..sample_size {
            // randFromSet picks the n-th element in insertion order; removing keeps that order.
            let index = self.rnd().rand_index(a.len());
            out.push(a.remove(index));
        }
        out
    }

    pub(super) fn structure_spawn_tile_value(
        &mut self,
        game: &GameImpl,
        t: UnitType,
    ) -> Option<SpawnValue> {
        Some(match t {
            UnitType::City => self.city_value(game),
            UnitType::MissileSilo => self.missile_silo_value(game),
            UnitType::Factory => self.factory_value(game),
            UnitType::Port => self.port_value(game),
            UnitType::SAMLauncher => self.sam_launcher_value(game),
            UnitType::DefensePost => return self.defense_post_value(game),
            _ => panic!("Value function not implemented for {}", t.as_str()),
        })
    }

    /// Defense posts (before v0.31): none without a hostile non-bot neighbour with more troops.
    pub(super) fn defense_post_value(&self, game: &GameImpl) -> Option<SpawnValue> {
        let me = game.player(self.player);
        let has_hostile_neighbor = me.nearby().into_iter().any(|n| {
            n != 0
                && !me.is_friendly(n, false)
                && game.player(n).player_type() != PlayerType::Bot
                && game.player(n).troops() > me.troops()
        });
        if !has_hostile_neighbor {
            return None;
        }
        Some(SpawnValue::DefensePost {
            other_tiles: self.unit_tiles(game, UnitType::DefensePost),
            spacing: Self::spacing(game),
        })
    }

    pub(super) fn unit_tiles(&self, game: &GameImpl, t: UnitType) -> Vec<TileRef> {
        game.player(self.player)
            .units_of(t)
            .iter()
            .map(|&u| game.unit_any(u).tile)
            .collect()
    }

    /// Prefers high elevation, distance from the border, and spacing from same-type structures.
    pub(super) fn missile_silo_value(&self, game: &GameImpl) -> SpawnValue {
        SpawnValue::MissileSilo {
            other_tiles: self.unit_tiles(game, UnitType::MissileSilo),
            spacing: Self::spacing(game),
        }
    }

    /// Prefers spacing from other ports.
    pub(super) fn port_value(&self, game: &GameImpl) -> SpawnValue {
        SpawnValue::Port {
            other_tiles: self.unit_tiles(game, UnitType::Port),
            cap: Self::legacy31(game).then(|| Self::spacing(game).structure),
        }
    }

    /// Prefers elevation, spacing and (by difficulty) rail connectivity.
    pub(super) fn factory_value(&mut self, game: &GameImpl) -> SpawnValue {
        let spacing = Self::spacing(game);
        let station_range = game.config().train_station_max_range();
        let use_connection_score = self.should_use_connectivity_score(Self::difficulty(game));
        let connectivity = self.connectivity(game, use_connection_score);
        SpawnValue::Factory {
            other_tiles: self.unit_tiles(game, UnitType::Factory),
            city_tiles: self.unit_tiles(game, UnitType::City),
            spacing,
            station_range,
            connectivity,
        }
    }

    /// Prefers interior positions, spacing from cities and factories, and rail connectivity.
    pub(super) fn city_value(&mut self, game: &GameImpl) -> SpawnValue {
        let spacing = Self::spacing(game);
        let use_connection_score = self.should_use_connectivity_score(Self::difficulty(game));
        let connectivity = self.connectivity(game, use_connection_score);
        SpawnValue::City {
            other_tiles: self.unit_tiles(game, UnitType::City),
            factory_tiles: self.unit_tiles(game, UnitType::Factory),
            spacing,
            connectivity,
        }
    }

    pub(super) fn connectivity(
        &mut self,
        game: &GameImpl,
        use_connection_score: bool,
    ) -> Option<ConnectivityScore> {
        if !use_connection_score {
            return None;
        }
        let station_range = game.config().train_station_max_range();
        let stations = self.get_or_build_reachable_stations(game).clone();
        Some(ConnectivityScore {
            stations,
            min_range_squared: game.config().train_station_min_range().powi(2),
            station_range_squared: station_range * station_range,
        })
    }

    /// Whether to use connectivity scoring for factories and cities (consumes a draw on every difficulty).
    pub(super) fn should_use_connectivity_score(&self, difficulty: Difficulty) -> bool {
        let random_chance = match difficulty {
            Difficulty::Easy => 0.0,
            Difficulty::Medium => 60.0,
            Difficulty::Hard => 75.0,
            Difficulty::Impossible => 100.0,
        };
        self.rnd().next_int(0.0, 100.0) < random_chance
    }

    pub(super) fn get_or_build_reachable_stations(
        &mut self,
        game: &GameImpl,
    ) -> &Vec<StationEntry> {
        if self.reachable_stations_cache.is_none() {
            self.reachable_stations_cache = Some(self.build_reachable_stations(game));
        }
        self.reachable_stations_cache.as_ref().unwrap()
    }

    /// Trade-weighted station entries for connectivity scoring: own and non-embargoed non-bot neighbour structures.
    pub(super) fn build_reachable_stations(&self, game: &GameImpl) -> Vec<StationEntry> {
        let player = game.player(self.player);
        let is_lobby_creator = player.info().is_lobby_creator;

        // Unit to cluster lookup in one pass over all stations.
        let unit_to_cluster: HashMap<UnitId, Option<u32>> =
            game.rail_network.station_clusters().into_iter().collect();

        let train_gold = |rel: TrainRelation| -> f64 {
            game.config().train_gold(rel, 0.0, is_lobby_creator) as f64
        };
        let max_trade_gold = train_gold(TrainRelation::Ally).max(1.0);
        let mut result = Vec::new();

        // Own structures: weighted by "self" trade gold.
        let self_weight = train_gold(TrainRelation::Self_) / max_trade_gold;
        let types = [UnitType::City, UnitType::Port, UnitType::Factory];
        for unit in player.units_of_types(&types) {
            if let Some(&cluster) = unit_to_cluster.get(&unit) {
                result.push(StationEntry {
                    tile: game.unit_any(unit).tile,
                    cluster,
                    weight: self_weight,
                });
            }
        }

        // Neighbour structures: all non-embargoed non-bot neighbours.
        for neighbor in player.nearby() {
            if neighbor == 0 {
                continue;
            }
            if game.player(neighbor).player_type() == PlayerType::Bot {
                continue;
            }
            if !player.can_trade(neighbor) {
                continue;
            }
            let rel = if player.is_on_same_team(neighbor) {
                TrainRelation::Team
            } else if player.is_allied_with(neighbor) {
                TrainRelation::Ally
            } else {
                TrainRelation::Other
            };
            let weight = train_gold(rel) / max_trade_gold;
            for unit in game.player(neighbor).units_of_types(&types) {
                if let Some(&cluster) = unit_to_cluster.get(&unit) {
                    result.push(StationEntry {
                        tile: game.unit_any(unit).tile,
                        cluster,
                        weight,
                    });
                }
            }
        }

        result
    }

    /// Summed cluster-deduplicated connectivity weight of a candidate tile (best weight per cluster in range).
    pub(super) fn compute_connectivity_score(
        game: &GameImpl,
        tile: TileRef,
        c: &ConnectivityScore,
    ) -> f64 {
        let mut clusters_in_range: OrderedMap<u32, f64> = OrderedMap::new();
        let mut isolated_weight = 0.0;
        for station in &c.stations {
            let dist = game.map.euclidean_dist_squared(tile, station.tile) as f64;
            if dist < c.min_range_squared || dist > c.station_range_squared {
                continue;
            }
            match station.cluster {
                Some(cluster) => {
                    let best = clusters_in_range.get(&cluster).copied().unwrap_or(0.0);
                    clusters_in_range.insert(cluster, best.max(station.weight));
                }
                None => isolated_weight += station.weight,
            }
        }
        let mut score = isolated_weight;
        for cw in clusters_in_range.values() {
            score += *cw;
        }
        score
    }

    /// Prefers elevation, border distance, spacing, and proximity to protectable structures.
    pub(super) fn sam_launcher_value(&mut self, game: &GameImpl) -> SpawnValue {
        let player = game.player(self.player);
        let spacing = Self::spacing(game);
        let difficulty = Self::difficulty(game);
        let weight_by_level =
            difficulty == Difficulty::Hard || difficulty == Difficulty::Impossible;

        let mut protect_entries: Vec<(TileRef, f64)> = Vec::new();
        for &unit in player.units() {
            let u = game.unit_any(unit);
            if matches!(
                u.unit_type,
                UnitType::City | UnitType::Factory | UnitType::MissileSilo | UnitType::Port
            ) {
                protect_entries.push((u.tile, if weight_by_level { u.level as f64 } else { 1.0 }));
            }
        }
        let range = game.config().default_sam_range();
        let range_squared = range * range;

        let use_coverage_weighting =
            difficulty != Difficulty::Easy && self.rnd().next_int(0.0, 100.0) < 25.0;

        // Existing SAM coverage of each protectable structure.
        let mut structure_coverage: Option<HashMap<TileRef, f64>> = None;
        if use_coverage_weighting {
            let mut coverage = HashMap::new();
            let existing_sams = player.units_of(UnitType::SAMLauncher);
            for &(tile, _) in &protect_entries {
                let mut coverage_score = 0.0;
                for &sam in &existing_sams {
                    let sam_unit = game.unit_any(sam);
                    let sam_range = game.config().sam_range(sam_unit.level as f64);
                    let dist = game.map.euclidean_dist_squared(tile, sam_unit.tile) as f64;
                    if dist <= sam_range * sam_range {
                        coverage_score += sam_unit.level as f64;
                    }
                }
                coverage.insert(tile, coverage_score);
            }
            structure_coverage = Some(coverage);
        }

        SpawnValue::SamLauncher {
            other_tiles: self.unit_tiles(game, UnitType::SAMLauncher),
            spacing,
            difficulty,
            protect_entries,
            range_squared,
            structure_coverage,
        }
    }

    /// Evaluates a value function at a tile.
    pub(super) fn eval_value(&self, game: &GameImpl, value: &SpawnValue, tile: TileRef) -> f64 {
        let owner = game.player(self.player);
        let border_tiles = owner.border_tiles();
        let nearest_other = |tiles: &[TileRef]| {
            nearest_tile_dist(
                &game.map,
                tiles.iter().copied().filter(|&t| t != tile),
                tile,
            )
        };
        let border_term = |spacing: &Spacing| {
            // Clamped at borderSpacing, so the ring search may stop there.
            let closest_border_dist =
                nearest_tile_dist_capped(&game.map, border_tiles, tile, spacing.border as i64);
            closest_border_dist.min(spacing.border)
        };
        match value {
            SpawnValue::MissileSilo {
                other_tiles,
                spacing,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                w
            }
            SpawnValue::Port { other_tiles, cap } => {
                let mut w = 0.0;
                let d = nearest_other(other_tiles);
                w += cap.map_or(d, |c| d.min(c));
                w
            }
            SpawnValue::DefensePost {
                other_tiles,
                spacing,
            } => {
                let mut w = 0.0;
                // Prefer higher elevations.
                w += game.map.magnitude(tile) as f64;
                let (closest, closest_border_dist) =
                    crate::Execution::Util::closest_tile(&game.map, border_tiles.iter(), tile);
                if let Some(closest) = closest {
                    // Prefer to be borderSpacing tiles from the border.
                    w += f64::max(
                        0.0,
                        spacing.border - (spacing.border - closest_border_dist).abs(),
                    );
                    // Prefer adjacent players who are hostile and have more troops.
                    let mut neighbors: Vec<SmallId> = Vec::new();
                    for neighbor_tile in game.map.neighbors(closest) {
                        if !game.map.is_land(neighbor_tile) {
                            continue;
                        }
                        let id = game.map.owner_id(neighbor_tile);
                        if id == self.player || id == 0 {
                            continue;
                        }
                        if game.player(id).player_type() == PlayerType::Bot {
                            continue;
                        }
                        if game.player(id).troops() <= owner.troops() {
                            continue;
                        }
                        if !neighbors.contains(&id) {
                            neighbors.push(id);
                        }
                    }
                    for neighbor in neighbors {
                        w += spacing.border
                            * (Relation::Friendly as u8 as f64
                                - owner.relation(neighbor) as u8 as f64);
                    }
                }
                // Prefer to be away from other structures of the same type.
                let others: Vec<TileRef> =
                    other_tiles.iter().copied().filter(|&t| t != tile).collect();
                if let Some((x, _)) = crate::Execution::Util::closest_two_tiles(
                    &game.map,
                    others.iter().copied(),
                    [tile],
                ) {
                    let d = game.map.manhattan_dist(x, tile) as f64;
                    w += d.min(spacing.structure);
                }
                w
            }
            SpawnValue::Factory {
                other_tiles,
                city_tiles,
                spacing,
                station_range,
                connectivity,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(*station_range);
                }
                // Cross-type spacing: prefer to be away from cities.
                let d2 = nearest_tile_dist(&game.map, city_tiles.iter().copied(), tile);
                if d2 != f64::INFINITY {
                    w += d2.min(spacing.structure);
                }
                if let Some(c) = connectivity {
                    w += Self::compute_connectivity_score(game, tile, c) * spacing.structure;
                }
                w
            }
            SpawnValue::City {
                other_tiles,
                factory_tiles,
                spacing,
                connectivity,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                w += border_term(spacing);
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                // Cross-type spacing: prefer to be away from factories.
                let d2 = nearest_tile_dist(&game.map, factory_tiles.iter().copied(), tile);
                if d2 != f64::INFINITY {
                    w += d2.min(spacing.structure);
                }
                if let Some(c) = connectivity {
                    w += Self::compute_connectivity_score(game, tile, c) * spacing.structure;
                }
                w
            }
            SpawnValue::SamLauncher {
                other_tiles,
                spacing,
                difficulty,
                protect_entries,
                range_squared,
                structure_coverage,
            } => {
                let mut w = 0.0;
                w += game.map.magnitude(tile) as f64;
                // Infinity (farther than borderSpacing) still earns the clamped term; only an empty border earns nothing.
                if !border_tiles.is_empty() {
                    w += border_term(spacing);
                }
                let d = nearest_other(other_tiles);
                if d != f64::INFINITY {
                    w += d.min(spacing.structure);
                }
                // Prefer to be in range of other structures (skipped on Easy).
                if *difficulty != Difficulty::Easy {
                    for &(entry_tile, entry_weight) in protect_entries {
                        let distance_squared =
                            game.map.euclidean_dist_squared(tile, entry_tile) as f64;
                        if distance_squared > *range_squared {
                            continue;
                        }
                        if let Some(coverage_map) = structure_coverage {
                            let coverage = coverage_map.get(&entry_tile).copied().unwrap_or(0.0);
                            let coverage_weight = 1.0 / (1.0 + coverage);
                            w += spacing.structure * entry_weight * coverage_weight;
                        } else {
                            w += spacing.structure * entry_weight;
                        }
                    }
                }
                w
            }
        }
    }

    /// Spacing constants derived from the atom bomb range.
    pub(super) fn spacing(game: &GameImpl) -> Spacing {
        let border = game.config().nuke_magnitudes(UnitType::AtomBomb).outer;
        Spacing {
            border,
            structure: border * 2.0,
        }
    }
}
