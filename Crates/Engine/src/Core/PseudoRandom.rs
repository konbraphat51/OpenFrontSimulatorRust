// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/PseudoRandom.ts: the sfc32 generator seeded through splitmix32, or the older seedrandom (ARC4) generator.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::{Change, Rules};
use crate::Compat::JsNumber::{floor, pad_start, to_int32, to_string_radix36};
use crate::Compat::OrderedSet::OrderedSet;
use crate::Core::Arc4::Arc4;
use std::hash::{BuildHasher, Hash};

/// 36 ** 8, the exclusive bound of [`PseudoRandom::next_id`].
const POW36_8: f64 = 2_821_109_907_456.0;

/// Debug log of every draw (`OFSIM_RNG_LOG=<file>`, `OFSIM_RNG_STACK_AT=<n>`), for diffing against `Reference/RngLog.ts`.
mod rng_log {
    use std::io::Write;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::sync::{Mutex, OnceLock};

    pub static ENABLED: AtomicBool = AtomicBool::new(false);
    static COUNT: AtomicU64 = AtomicU64::new(0);
    static SINK: OnceLock<Mutex<std::io::LineWriter<std::fs::File>>> = OnceLock::new();

    /// Turns the log on when `OFSIM_RNG_LOG` is set.
    pub fn init() {
        if let Ok(path) = std::env::var("OFSIM_RNG_LOG")
            && let Ok(file) = std::fs::File::create(path)
        {
            let _ = SINK.set(Mutex::new(std::io::LineWriter::new(file)));
            ENABLED.store(true, Ordering::Relaxed);
        }
    }

    /// Writes a tick marker `T <tick>` (era S log).
    pub fn mark_tick(tick: u32) {
        if let Some(sink) = SINK.get() {
            let mut w = sink.lock().expect("log lock");
            let _ = writeln!(w, "T {}", tick);
        }
    }

    /// Writes one era S draw: the stream's first value (its key), the draw index and the value.
    pub fn log_arc4(first: f64, index: u64, v: f64) {
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        if let Some(sink) = SINK.get() {
            let mut w = sink.lock().expect("log lock");
            let _ = writeln!(w, "A {} {} {}", first, index, v);
            if std::env::var("OFSIM_RNG_STACK_AT")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                == Some(n)
            {
                let _ = w.flush();
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
                std::process::exit(3);
            }
        }
    }

    /// Writes one draw: the stream's counter word before the draw and the raw 32-bit value.
    pub fn log(s3: i32, t: u32) {
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        if let Some(sink) = SINK.get() {
            let mut w = sink.lock().expect("log lock");
            let _ = writeln!(w, "{} {}", s3, t);
            if std::env::var("OFSIM_RNG_STACK_AT")
                .ok()
                .and_then(|v| v.parse::<u64>().ok())
                == Some(n)
            {
                let _ = w.flush();
                eprintln!("{}", std::backtrace::Backtrace::force_capture());
                std::process::exit(3);
            }
        }
    }
}

/// Marks the start of a tick in the draw log.
pub fn mark_rng_log_tick(tick: u32) {
    if rng_log::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
        rng_log::mark_tick(tick);
    }
}

/// Starts the optional draw log (see `rng_log`).
pub fn init_rng_log() {
    rng_log::init();
}

/// Seeded sfc32 generator; every draw order must match upstream exactly.
#[derive(Clone, Debug)]
pub struct PseudoRandom {
    s0: i32,
    s1: i32,
    s2: i32,
    s3: i32,
    /// Era S backend (`seedrandom(String(seed))`); the sfc32 words are unused when set.
    arc4: Option<Box<Arc4>>,
    /// `nextInt` floors its bounds first (builds since v0.31).
    floors_args: bool,
    /// `shuffleArray` skips the index-0 draw (PR 2046 on; older builds loop to `i >= 0`).
    shuffle_skips_zero: bool,
    /// Debug log state of an era S stream: first value and draws so far.
    arc4_log: (Option<f64>, u64),
}

impl PseudoRandom {
    /// Creates the generator the build's rules select (sfc32, or seedrandom before PR 4233).
    pub fn for_rules(rules: &Rules, seed: f64) -> Self {
        let mut random = if rules.has(Change::Sfc32PseudoRandom4233) {
            Self::new(seed)
        } else {
            Self::new_seedrandom(seed)
        };
        random.floors_args = rules.has(Change::NextIntFloorsArgs31);
        random.shuffle_skips_zero = rules.has(Change::ShuffleArrayStopsAtOne2046);
        random
    }

