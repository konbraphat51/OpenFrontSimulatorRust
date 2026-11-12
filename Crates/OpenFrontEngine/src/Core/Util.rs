//! Port of the simulation-relevant functions of src/core/Util.ts.
//!
//! Skipped (client-only, record handling in P1b, randomness outside the simulation, or text
//! sanitising): `sanitize`, `onlyImages`, `toWireGameStartInfo`, `createPartialGameRecord`,
//! `decompressGameRecord`, `assertNever`, `generateID`, `generateGameID`, `createRandomName`
//! (display text only), `replacer`, `sanitizeClanTag`, `LOBBY_LABEL_MAX`, `sanitizeLobbyLabel`.
//! `sigmoid` uses the DetMath `exp` of the baseline build.

use crate::Compat::JsNumber::to_int32;
use crate::Compat::OrderedMap::OrderedMap;
use crate::Core::DetMath;
use crate::Game::Game::{Cell, Gold};
use crate::Game::GameMap::{GameMapImpl, TileRef};
use std::cmp::Ordering;
use std::hash::Hash;

/// Largest integer a JS double holds exactly.
const MAX_SAFE_INTEGER: i64 = 9007199254740991;

/// Manhattan distance where the x axis wraps around `width`.
pub fn manhattan_dist_wrapped(c1: Cell, c2: Cell, width: i32) -> i32 {
    let mut dx = (c1.x - c2.x).abs();
    dx = dx.min(width - dx);
    let dy = (c1.y - c2.y).abs();
    dx + dy
}

/// `Math.min(Math.max(value, min), max)` on plain floats (callers never pass NaN).
pub fn within(value: f64, min: f64, max: f64) -> f64 {
    crate::Compat::JsNumber::min(crate::Compat::JsNumber::max(value, min), max)
}

/// Comparator ordering tiles by Manhattan distance to `target` (`distSort`).
pub fn dist_sort(
    gm: &GameMapImpl,
    target: TileRef,
) -> impl Fn(&TileRef, &TileRef) -> Ordering + '_ {
    move |a, b| {
        gm.manhattan_dist(*a, target)
            .cmp(&gm.manhattan_dist(*b, target))
    }
}

/// Comparator ordering items by Manhattan distance of `tile_of(item)` to `target` (`distSortUnit`).
pub fn dist_sort_by<'a, T>(
    gm: &'a GameMapImpl,
    target: TileRef,
    tile_of: impl Fn(&T) -> TileRef + 'a,
) -> impl Fn(&T, &T) -> Ordering + 'a {
    move |a, b| {
        gm.manhattan_dist(tile_of(a), target)
            .cmp(&gm.manhattan_dist(tile_of(b), target))
    }
}

/// First item with the strictly smallest score among candidates (`findMinimumBy`).
pub fn find_minimum_by<T>(
    values: &[T],
    mut score: impl FnMut(&T) -> f64,
    mut is_candidate: impl FnMut(&T) -> bool,
) -> Option<&T> {
    let mut best = None;
    let mut best_score = f64::INFINITY;
    for value in values {
        if !is_candidate(value) {
            continue;
        }
        let current = score(value);
        if current < best_score {
            best_score = current;
            best = Some(value);
        }
    }
    best
}

/// Alias of [`find_minimum_by`] (`findClosestBy`).
pub fn find_closest_by<T>(
    values: &[T],
    distance: impl FnMut(&T) -> f64,
    is_candidate: impl FnMut(&T) -> bool,
) -> Option<&T> {
    find_minimum_by(values, distance, is_candidate)
}

/// 31-multiplier string hash over UTF-16 code units, absolute value (`simpleHash`).
///
/// `Math.abs(-2^31)` is 2147483648, hence the `u32` result.
pub fn simple_hash(s: &str) -> u32 {
    let mut hash: i32 = 0;
    for unit in s.encode_utf16() {
        // (hash << 5) - hash + char evaluated in f64, then `hash & hash` (ToInt32).
        let v = (hash.wrapping_shl(5) as f64) - hash as f64 + unit as f64;
        hash = to_int32(v);
    }
    hash.unsigned_abs()
}

