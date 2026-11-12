// SPDX-License-Identifier: AGPL-3.0-only
//! Formatting of `f64` exactly like `JSON.stringify` prints a JS number.

/// `JSON.stringify(x)` for a number: shortest round-trip digits, no exponent between 1e-6 and
/// 1e21, `null` for NaN and infinities, `0` for negative zero.
pub fn js_number_to_string(x: f64) -> String {
    if !x.is_finite() {
        return "null".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let abs = x.abs();
    if (1e-6..1e21).contains(&abs) {
        // Rust's Display prints the shortest round-trip digits and pads with zeros.
        return format!("{x}");
    }
    // Exponent form: d.ddde+N
    let s = format!("{x:e}");
    match s.split_once('e') {
        Some((mantissa, exp)) => {
            let sign = if exp.starts_with('-') { "" } else { "+" };
            format!("{mantissa}e{sign}{exp}")
        }
        None => s,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_js() {
        assert_eq!(js_number_to_string(25000.0), "25000");
        assert_eq!(js_number_to_string(0.1), "0.1");
        assert_eq!(js_number_to_string(-0.0), "0");
        assert_eq!(js_number_to_string(2f64.powi(60)), "1152921504606847000");
        assert_eq!(
            js_number_to_string(13238919558137502.0),
            "13238919558137502"
        );
        assert_eq!(js_number_to_string(1e21), "1e+21");
        assert_eq!(js_number_to_string(1.5e-7), "1.5e-7");
        assert_eq!(js_number_to_string(123456789.125), "123456789.125");
    }
}
