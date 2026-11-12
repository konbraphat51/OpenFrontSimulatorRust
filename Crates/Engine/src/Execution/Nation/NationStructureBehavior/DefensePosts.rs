// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationStructureBehavior.ts (part: defenseposts): defense post placement.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationStructureBehavior {
    /// Land attacks (no source tile) on the nation: (attacker, troops) pairs.
    pub(super) fn incoming_land_attacks(
        &self,
        game: &GameImpl,
        all_attacks: bool,
    ) -> Vec<(SmallId, f64)> {
        game.player(self.player)
            .incoming_attacks()
            .into_iter()
            .filter(|&a| all_attacks || game.attack(a).source_tile().is_none())
            .map(|a| (game.attack(a).attacker(), game.attack(a).troops()))
            .collect()
    }

    pub(super) fn try_build_defense_post(&mut self, game: &mut GameImpl) -> bool {
        let difficulty = Self::difficulty(game);
        if difficulty == Difficulty::Easy {
            return false;
        }
        if difficulty == Difficulty::Medium && !self.rnd().chance(2.0) {
            return false;
        }

        let v2 = game.has(Change::NationsLikeHumans5899);
        let land_attacks = self.incoming_land_attacks(game, v2);
        if land_attacks.is_empty() {
            return false;
        }

        let our_troops = game.player(self.player).troops();
        if our_troops <= 0.0 {
            return false;
        }

        let incoming_troops = land_attacks.iter().fold(0.0, |sum, a| sum + a.1);
        let ratio = incoming_troops / our_troops;
        if ratio < UNDER_ATTACK_THREAT_RATIO {
            return false;
        }

        let allowed = if difficulty == Difficulty::Medium {
            1.0
        } else {
            (ratio / DEFENSE_POST_RATIO_PER_POST).ceil()
        };

        if v2 {
            let range = game.config().defense_post_range();
            let scaled_ratio = if difficulty == Difficulty::Medium {
                0.0
            } else {
                f64::min(ratio, DEFENSE_POST_FRONT_RATIO_CAP)
            };
            let min_dist = range
                * (DEFENSE_POST_FRONT_MIN_DIST + DEFENSE_POST_FRONT_DIST_PER_RATIO * scaled_ratio);
            let max_dist = min_dist + range * DEFENSE_POST_FRONT_BAND;
            let front_tiles = self.get_attack_front_tiles_v2(game);
            if self.count_defense_posts_near_front_v2(game, &front_tiles, max_dist, Some(allowed))
                >= allowed
            {
                return false;
            }
            return self.build_defense_post_behind(game, &front_tiles, min_dist, max_dist);
        }

        let front_tiles = self.get_attack_front_tiles(game, &land_attacks);
        if self.count_defense_posts_near_front(game, &front_tiles, Some(allowed)) >= allowed {
            return false;
        }

        let cost = self.cost(game, UnitType::DefensePost);
        if game.player(self.player).gold() < cost {
            return false;
        }

        let tiles = self.sample_tiles_near_front(game, &front_tiles, 25, UnitType::DefensePost);
        for tile in tiles {
            if game
                .player(self.player)
                .can_build(UnitType::DefensePost, tile, None)
                .is_none()
            {
                continue;
            }
            self.log.queue(
                game,
                self.player,
                Decision::Construct {
                    unit_type: UnitType::DefensePost,
                    tile,
                },
            );
            return true;
        }
        false
    }

    /// Our border tiles adjacent to a land attacker's territory, plus the landing frontier of boat attacks.
    pub(super) fn get_attack_front_tiles_v2(&self, game: &GameImpl) -> Vec<TileRef> {
        let attacks = game.player(self.player).incoming_attacks();
        let mut land_attackers: Vec<SmallId> = Vec::new();
        for &a in &attacks {
            if game.attack(a).source_tile().is_none() {
                let attacker = game.attack(a).attacker();
                if !land_attackers.contains(&attacker) {
                    land_attackers.push(attacker);
                }
            }
        }
        let mut front_tiles = self.border_tiles_facing(game, &land_attackers);
        // A land attacker's beachheads are already in its adjacency front above.
        for &a in &attacks {
            let attack = game.attack(a);
            if attack.source_tile().is_none() || land_attackers.contains(&attack.attacker()) {
                continue;
            }
            for &t in attack.border().iter() {
                if game.owner_id(t) == self.player {
                    front_tiles.push(t);
                }
            }
        }
        front_tiles
    }

    /// Our border tiles adjacent to land owned by any of `players`.
    pub(super) fn border_tiles_facing(&self, game: &GameImpl, players: &[SmallId]) -> Vec<TileRef> {
        let mut front_tiles = Vec::new();
        if players.is_empty() {
            return front_tiles;
        }
        let mut nbuf = [0; 4];
        game.player(self.player)
            .border_tiles()
            .for_each(|border_tile| {
                let n = game.map.neighbors4(border_tile, &mut nbuf);
                for &neighbor in &nbuf[..n] {
                    if players.contains(&game.owner_id(neighbor)) {
                        front_tiles.push(border_tile);
                        return;
                    }
                }
            });
        front_tiles
    }

    /// Counts defense posts within `range` of any front tile; `cap` stops the scan early.
    pub(super) fn count_defense_posts_near_front_v2(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        range: f64,
        cap: Option<f64>,
    ) -> f64 {
        if front_tiles.is_empty() {
            return 0.0;
        }
        let range_squared = range * range;
        let mut count = 0.0;
        for dp in game.player(self.player).units_of(UnitType::DefensePost) {
            let dp_tile = game.unit_any(dp).tile;
            for &front_tile in front_tiles {
                if game.map.euclidean_dist_squared(dp_tile, front_tile) as f64 <= range_squared {
                    count += 1.0;
                    if let Some(cap) = cap
                        && count >= cap
                    {
                        return count;
                    }
                    break;
                }
            }
        }
        count
    }

    pub(super) fn build_defense_post_behind(
        &mut self,
        game: &mut GameImpl,
        front_tiles: &[TileRef],
        min_dist: f64,
        max_dist: f64,
    ) -> bool {
        if game.player(self.player).gold() < self.cost(game, UnitType::DefensePost) {
            return false;
        }
        let Some(tile) = self.find_defense_post_tile(game, front_tiles, min_dist, max_dist) else {
            return false;
        };
        self.log.queue(
            game,
            self.player,
            Decision::Construct {
                unit_type: UnitType::DefensePost,
                tile,
            },
        );
        true
    }

    /// Finds an own tile where a defense post can be built at a distance in [min, max] from the nearest front tile.
    pub(super) fn find_defense_post_tile(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        min_dist: f64,
        max_dist: f64,
    ) -> Option<TileRef> {
        if front_tiles.is_empty() {
            return None;
        }
        let player = game.player(self.player);
        let range = game.config().defense_post_range() * 1.5;
        let spread_range_squared = range * range;
        let existing_dp_tiles: Vec<TileRef> = player
            .units_of(UnitType::DefensePost)
            .iter()
            .map(|&u| game.unit_any(u).tile)
            .collect();
        let near_existing_post = |t: TileRef| {
            existing_dp_tiles
                .iter()
                .any(|&dp| (game.map.euclidean_dist_squared(t, dp) as f64) < spread_range_squared)
        };
        let anchors: Vec<TileRef> = front_tiles
            .iter()
            .copied()
            .filter(|&ft| !near_existing_post(ft))
            .collect();
        if anchors.is_empty() {
            return None;
        }
        let min_squared = min_dist * min_dist;
        let max_squared = max_dist * max_dist;
        let search_radius = max_dist.ceil() as i32;
        for _ in 0..DEFENSE_POST_SAMPLE_ATTEMPTS {
            let anchor = *self.rnd().rand_element(&anchors);
            let ax = game.x(anchor) as i32;
            let ay = game.y(anchor) as i32;
            let x = self
                .rnd()
                .next_int((ax - search_radius) as f64, (ax + search_radius + 1) as f64)
                as i32;
            let y = self
                .rnd()
                .next_int((ay - search_radius) as f64, (ay + search_radius + 1) as f64)
                as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let t = game.tile_ref(x, y);
            if game.owner_id(t) != self.player {
                continue;
            }
            if near_existing_post(t) {
                continue;
            }
            let mut nearest_squared = f64::INFINITY;
            for &ft in front_tiles {
                nearest_squared =
                    nearest_squared.min(game.map.euclidean_dist_squared(t, ft) as f64);
                if nearest_squared < min_squared {
                    break;
                }
            }
            if nearest_squared < min_squared || nearest_squared > max_squared {
                continue;
            }
            if player.can_build(UnitType::DefensePost, t, None).is_none() {
                continue;
            }
            return Some(t);
        }
        None
    }

    /// Hard and Impossible: covers the border with a hostile (recently attacking) neighbour before it attacks again.
    pub(super) fn maybe_build_proactive_defense_post(&mut self, game: &mut GameImpl) -> bool {
        let config = game.config();
        let difficulty = config.game_config().difficulty;
        let Some(hostile_troop_ratio) = proactive_defense_post_hostile_troop_ratio(difficulty)
        else {
            return false;
        };
        if config.is_unit_disabled(UnitType::DefensePost) {
            return false;
        }
        let city_count = self.city_count(game);
        if city_count < PROACTIVE_DEFENSE_POST_MIN_CITIES {
            return false;
        }
        let player = game.player(self.player);
        if player.units_of(UnitType::DefensePost).len() as f64
            >= city_count * PROACTIVE_DEFENSE_POSTS_PER_CITY
        {
            return false;
        }
        if player.gold() < self.cost(game, UnitType::DefensePost) {
            return false;
        }

        let min_troops = player.troops() * hostile_troop_ratio;
        let mut threats: Vec<SmallId> = Vec::new();
        for neighbor in player.nearby() {
            if neighbor == 0 || game.player(neighbor).player_type() == PlayerType::Bot {
                continue;
            }
            if player.is_friendly(neighbor, false) {
                continue;
            }
            if player.relation(neighbor) == Relation::Hostile
                && game.player(neighbor).troops() >= min_troops
                && !threats.contains(&neighbor)
            {
                threats.push(neighbor);
            }
        }
        if threats.is_empty() {
            return false;
        }

        let range = config.defense_post_range();
        let front_tiles = self.border_tiles_facing(game, &threats);
        if self.count_defense_posts_near_front_v2(
            game,
            &front_tiles,
            range * 1.5,
            Some(threats.len() as f64),
        ) >= threats.len() as f64
        {
            return false;
        }
        self.build_defense_post_behind(
            game,
            &front_tiles,
            range * PROACTIVE_DEFENSE_POST_MIN_DIST,
            range * PROACTIVE_DEFENSE_POST_MAX_DIST,
        )
    }

    pub(super) fn defense_post_needed(&self, game: &GameImpl) -> bool {
        if Self::difficulty(game) == Difficulty::Easy {
            return false;
        }
        let land_attacks =
            self.incoming_land_attacks(game, game.has(Change::NationsLikeHumans5899));
        if land_attacks.is_empty() {
            return false;
        }
        let our_troops = game.player(self.player).troops();
        if our_troops <= 0.0 {
            return false;
        }
        let incoming_troops = land_attacks.iter().fold(0.0, |sum, a| sum + a.1);
        incoming_troops / our_troops >= UNDER_ATTACK_THREAT_RATIO
    }

    /// Our border tiles adjacent to a tile owned by any of the attacking players.
    pub(super) fn get_attack_front_tiles(
        &self,
        game: &GameImpl,
        land_attacks: &[(SmallId, f64)],
    ) -> Vec<TileRef> {
        let attackers: HashSet<SmallId> = land_attacks.iter().map(|a| a.0).collect();
        if attackers.is_empty() {
            return Vec::new();
        }
        let mut front_tiles = Vec::new();
        let mut nbuf = [0; 4];
        game.player(self.player)
            .border_tiles()
            .for_each(|border_tile| {
                let n = game.map.neighbors4_nswe(border_tile, &mut nbuf);
                for &neighbor in &nbuf[..n] {
                    if attackers.contains(&game.owner_id(neighbor)) {
                        front_tiles.push(border_tile);
                        return;
                    }
                }
            });
        front_tiles
    }

    /// Counts defense posts within 1.5 x borderSpacing of any front tile; `cap` stops the scan early.
    pub(super) fn count_defense_posts_near_front(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        cap: Option<f64>,
    ) -> f64 {
        if front_tiles.is_empty() {
            return 0.0;
        }
        let border_spacing = Self::spacing(game).border;
        let range_squared = (border_spacing * 1.5).powi(2);

        let mut count = 0.0;
        for dp in game.player(self.player).units_of(UnitType::DefensePost) {
            let dp_tile = game.unit_any(dp).tile;
            for &front_tile in front_tiles {
                if game.map.euclidean_dist_squared(dp_tile, front_tile) as f64 <= range_squared {
                    count += 1.0;
                    if let Some(cap) = cap
                        && count >= cap
                    {
                        return count;
                    }
                    break;
                }
            }
        }
        count
    }

    /// Samples territory tiles for defense-post placement around the attack front (see upstream doc).
    pub(super) fn sample_tiles_near_front(
        &self,
        game: &GameImpl,
        front_tiles: &[TileRef],
        count: usize,
        unit_type: UnitType,
    ) -> Vec<TileRef> {
        if front_tiles.is_empty() {
            return Vec::new();
        }

        let border_spacing = Self::spacing(game).border;
        let search_radius = (border_spacing * 1.5).ceil() as i32;
        let min_border_dist = (border_spacing * 0.75).ceil();
        let max_border_dist = (border_spacing * 1.5).ceil();
        let player = game.player(self.player);

        // Spread: prefer front tiles far from existing defense posts.
        let spread_range_squared = (border_spacing * 1.5).powi(2);
        let existing_dp_tiles: Vec<TileRef> = player
            .units_of(UnitType::DefensePost)
            .iter()
            .map(|&u| game.unit_any(u).tile)
            .collect();

        let anchors: Vec<TileRef> = if !existing_dp_tiles.is_empty() {
            let filtered: Vec<TileRef> = front_tiles
                .iter()
                .copied()
                .filter(|&ft| {
                    !existing_dp_tiles.iter().any(|&dp| {
                        (game.map.euclidean_dist_squared(ft, dp) as f64) < spread_range_squared
                    })
                })
                .collect();
            if filtered.is_empty() {
                front_tiles.to_vec()
            } else {
                filtered
            }
        } else {
            front_tiles.to_vec()
        };

        let mut result: Vec<TileRef> = Vec::new();
        let mut attempt = 0;
        while attempt < count * 6 && result.len() < count {
            attempt += 1;
            let anchor = *self.rnd().rand_element(&anchors);
            let ax = game.x(anchor) as i32;
            let ay = game.y(anchor) as i32;
            let x = self
                .rnd()
                .next_int((ax - search_radius) as f64, (ax + search_radius + 1) as f64)
                as i32;
            let y = self
                .rnd()
                .next_int((ay - search_radius) as f64, (ay + search_radius + 1) as f64)
                as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let t = game.tile_ref(x, y);
            if game.owner_id(t) != self.player {
                continue;
            }
            // Only "inside [min, max]" matters, so the search is capped at max.
            let border_dist = nearest_tile_dist_capped(
                &game.map,
                player.border_tiles(),
                t,
                max_border_dist as i64,
            );
            if border_dist < min_border_dist || border_dist > max_border_dist {
                continue;
            }
            if player.can_build(unit_type, t, None).is_none() {
                continue;
            }
            result.push(t);
        }

        if !result.is_empty() {
            return result;
        }

        // Fallback: relax the border-depth constraint (territory too small for the depth ring).
        let mut fallback: Vec<TileRef> = Vec::new();
        let mut attempt = 0;
        while attempt < count * 4 && fallback.len() < count {
            attempt += 1;
            let anchor = *self.rnd().rand_element(&anchors);
            let ax = game.x(anchor) as i32;
            let ay = game.y(anchor) as i32;
            let x = self
                .rnd()
                .next_int((ax - search_radius) as f64, (ax + search_radius + 1) as f64)
                as i32;
            let y = self
                .rnd()
                .next_int((ay - search_radius) as f64, (ay + search_radius + 1) as f64)
                as i32;
            if !game.is_valid_coord(x, y) {
                continue;
            }
            let t = game.tile_ref(x, y);
            if game.owner_id(t) != self.player {
                continue;
            }
            fallback.push(t);
        }
        fallback
    }
}
