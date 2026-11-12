// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/PseudoRandom.ts: the sfc32 generator seeded through splitmix32.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Compat::JsNumber::{pad_start, to_int32, to_string_radix36};
use crate::Compat::OrderedSet::OrderedSet;
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
}

impl PseudoRandom {
    /// Creates a generator; the seed is truncated to 32 bits with JS `| 0`.
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
        let mut rng = Self { s0, s1, s2, s3 };
        for _ in 0..12 {
            rng.next();
        }
        rng
    }

    /// Next number in [0, 1).
    #[allow(clippy::should_implement_trait)]
    pub fn next(&mut self) -> f64 {
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
        let lo = min.floor();
        let hi = max.floor();
        (self.next() * (hi - lo)).floor() + lo
    }

    /// Integer-typed convenience over [`Self::next_int`]; identical draw and arithmetic.
    pub fn next_int_i64(&mut self, min: i64, max: i64) -> i64 {
        self.next_int(min as f64, max as f64) as i64
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
        while i > 1 {
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
        let golden: Value =
            serde_json::from_str(include_str!("../../Tests/Golden/PseudoRandom.json")).unwrap();
        for case in golden.as_array().unwrap() {
            let seed = case["seed"].as_f64().unwrap();
            let r = || PseudoRandom::new(seed);
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
}
