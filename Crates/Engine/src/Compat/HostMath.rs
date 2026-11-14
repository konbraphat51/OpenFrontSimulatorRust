// SPDX-License-Identifier: AGPL-3.0-only
//! Host `Math.exp/log/pow/atan2` of builds before DetMath: the clients' JS engine decides the bits.
//!
//! `exp`, `log` and `atan2` are V8's fdlibm in every V8 release of the period. `Math.pow` is fdlibm
//! up to V8 13.2 and the platform `std::pow` (glibc, here Rust's `powf`) from V8 13.3; which one the
//! recorded hashes agree with is chosen per build in `PowKind`.

use crate::Compat::V8Math;

/// Which `Math.pow` implementation the recording clients ran.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PowKind {
    /// V8's legacy fdlibm port (V8 up to 13.2).
    Fdlibm,
    /// Platform libm `pow` (V8 13.3 and later; glibc, correctly rounded in practice).
    Platform,
}

/// `Math.exp`.
pub fn exp(x: f64) -> f64 {
    V8Math::exp(x)
}

/// `Math.log`.
pub fn log(x: f64) -> f64 {
    V8Math::log(x)
}

/// `Math.atan2`.
pub fn atan2(y: f64, x: f64) -> f64 {
    V8Math::atan2(y, x)
}

/// `Math.pow` (and `**`) with the chosen implementation.
pub fn pow(kind: PowKind, x: f64, y: f64) -> f64 {
    match kind {
        PowKind::Fdlibm => V8Math::pow(x, y),
        PowKind::Platform => platform_pow(x, y),
    }
}

/// JS `x ** y` special cases (`NaN` for base 1 or -1 with infinite exponent) over libm `pow`.
fn platform_pow(x: f64, y: f64) -> f64 {
    if y.is_nan() {
        return f64::NAN;
    }
    if y.is_infinite() && (x == 1.0 || x == -1.0) {
        return f64::NAN;
    }
    x.powf(y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pow_kinds_agree_on_exact_cases() {
        for n in 0..40 {
            let a = pow(PowKind::Fdlibm, 2.0, n as f64);
            let b = pow(PowKind::Platform, 2.0, n as f64);
            assert_eq!(a.to_bits(), b.to_bits());
        }
        assert!(pow(PowKind::Platform, 1.0, f64::INFINITY).is_nan());
        assert_eq!(pow(PowKind::Platform, 4.0, 0.5), 2.0);
    }
}
