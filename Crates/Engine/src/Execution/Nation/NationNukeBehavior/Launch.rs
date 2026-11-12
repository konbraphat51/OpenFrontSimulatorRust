// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationNukeBehavior.ts (part: launch): nuke tile scoring and launching.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl NationNukeBehavior {
    pub(super) fn nuke_tile_score(
        &self,
        game: &GameImpl,
        tile: TileRef,
        silos: &[UnitId],
        targets: &[UnitId],
        nuke_type: UnitType,
    ) -> f64 {
        let magnitude = game.config().nuke_magnitudes(nuke_type);
        let dist = eucl_dist_fn(tile, magnitude.outer, false);
        let mut tile_value = 0.0;
        for &unit_id in targets {
            let unit = game.unit_any(unit_id);
            if !dist(&game.map, unit.tile) {
                continue;
            }
            let level = unit.level as f64;
            tile_value += match unit.unit_type {
                UnitType::City => 25_000.0 * level,
                UnitType::DefensePost => 5_000.0 * level,
                UnitType::MissileSilo => 50_000.0 * level,
                UnitType::Port => 15_000.0 * level,
                UnitType::Factory => 15_000.0 * level,
                _ => 0.0,
            };
        }

        let difficulty = Self::difficulty(game);
        // Easy ignores SAMs; Medium applies a simple local SAM penalty;
        // Hard & Impossible rely on the trajectory-based interception checks of maybeSendNuke().
        if difficulty == Difficulty::Medium {
            let dist50 = eucl_dist_fn(tile, 50.0, false);
            let has_sam = targets.iter().any(|&u| {
                let unit = game.unit_any(u);
                unit.unit_type == UnitType::SAMLauncher && dist50(&game.map, unit.tile)
            });
            if has_sam {
                return -1.0;
            }
        }

        // On Impossible with a hydrogen bomb, add value for SAMs that can be outranged.
        if difficulty == Difficulty::Impossible && nuke_type == UnitType::HydrogenBomb {
            let hydro_magnitude = game.config().nuke_magnitudes(UnitType::HydrogenBomb);
            let nearby_sams = game.nearby_units(
                tile,
                hydro_magnitude.outer,
                &[UnitType::SAMLauncher],
                None,
                false,
            );

            for (sam, _) in nearby_sams {
                let sam_unit = game.unit_any(sam);
                let sam_level = sam_unit.level as f64;
                if sam_level >= 5.0 {
                    continue; // Can't outrange level 5+ SAMs
                }

                let sam_range = game.config().sam_range(sam_level);
                let dist_to_sam =
                    (game.map.euclidean_dist_squared(tile, sam_unit.tile) as f64).sqrt();

                // Significant value for destroying a SAM that can be outranged.
                if dist_to_sam > sam_range {
                    tile_value += 100_000.0 * sam_level;
                }
            }
        }

        // Prefer tiles closer to a silo (but preserve structure value).
        let silo_tiles: Vec<TileRef> = silos.iter().map(|&u| game.unit_any(u).tile).collect();
        let Some((closest_silo, _)) = closest_two_tiles(&game.map, silo_tiles, [tile]) else {
            panic!("Missing result");
        };
        let distance_squared = game.map.euclidean_dist_squared(tile, closest_silo) as f64;
        let distance_to_closest_silo = distance_squared.sqrt();
        let distance_penalty = distance_to_closest_silo * 30.0;
        let base_tile_value = tile_value;
        // Keep at least 20% of the structure value.
        tile_value = (base_tile_value * 0.2).max(tile_value - distance_penalty);

        // Don't target near recent targets.
        for &(_, recent_tile, recent_nuke_type) in &self.recently_sent_nukes {
            let recent_inner_radius = game.config().nuke_magnitudes(recent_nuke_type).inner;
            let dist_squared = game.map.euclidean_dist_squared(tile, recent_tile) as f64;
            if dist_squared <= recent_inner_radius * recent_inner_radius {
                tile_value -= 1_000_000.0;
            }
        }

        tile_value
    }

    pub(super) fn send_nuke(
        &mut self,
        game: &mut GameImpl,
        tile: TileRef,
        nuke_type: UnitType,
        target_player: SmallId,
        wait_ticks: f64,
    ) {
        let tick = game.ticks();
        self.recently_sent_nukes.push((tick, tile, nuke_type));
        let v2 = game.has(Change::NationsLikeHumans5899);
        let perceived = if v2 {
            self.inflated_nuke_cost(game, nuke_type)
        } else if nuke_type == UnitType::AtomBomb {
            self.atom_bomb_perceived_cost
        } else {
            self.hydrogen_bomb_perceived_cost
        };
        if nuke_type == UnitType::AtomBomb {
            self.atom_bombs_launched += 1;
            // +50% each time to simulate saving up for a MIRV (more than hydro, so atom bombs lose appeal late).
            let percent = if game.has(Change::NationBehaviorUpdate3246) {
                150
            } else {
                135
            };
            self.atom_bomb_perceived_cost = (perceived * percent) / 100;
            self.atom_bomb_last_launch = Some(tick);
        } else if nuke_type == UnitType::HydrogenBomb {
            self.hydrogen_bombs_launched += 1;
            // +25% each time to simulate saving up for a MIRV.
            self.hydrogen_bomb_perceived_cost = (perceived * 125) / 100;
            self.hydrogen_bomb_last_launch = Some(tick);
        }
        self.log.queue(
            game,
            self.player,
            Decision::Nuke {
                nuke_type,
                tile,
                wait_ticks,
            },
        );
        self.emoji_behavior
            .borrow_mut()
            .maybe_send_nuke_emoji(game, target_player);
    }
}
