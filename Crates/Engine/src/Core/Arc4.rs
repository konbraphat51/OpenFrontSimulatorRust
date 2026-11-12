// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the npm `seedrandom` package (v3, default ARC4 generator) as used by the era S `PseudoRandom`.
//! Upstream: seedrandom (© David Bau, MIT), translated to Rust.

use crate::Compat::JsNumber::js_number_to_string;

/// Bytes per RC4 output, and the state size.
const WIDTH: usize = 256;
/// RC4 outputs consumed per double.
const CHUNKS: usize = 6;
/// 2^48, the denominator that goes with six RC4 bytes.
const START_DENOM: f64 = 281_474_976_710_656.0;
/// 2^52, the numerator needs at least this many significant bits.
const SIGNIFICANCE: f64 = 4_503_599_627_370_496.0;
/// 2^53, numerators at or above this are halved.
const OVERFLOW: f64 = 9_007_199_254_740_992.0;

/// `String(seed)` of a JS number.
fn seed_string(seed: f64) -> String {
    if seed.is_nan() {
        "NaN".to_string()
    } else if seed.is_infinite() {
        if seed > 0.0 { "Infinity" } else { "-Infinity" }.to_string()
    } else {
        js_number_to_string(seed)
    }
}

/// seedrandom's `mixkey`: smears the seed's UTF-16 code units into a key of at most 256 bytes.
fn mix_key(seed: &str) -> Vec<u8> {
    let mut key: Vec<u8> = Vec::new();
    let mut smear: i32 = 0;
    for (j, unit) in seed.encode_utf16().enumerate() {
        let idx = j & 255;
        // An unset slot is `undefined * 19 = NaN`, which `^=` treats as 0.
        let prev = key.get(idx).copied().unwrap_or(0) as i32;
        smear ^= prev * 19;
        let byte = (smear.wrapping_add(unit as i32) & 255) as u8;
        if idx < key.len() {
            key[idx] = byte;
        } else {
            key.push(byte);
        }
    }
    key
}

/// The ARC4 stream cipher state of seedrandom.
#[derive(Clone, Debug)]
pub struct Arc4 {
    i: usize,
    j: usize,
    s: [u8; WIDTH],
}

impl Arc4 {
    /// `seedrandom(String(seed))`: key schedule from the seed string, then the first 256 outputs are dropped.
    pub fn new(seed: f64) -> Self {
        let mut key = mix_key(&seed_string(seed));
        if key.is_empty() {
            key.push(0);
        }
        let mut s = [0u8; WIDTH];
        for (i, v) in s.iter_mut().enumerate() {
            *v = i as u8;
        }
        let mut j = 0usize;
        for i in 0..WIDTH {
            j = (j + key[i % key.len()] as usize + s[i] as usize) & 255;
            s.swap(i, j);
        }
        let mut arc4 = Self { i: 0, j: 0, s };
        arc4.generate(WIDTH);
        arc4
    }

    /// seedrandom's `g(count)`: the next `count` bytes folded big-endian into one number.
    fn generate(&mut self, count: usize) -> f64 {
        let mut r = 0.0f64;
        for _ in 0..count {
            self.i = (self.i + 1) & 255;
            let t = self.s[self.i];
            self.j = (self.j + t as usize) & 255;
            self.s[self.i] = self.s[self.j];
            self.s[self.j] = t;
            let k = (self.s[self.i] as usize + self.s[self.j] as usize) & 255;
            r = r * WIDTH as f64 + self.s[k] as f64;
        }
        r
    }

    /// The next double in [0, 1) (the `prng()` closure of seedrandom).
    pub fn next_double(&mut self) -> f64 {
        let mut n = self.generate(CHUNKS);
        let mut d = START_DENOM;
        let mut x = 0.0f64;
        while n < SIGNIFICANCE {
            n = (n + x) * WIDTH as f64;
            d *= WIDTH as f64;
            x = self.generate(1);
        }
        while n >= OVERFLOW {
            n /= 2.0;
            d /= 2.0;
            x = ((x as u32) >> 1) as f64;
        }
        (n + x) / d
    }
}
