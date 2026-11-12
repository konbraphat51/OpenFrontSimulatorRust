//! Port of src/core/DetMath.ts: transcendental functions built only from + - * / and bit views.

#![allow(clippy::approx_constant)]

use crate::Compat::JsNumber::to_int32;

const LN2: f64 = 0.6931471805599453;
const LOG2E: f64 = 1.4426950408889634;
const SQRT2: f64 = 1.4142135623730951;
const PI: f64 = std::f64::consts::PI;
const PI_2: f64 = 1.5707963267948966;
const PI_4: f64 = 0.7853981633974483;
const TAN_PI_8: f64 = 0.41421356237309503;

/// 2 ** n for integer n; exact, saturating to infinity / 0.
pub fn pow2(n: f64) -> f64 {
    if n > 1023.0 {
        return f64::INFINITY;
    }
    if n < -1022.0 {
        return 0.0;
    }
    let hi = to_int32(n + 1023.0).wrapping_shl(20);
    f64::from_bits((hi as u32 as u64) << 32)
}

/// e ** x.
pub fn exp(x: f64) -> f64 {
    if x > 709.0 {
        return f64::INFINITY;
    }
    if x < -708.0 {
        return 0.0;
    }
    let n = (x * LOG2E + 0.5).floor();
    let r = x - n * LN2;
    let p = 1.0
        + r * (1.0
            + r * (1.0 / 2.0
                + r * (1.0 / 6.0
                    + r * (1.0 / 24.0
                        + r * (1.0 / 120.0
                            + r * (1.0 / 720.0 + r * (1.0 / 5040.0 + r * (1.0 / 40320.0))))))));
    p * pow2(n)
}

/// Natural log of x > 0.
pub fn log(x: f64) -> f64 {
    if x <= 0.0 {
        return if x == 0.0 {
            f64::NEG_INFINITY
        } else {
            f64::NAN
        };
    }
    let mut x = x;
    let mut e = 0.0;
    if x < 2.2250738585072014e-308 {
        x *= 18014398509481984.0;
        e = -54.0;
    }
    let bits = x.to_bits();
    let hi = (bits >> 32) as u32;
    e += (((hi >> 20) & 0x7ff) as f64) - 1023.0;
    let new_hi = (hi & 0x000fffff) | 0x3ff00000;
    let mut m = f64::from_bits(((new_hi as u64) << 32) | (bits & 0xffff_ffff));
    if m > SQRT2 {
        m *= 0.5;
        e += 1.0;
    }
    let s = (m - 1.0) / (m + 1.0);
    let z = s * s;
    let series = 1.0
        + z * (1.0 / 3.0
            + z * (1.0 / 5.0
                + z * (1.0 / 7.0 + z * (1.0 / 9.0 + z * (1.0 / 11.0 + z * (1.0 / 13.0))))));
    e * LN2 + 2.0 * s * series
}

/// x ** y for x >= 0.
pub fn pow(x: f64, y: f64) -> f64 {
    if y == 0.0 || x == 1.0 {
        return 1.0;
    }
    if x == 0.0 {
        return if y > 0.0 { 0.0 } else { f64::INFINITY };
    }
    if x < 0.0 {
        return f64::NAN;
    }
    exp(y * log(x))
}

fn atan_unit(z: f64) -> f64 {
    let mut z = z;
    let mut base = 0.0;
    if z > TAN_PI_8 {
        base = PI_4;
        z = (z - 1.0) / (z + 1.0);
    }
    let w = z * z;
    let series = 1.0
        - w * (1.0 / 3.0
            - w * (1.0 / 5.0
                - w * (1.0 / 7.0
                    - w * (1.0 / 9.0
                        - w * (1.0 / 11.0 - w * (1.0 / 13.0 - w * (1.0 / 15.0 - w / 17.0)))))));
    base + z * series
}

/// Angle of (x, y) in (-pi, pi], like `Math.atan2` ignoring signed zeros.
pub fn atan2(y: f64, x: f64) -> f64 {
    if y == 0.0 {
        return if x >= 0.0 { 0.0 } else { PI };
    }
    if x == 0.0 {
        return if y > 0.0 { PI_2 } else { -PI_2 };
    }
    let ax = if x < 0.0 { -x } else { x };
    let ay = if y < 0.0 { -y } else { y };
    let mut a = if ay <= ax {
        atan_unit(ay / ax)
    } else {
        PI_2 - atan_unit(ax / ay)
    };
    if x < 0.0 {
        a = PI - a;
    }
    if y < 0.0 { -a } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn from_hex(s: &Value) -> f64 {
        f64::from_bits(u64::from_str_radix(s.as_str().unwrap(), 16).unwrap())
    }

    fn same(got: f64, want: f64) -> bool {
        got.to_bits() == want.to_bits() || (got.is_nan() && want.is_nan())
    }

    #[test]
    fn matches_upstream_goldens() {
        let golden: Value =
            serde_json::from_str(include_str!("../../Tests/Golden/DetMath.json")).unwrap();
        let mut checked = 0;
        for (name, f) in [("pow2", pow2 as fn(f64) -> f64), ("exp", exp), ("log", log)] {
            for row in golden[name].as_array().unwrap() {
                let (x, want) = (from_hex(&row[0]), from_hex(&row[1]));
                assert!(
                    same(f(x), want),
                    "{name}({x:e}) got {:016x} want {:016x}",
                    f(x).to_bits(),
                    want.to_bits()
                );
                checked += 1;
            }
        }
        for (name, f) in [("pow", pow as fn(f64, f64) -> f64), ("atan2", atan2)] {
            for row in golden[name].as_array().unwrap() {
                let (x, y, want) = (from_hex(&row[0]), from_hex(&row[1]), from_hex(&row[2]));
                assert!(
                    same(f(x, y), want),
                    "{name}({x:e},{y:e}) got {:016x} want {:016x}",
                    f(x, y).to_bits(),
                    want.to_bits()
                );
                checked += 1;
            }
        }
        assert!(checked > 1000);
    }
}