    /// Creates an era S generator: npm `seedrandom` seeded with `String(seed)`.
    pub fn new_seedrandom(seed: f64) -> Self {
        Self {
            s0: 0,
            s1: 0,
            s2: 0,
            s3: 0,
            arc4: Some(Box::new(Arc4::new(seed))),
            floors_args: true,
            shuffle_skips_zero: true,
            arc4_log: (None, 0),
        }
    }

    /// Creates an sfc32 generator; the seed is truncated to 32 bits with JS `| 0`.
    pub fn new(seed: f64) -> Self {
        let mut h = to_int32(seed);
        let mut split = || {
            h = h.wrapping_add(0x9e3779b9u32 as i32);
            let mut t = h ^ (((h as u32) >> 16) as i32);
            t = t.wrapping_mul(0x21f0aaad);
            t ^= ((t as u32) >> 15) as i32;
            t = t.wrapping_mul(0x735a2d97);
            t ^ (((t as u32) >> 15) as i32)
        };
        let s0 = split();
        let s1 = split();
        let s2 = split();
        let s3 = split();
        let mut rng = Self {
            s0,
            s1,
            s2,
            s3,
            arc4: None,
            floors_args: true,
            shuffle_skips_zero: true,
            arc4_log: (None, 0),
        };
        for _ in 0..12 {
            rng.next();
        }
        rng
    }