/// Bounding box of tiles as (min, max) cells; `None` for no tiles (upstream yields infinities).
pub fn calculate_bounding_box(
    gm: &GameMapImpl,
    tiles: impl IntoIterator<Item = TileRef>,
) -> Option<(Cell, Cell)> {
    let mut any = false;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for t in tiles {
        any = true;
        let x = gm.x(t) as i32;
        let y = gm.y(t) as i32;
        min_x = min_x.min(x);
        min_y = min_y.min(y);
        max_x = max_x.max(x);
        max_y = max_y.max(y);
    }
    any.then(|| (Cell::new(min_x, min_y), Cell::new(max_x, max_y)))
}

/// Perimeter tiles of the square of `radius` around `center`, clipped to the map (`boundingBoxTiles`).
pub fn bounding_box_tiles(gm: &GameMapImpl, center: TileRef, radius: i32) -> Vec<TileRef> {
    let mut tiles = Vec::new();
    let cx = gm.x(center) as i32;
    let cy = gm.y(center) as i32;
    let (min_x, max_x, min_y, max_y) = (cx - radius, cx + radius, cy - radius, cy + radius);
    for x in min_x..=max_x {
        if gm.is_valid_coord(x, min_y) {
            tiles.push(gm.tile_ref(x, min_y));
        }
        if gm.is_valid_coord(x, max_y) && min_y != max_y {
            tiles.push(gm.tile_ref(x, max_y));
        }
    }
    for y in (min_y + 1)..max_y {
        if gm.is_valid_coord(min_x, y) {
            tiles.push(gm.tile_ref(min_x, y));
        }
        if gm.is_valid_coord(max_x, y) && min_x != max_x {
            tiles.push(gm.tile_ref(max_x, y));
        }
    }
    tiles
}

/// First key (in insertion order) with the strictly largest positive count (`getMode`).
pub fn get_mode<T: Hash + Eq + Clone>(counts: &OrderedMap<T, f64>) -> Option<T> {
    let mut mode = None;
    let mut max_count = 0.0;
    for (item, &count) in counts.iter() {
        if count > max_count {
            max_count = count;
            mode = Some(item.clone());
        }
    }
    mode
}

/// Center cell of a box (`boundingBoxCenter`).
pub fn bounding_box_center(min: Cell, max: Cell) -> Cell {
    Cell::new(
        min.x + (max.x - min.x).div_euclid(2),
        min.y + (max.y - min.y).div_euclid(2),
    )
}

/// Center of the bounding box of the tiles (`calculateBoundingBoxCenter`).
pub fn calculate_bounding_box_center(
    gm: &GameMapImpl,
    tiles: impl IntoIterator<Item = TileRef>,
) -> Option<Cell> {
    calculate_bounding_box(gm, tiles).map(|(min, max)| bounding_box_center(min, max))
}

/// True when `inner` lies within `outer` (`inscribed`).
pub fn inscribed(outer: (Cell, Cell), inner: (Cell, Cell)) -> bool {
    outer.0.x <= inner.0.x
        && outer.0.y <= inner.0.y
        && outer.1.x >= inner.1.x
        && outer.1.y >= inner.1.y
}

/// `toInt`: floor to an integer; infinities map to the safe-integer limits. Panics on NaN
/// (upstream `BigInt(NaN)` throws a RangeError).
pub fn to_int(num: f64) -> Gold {
    if num == f64::INFINITY {
        return MAX_SAFE_INTEGER;
    }
    if num == f64::NEG_INFINITY {
        return -MAX_SAFE_INTEGER;
    }
    assert!(!num.is_nan(), "toInt(NaN): BigInt would throw");
    num.floor() as i64
}

/// `withinInt`: clamp an integer into [min, max].
pub fn within_int(num: i64, min: i64, max: i64) -> i64 {
    num.max(min).min(max)
}

/// Logistic function with DetMath `exp` (`sigmoid`).
pub fn sigmoid(value: f64, decay_rate: f64, midpoint: f64) -> f64 {
    1.0 / (1.0 + DetMath::exp(-decay_rate * (value - midpoint)))
}

/// `[clan] name` when a non-empty clan tag exists, else `name` (`formatPlayerDisplayName`).
pub fn format_player_display_name(username: &str, clan_tag: Option<&str>) -> String {
    match clan_tag {
        Some(tag) if !tag.is_empty() => format!("[{tag}] {username}"),
        _ => username.to_string(),
    }
}

