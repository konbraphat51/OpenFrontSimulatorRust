//! JS number semantics on `f64`: `ToInt32`, `ToUint32`, `Math.round`, `toString(radix)`, `padStart`.

/// JS `ToInt32`: truncate toward zero, wrap modulo 2^32, NaN and infinities become 0.
pub fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let t = x.trunc();
    if t.abs() < 2147483648.0 {
        return t as i32;
    }
    // Exact modulo 2^32 on a finite integral f64.
    let m = t.rem_euclid(4294967296.0);
    (m as u64 as u32) as i32
}

/// JS `ToUint32`.
pub fn to_uint32(x: f64) -> u32 {
    to_int32(x) as u32
}

/// JS `Math.round`: `floor(x + 0.5)` with JS handling of -0, NaN, infinities and the largest doubles.
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    // Values at or beyond 2^52 are already integral; x + 0.5 would round them.
    if x.abs() >= 4503599627370496.0 {
        return x;
    }
    if (-0.5..0.0).contains(&x) {
        return -0.0;
    }
    // floor(x + 0.5) is wrong for 0.49999999999999994 (the sum rounds up to 1); JS gives 0.
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// JS `Math.trunc`.
pub fn trunc(x: f64) -> f64 {
    x.trunc()
}

/// JS `Math.floor`.
pub fn floor(x: f64) -> f64 {
    x.floor()
}

/// JS `Math.sign`: NaN and signed zeros are returned unchanged.
pub fn sign(x: f64) -> f64 {
    if x.is_nan() || x == 0.0 {
        x
    } else if x > 0.0 {
        1.0
    } else {
        -1.0
    }
}

/// JS `Math.imul`: 32-bit wrapping multiplication.
pub fn imul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

/// JS `Math.max` for two arguments (NaN wins, +0 beats -0).
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { b } else { a }
    } else if a > b {
        a
    } else {
        b
    }
}

/// JS `Math.min` for two arguments (NaN wins, -0 beats +0).
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a == 0.0 && b == 0.0 {
        if a.is_sign_negative() { a } else { b }
    } else if a < b {
        a
    } else {
        b
    }
}

/// `Number.prototype.toString(36)` for a non-negative integral value below 2^53.
///
/// Panics on negative, fractional or non-finite input; upstream only calls it on floored ids.
pub fn to_string_radix36(x: f64) -> String {
    assert!(
        x.is_finite() && x >= 0.0 && x.fract() == 0.0 && x < 9007199254740992.0,
        "to_string_radix36 expects a non-negative integer below 2^53, got {x}"
    );
    let mut n = x as u64;
    if n == 0 {
        return "0".to_string();
    }
    let mut digits = Vec::new();
    while n > 0 {
        digits.push(std::char::from_digit((n % 36) as u32, 36).unwrap() as u8);
        n /= 36;
    }
    digits.reverse();
    String::from_utf8(digits).unwrap()
}

/// `String.prototype.padStart(len, fill)` counted in UTF-16 code units, for an ASCII `fill`.
pub fn pad_start(s: &str, len: usize, fill: char) -> String {
    assert!(fill.is_ascii(), "pad_start supports ASCII fill only");
    let have = s.encode_utf16().count();
    if have >= len {
        return s.to_string();
    }
    let mut out = String::with_capacity(len);
    for _ in 0..(len - have) {
        out.push(fill);
    }
    out.push_str(s);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_int32_matches_js() {
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
        assert_eq!(to_int32(2147483648.0), -2147483648);
        assert_eq!(to_int32(4294967296.0), 0);
        assert_eq!(to_int32(4294967297.5), 1);
        assert_eq!(to_int32(-1.9), -1);
        assert_eq!(to_int32(-2147483649.0), 2147483647);
        assert_eq!(to_int32(1e20), 1661992960);
        assert_eq!(to_uint32(-1.0), 4294967295);
        assert_eq!(to_uint32(-0.5), 0);
    }

    #[test]
    fn round_matches_js() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert_eq!(round(0.49999999999999994), 0.0);
        assert_eq!(round(-0.4).to_bits(), (-0.0f64).to_bits());
        assert_eq!(round(-0.0).to_bits(), (-0.0f64).to_bits());
        assert!(round(f64::NAN).is_nan());
        assert_eq!(round(4503599627370495.5), 4503599627370496.0);
        assert_eq!(round(-3.5), -3.0);
    }

    #[test]
    fn radix36_and_pad() {
        assert_eq!(to_string_radix36(0.0), "0");
        assert_eq!(to_string_radix36(35.0), "z");
        assert_eq!(to_string_radix36(36.0), "10");
        assert_eq!(to_string_radix36(2821109907455.0), "zzzzzzzz");
        assert_eq!(pad_start("ab", 8, '0'), "000000ab");
        assert_eq!(pad_start("abcdefghij", 8, '0'), "abcdefghij");
    }

    #[test]
    fn min_max_signed_zero() {
        assert_eq!(max(-0.0, 0.0).to_bits(), 0.0f64.to_bits());
        assert_eq!(min(0.0, -0.0).to_bits(), (-0.0f64).to_bits());
        assert!(max(f64::NAN, 1.0).is_nan());
    }

    #[test]
    fn matches_node_goldens() {
        let g: serde_json::Value =
            serde_json::from_str(include_str!("../../Tests/Golden/JsNumber.json")).unwrap();
        let hex = |v: &serde_json::Value| {
            f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
        };
        for row in g["int32"].as_array().unwrap() {
            let x = hex(&row[0]);
            assert_eq!(to_int32(x) as i64, row[1].as_i64().unwrap(), "int32 {x:e}");
            assert_eq!(
                to_uint32(x) as i64,
                row[2].as_i64().unwrap(),
                "uint32 {x:e}"
            );
            let want = hex(&row[3]);
            let got = round(x);
            assert!(
                got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan()),
                "round {x:e}"
            );
        }
        for row in g["radix"].as_array().unwrap() {
            assert_eq!(
                to_string_radix36(row[0].as_f64().unwrap()),
                row[1].as_str().unwrap()
            );
        }
        for row in g["pad"].as_array().unwrap() {
            let (s, n) = (row[0].as_str().unwrap(), row[1].as_u64().unwrap() as usize);
            assert_eq!(pad_start(s, n, '0'), row[2].as_str().unwrap());
        }
    }
}