    /// Next number in [0, 1).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
        if let Some(arc4) = self.arc4.as_mut() {
            let v = arc4.next_double();
            if rng_log::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
                let first = *self.arc4_log.0.get_or_insert(v);
                rng_log::log_arc4(first, self.arc4_log.1, v);
                self.arc4_log.1 += 1;
            }
            return v;
        }
        let t = self.s0.wrapping_add(self.s1).wrapping_add(self.s3);
        if rng_log::ENABLED.load(std::sync::atomic::Ordering::Relaxed) {
            rng_log::log(self.s3, t as u32);
        }
        self.s3 = self.s3.wrapping_add(1);
        self.s0 = self.s1 ^ (((self.s1 as u32) >> 9) as i32);
        self.s1 = self.s2.wrapping_add(self.s2.wrapping_shl(3));
        self.s2 = self.s2.wrapping_shl(21) | (((self.s2 as u32) >> 11) as i32);
        self.s2 = self.s2.wrapping_add(t);
        (t as u32) as f64 / 4294967296.0
    }

    /// Random integer in [min, max); arguments are floored first like upstream.
    pub fn next_int(&mut self, min: f64, max: f64) -> f64 {
        if !self.floors_args {
            return floor(self.next() * (max - min)) + min;
        }
        let lo = floor(min);
        let hi = floor(max);
        floor(self.next() * (hi - lo)) + lo
    }

    /// Random float in [min, max).
    pub fn next_float(&mut self, min: f64, max: f64) -> f64 {
        self.next() * (max - min) + min
    }

    /// Random 8-character base-36 id.
    pub fn next_id(&mut self) -> String {
        let v = (self.next() * POW36_8).floor();
        pad_start(&to_string_radix36(v), 8, '0')
    }

    /// Index of a random element of a collection of `len` items; panics when empty like upstream.
    pub fn rand_index(&mut self, len: usize) -> usize {
        assert!(len != 0, "array must not be empty");
        self.next_int(0.0, len as f64) as usize
    }

    /// Random element of a slice.
    pub fn rand_element<'a, T>(&mut self, arr: &'a [T]) -> &'a T {
        let i = self.rand_index(arr.len());
        &arr[i]
    }

    /// Random element of an insertion-ordered set (the n-th in iteration order).
    pub fn rand_from_set<'a, T: Hash + Eq + Clone, S: BuildHasher + Default>(
        &mut self,
        set: &'a OrderedSet<T, S>,
    ) -> &'a T {
        let i = self.rand_index(set.len());
        set.nth(i).expect("index below size")
    }

    /// True with probability 1/odds.
    pub fn chance(&mut self, odds: f64) -> bool {
        self.next_int(0.0, odds) == 0.0
    }

    /// Fisher-Yates shuffled copy.
    pub fn shuffle_array<T: Clone>(&mut self, array: &[T]) -> Vec<T> {
        let mut result = array.to_vec();
        let mut i = result.len();
        let stop = if self.shuffle_skips_zero { 1 } else { 0 };
        while i > stop {
            i -= 1;
            let j = self.next_int(0.0, (i + 1) as f64) as usize;
            result.swap(i, j);
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn bits(x: f64) -> String {
        format!("{:016x}", x.to_bits())
    }

    fn strs(v: &Value) -> Vec<String> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string())
            .collect()
    }

    fn nums(v: &Value) -> Vec<f64> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_f64().unwrap())
            .collect()
    }

    #[test]
    fn matches_upstream_goldens() {
        check_goldens(
            include_str!("../../Tests/Golden/PseudoRandom.json"),
            PseudoRandom::new,
        );
    }

    #[test]
    fn seedrandom_matches_upstream_goldens() {
        let text = include_str!("../../Tests/Golden/PseudoRandomSeedrandom.json");
        check_goldens(text, PseudoRandom::new_seedrandom);
        let golden: Value = serde_json::from_str(text).unwrap();
        for case in golden.as_array().unwrap() {
            let mut g = PseudoRandom::new_seedrandom(case["seed"].as_f64().unwrap());
            let got: Vec<_> = (0..700).map(|_| bits(g.next())).collect();
            assert_eq!(got, strs(&case["long"]), "long seed {}", case["seed"]);
        }
    }

    fn check_goldens(text: &str, make: fn(f64) -> PseudoRandom) {
        let golden: Value = serde_json::from_str(text).unwrap();
        for case in golden.as_array().unwrap() {
            let seed = case["seed"].as_f64().unwrap();
            let r = || make(seed);
            let ctx = format!("seed {seed}");

            let mut g = r();
            let got: Vec<_> = (0..16).map(|_| bits(g.next())).collect();
            assert_eq!(got, strs(&case["next"]), "next {ctx}");

            let mut g = r();
            let got: Vec<_> = (0..16).map(|_| g.next_int(0.0, 100.0)).collect();
            assert_eq!(got, nums(&case["nextInt"]), "nextInt {ctx}");

            let mut g = r();
            let got: Vec<_> = (0..8).map(|_| g.next_int(-7.5, 20.2)).collect();
            assert_eq!(got, nums(&case["nextIntOdd"]), "nextIntOdd {ctx}");

            let mut g = r();
            let got: Vec<_> = (0..8).map(|_| bits(g.next_float(-3.0, 9.0))).collect();
            assert_eq!(got, strs(&case["nextFloat"]), "nextFloat {ctx}");

            let mut g = r();
            let got: Vec<_> = (0..6).map(|_| g.next_id()).collect();
            assert_eq!(got, strs(&case["nextID"]), "nextID {ctx}");

            let mut g = r();
            let got: Vec<bool> = (0..16).map(|_| g.chance(5.0)).collect();
            let want: Vec<bool> = case["chance"]
                .as_array()
                .unwrap()
                .iter()
                .map(|b| b.as_bool().unwrap())
                .collect();
            assert_eq!(got, want, "chance {ctx}");

            let mut g = r();
            let items = ["a", "b", "c", "d", "e", "f", "g", "h", "i", "j"];
            let got: Vec<_> = (0..8).map(|_| g.rand_element(&items).to_string()).collect();
            assert_eq!(got, strs(&case["randElement"]), "randElement {ctx}");

            let mut g = r();
            let got = g.shuffle_array(&(0..12).collect::<Vec<i64>>());
            let want: Vec<i64> = nums(&case["shuffle"]).iter().map(|x| *x as i64).collect();
            assert_eq!(got, want, "shuffle {ctx}");
            assert!(g.shuffle_array::<i64>(&[]).is_empty());

            let mut g = r();
            let set: OrderedSet<i64> = [50, 7, 9, 1000, 3, 12, 4].into_iter().collect();
            let got: Vec<f64> = (0..8).map(|_| *g.rand_from_set(&set) as f64).collect();
            assert_eq!(got, nums(&case["randFromSet"]), "randFromSet {ctx}");

            let mut g = r();
            let mut got = Vec::new();
            for _ in 0..10 {
                got.push(bits(g.next()));
                got.push(format!("{}", g.next_int(3.0, 9.0)));
                got.push(g.next_id());
            }
            assert_eq!(got, strs(&case["mixed"]), "mixed {ctx}");
        }
    }

    #[test]
    fn shuffle_before_pr_2046_draws_once_more() {
        // Older builds loop to index 0 and consume one extra draw per shuffle.
        let mut old = PseudoRandom::new_seedrandom(7.0);
        old.shuffle_skips_zero = false;
        let mut new = PseudoRandom::new_seedrandom(7.0);
        old.shuffle_array(&[1, 2, 3]);
        new.shuffle_array(&[1, 2, 3]);
        assert_ne!(old.next(), new.next());
        assert!(old.shuffle_array::<i64>(&[]).is_empty());
    }
}