/// `flattenedEmojiTable`: the 12x5 emoji table flattened row-major (nation AI picks by index).
pub const FLATTENED_EMOJI_TABLE: [&str; 60] = [
    "\u{1f600}",
    "\u{1f60a}",
    "\u{1f970}",
    "\u{1f607}",
    "\u{1f60e}",
    "\u{1f61e}",
    "\u{1f97a}",
    "\u{1f62d}",
    "\u{1f631}",
    "\u{1f621}",
    "\u{1f608}",
    "\u{1f921}",
    "\u{1f971}",
    "\u{1fae1}",
    "\u{1f595}",
    "\u{1f44b}",
    "\u{1f44f}",
    "\u{270b}",
    "\u{1f64f}",
    "\u{1f4aa}",
    "\u{1f44d}",
    "\u{1f44e}",
    "\u{1faf4}",
    "\u{1f90c}",
    "\u{1f926}\u{200d}\u{2642}\u{fe0f}",
    "\u{1f91d}",
    "\u{1f198}",
    "\u{1f54a}\u{fe0f}",
    "\u{1f3f3}\u{fe0f}",
    "\u{23f3}",
    "\u{1f525}",
    "\u{1f4a5}",
    "\u{1f480}",
    "\u{2622}\u{fe0f}",
    "\u{26a0}\u{fe0f}",
    "\u{2196}\u{fe0f}",
    "\u{2b06}\u{fe0f}",
    "\u{2197}\u{fe0f}",
    "\u{1f451}",
    "\u{1f947}",
    "\u{2b05}\u{fe0f}",
    "\u{1f3af}",
    "\u{27a1}\u{fe0f}",
    "\u{1f948}",
    "\u{1f949}",
    "\u{2199}\u{fe0f}",
    "\u{2b07}\u{fe0f}",
    "\u{2198}\u{fe0f}",
    "\u{2764}\u{fe0f}",
    "\u{1f494}",
    "\u{1f4b0}",
    "\u{2693}",
    "\u{26f5}",
    "\u{1f3e1}",
    "\u{1f6e1}\u{fe0f}",
    "\u{1f3ed}",
    "\u{1f682}",
    "\u{2753}",
    "\u{1f414}",
    "\u{1f400}",
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn hex(v: &Value) -> f64 {
        f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
    }

    #[test]
    fn matches_upstream_goldens() {
        let g: Value = serde_json::from_str(include_str!("../../Tests/Golden/Util.json")).unwrap();
        for row in g["hash"].as_array().unwrap() {
            assert_eq!(
                simple_hash(row[0].as_str().unwrap()) as u64,
                row[1].as_u64().unwrap(),
                "{row}"
            );
        }
        for row in g["toInt"].as_array().unwrap() {
            assert_eq!(to_int(hex(&row[0])).to_string(), row[1].as_str().unwrap());
        }
        for row in g["within"].as_array().unwrap() {
            let got = within(
                row[0].as_f64().unwrap(),
                row[1].as_f64().unwrap(),
                row[2].as_f64().unwrap(),
            );
            assert_eq!(got.to_bits(), hex(&row[3]).to_bits());
        }
        for row in g["sigmoid"].as_array().unwrap() {
            let got = sigmoid(hex(&row[0]), hex(&row[1]), hex(&row[2]));
            assert_eq!(got.to_bits(), hex(&row[3]).to_bits());
        }
        for row in g["wrapped"].as_array().unwrap() {
            let n: Vec<i32> = row
                .as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_i64().unwrap() as i32)
                .collect();
            assert_eq!(
                manhattan_dist_wrapped(Cell::new(n[0], n[1]), Cell::new(n[2], n[3]), n[4]),
                n[5]
            );
        }
        let mut counts = OrderedMap::<String, f64>::new();
        for (k, v) in [("a", 1.0), ("b", 3.0), ("c", 3.0), ("d", 2.0)] {
            counts.insert(k.to_string(), v);
        }
        assert_eq!(get_mode(&counts), g["mode"].as_str().map(str::to_string));
        for row in g["display"].as_array().unwrap() {
            assert_eq!(
                format_player_display_name(row[0].as_str().unwrap(), row[1].as_str()),
                row[2].as_str().unwrap()
            );
        }
    }

    #[test]
    fn emoji_table_has_sixty_entries() {
        assert_eq!(FLATTENED_EMOJI_TABLE.len(), 60);
        assert_eq!(FLATTENED_EMOJI_TABLE[0], "😀");
    }
}
