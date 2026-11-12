// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/SharedWaterCache.ts: which water components each nation shares with a valid trade partner.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Rebuilt at most once every `TTL_TICKS`. The game objects it reads are behind [`SharedWaterView`],
//! which `GameImpl` implements.

use crate::Game::GameMap::{GameMapImpl, TileRef};
use crate::Pathfinding::PathGameView::SmallId;
use std::collections::{HashMap, HashSet};

const TTL_TICKS: u32 = 30;

/// Sentinel in a player's shared-water set meaning "touches the ocean".
pub const OCEAN_SENTINEL: i64 = -1;

/// What the cache reads from the game.
pub trait SharedWaterView {
    /// The game map (`isWater`, `isOcean`, `isShore`, neighbours).
    fn map(&self) -> &GameMapImpl;
    /// `game.players()` in order.
    fn players(&self) -> Vec<SmallId>;
    /// True for `PlayerType.Bot`.
    fn is_bot(&self, player: SmallId) -> bool;
    /// `player.tileChangeVersion()`.
    fn tile_change_version(&self, player: SmallId) -> u32;
    /// `player.borderTiles()` in set order.
    fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef));
    /// `player.canTrade(other)`.
    fn can_trade(&self, player: SmallId, other: SmallId) -> bool;
    /// `game.getWaterComponent(tile)`.
    fn water_component(&self, tile: TileRef) -> Option<u32>;
}

/// Pass-1 result of one player, valid while its border and the map's water are unchanged.
struct PlayerWater {
    tile_version: u32,
    water_version: u32,
    has_ocean: bool,
    lakes: HashSet<u32>,
}

/// Per-player sets of shared water components (see the module comment).
#[derive(Default)]
pub struct SharedWaterCache {
    tick: Option<u32>,
    by_player: Option<HashMap<SmallId, Option<HashSet<i64>>>>,
    player_water: HashMap<SmallId, PlayerWater>,
}

impl SharedWaterCache {
    /// Creates an empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Shared water components of `player` (`None` when it shares none or is a bot); rebuilds when the TTL expired.
    pub fn get(
        &mut self,
        view: &impl SharedWaterView,
        ticks: u32,
        player: SmallId,
    ) -> Option<&HashSet<i64>> {
        let expired = match self.tick {
            None => true,
            Some(last) => ticks.wrapping_sub(last) >= TTL_TICKS,
        };
        if self.by_player.is_none() || expired {
            self.by_player = Some(self.build(view));
            self.tick = Some(ticks);
        }
        self.by_player
            .as_ref()
            .and_then(|m| m.get(&player))
            .and_then(|o| o.as_ref())
    }

    fn water_for(&mut self, view: &impl SharedWaterView, player: SmallId, water_version: u32) {
        let tile_version = view.tile_change_version(player);
        if let Some(cached) = self.player_water.get(&player)
            && cached.tile_version == tile_version
            && cached.water_version == water_version
        {
            return;
        }
        let map = view.map();
        let mut has_ocean = false;
        let mut lakes: HashSet<u32> = HashSet::new();
        // Only membership is tested, so neighbour order does not matter
        view.for_each_border_tile(player, &mut |tile| {
            if !map.is_shore(tile) {
                return;
            }
            map.for_each_neighbor(tile, |neighbor| {
                if !map.is_water(neighbor) {
                    return;
                }
                if map.is_ocean(neighbor) {
                    has_ocean = true;
                    return;
                }
                if let Some(comp) = view.water_component(neighbor) {
                    lakes.insert(comp);
                }
            });
        });
        self.player_water.insert(
            player,
            PlayerWater {
                tile_version,
                water_version,
                has_ocean,
                lakes,
            },
        );
    }

    fn build(&mut self, view: &impl SharedWaterView) -> HashMap<SmallId, Option<HashSet<i64>>> {
        // Pass 1: which water bodies each non-bot player touches, and the lakes' candidate partners
        let mut player_order: Vec<SmallId> = Vec::new();
        let mut lake_partners: HashMap<u32, Vec<SmallId>> = HashMap::new();

        let water_version = view.map().water_version();
        for player in view.players() {
            if view.is_bot(player) {
                continue;
            }
            self.water_for(view, player, water_version);
            player_order.push(player);

            let entry = &self.player_water[&player];
            // Lakes are visited in upstream's Set order only for partner lists, which are order-insensitive here
            for &c in &entry.lakes {
                lake_partners.entry(c).or_default().push(player);
            }
        }

        // Pass 2: ocean is always shared; a lake only if another player on it can trade with this one
        let mut result: HashMap<SmallId, Option<HashSet<i64>>> = HashMap::new();
        for &player in &player_order {
            let entry = &self.player_water[&player];
            let mut shared: HashSet<i64> = HashSet::new();
            if entry.has_ocean {
                shared.insert(OCEAN_SENTINEL);
            }
            for &c in &entry.lakes {
                let Some(partners) = lake_partners.get(&c) else {
                    continue;
                };
                if partners
                    .iter()
                    .any(|&other| other != player && view.can_trade(player, other))
                {
                    shared.insert(c as i64);
                }
            }
            result.insert(
                player,
                if shared.is_empty() {
                    None
                } else {
                    Some(shared)
                },
            );
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 4x1 map: shore land, ocean, shore land, lake (water without the ocean bit).
    struct View {
        map: GameMapImpl,
        trade: bool,
    }

    impl SharedWaterView for View {
        fn map(&self) -> &GameMapImpl {
            &self.map
        }
        fn players(&self) -> Vec<SmallId> {
            vec![1, 2, 3, 4]
        }
        fn is_bot(&self, player: SmallId) -> bool {
            player == 4
        }
        fn tile_change_version(&self, _player: SmallId) -> u32 {
            0
        }
        fn for_each_border_tile(&self, player: SmallId, f: &mut dyn FnMut(TileRef)) {
            match player {
                1 => f(0),
                2 | 3 => f(2),
                _ => f(0),
            }
        }
        fn can_trade(&self, _player: SmallId, _other: SmallId) -> bool {
            self.trade
        }
        fn water_component(&self, tile: TileRef) -> Option<u32> {
            Some(if tile == 3 { 7 } else { 1 })
        }
    }

    fn view(trade: bool) -> View {
        let map = GameMapImpl::new(4, 1, vec![0xc0, 0x20, 0xc0, 0x00], 2).unwrap();
        View { map, trade }
    }

    #[test]
    fn shares_ocean_and_lakes_with_tradeable_partners() {
        let mut cache = SharedWaterCache::new();
        let v = view(true);
        assert_eq!(cache.get(&v, 0, 1), Some(&HashSet::from([OCEAN_SENTINEL])));
        assert_eq!(
            cache.get(&v, 0, 2),
            Some(&HashSet::from([OCEAN_SENTINEL, 7]))
        );
        assert_eq!(cache.get(&v, 0, 4), None);

        // Within the TTL the old answer stays; afterwards it is rebuilt
        let embargo = view(false);
        assert_eq!(
            cache.get(&embargo, 29, 2),
            Some(&HashSet::from([OCEAN_SENTINEL, 7]))
        );
        assert_eq!(
            cache.get(&embargo, 30, 2),
            Some(&HashSet::from([OCEAN_SENTINEL]))
        );
    }
}
