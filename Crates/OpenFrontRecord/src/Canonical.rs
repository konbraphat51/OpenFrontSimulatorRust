// SPDX-License-Identifier: AGPL-3.0-only
//! Canonical JSON text (sorted keys, JS number formatting, null members dropped) for cross-language digests.

use serde_json::Value;

/// Formats an `f64` exactly as JavaScript's `Number.prototype.toString` does.
pub fn js_number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    let sign = if x < 0.0 { "-" } else { "" };
    // Shortest round-trip digits as "d.ddde<exp>".
    let scientific = format!("{:e}", x.abs());
    let (mantissa, exponent) = scientific.split_once('e').expect("scientific format");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exponent.parse::<i32>().expect("exponent") + 1;
    let body = if k <= n && n <= 21 {
        format!("{digits}{}", "0".repeat((n - k) as usize))
    } else if 0 < n && n <= 21 {
        format!("{}.{}", &digits[..n as usize], &digits[n as usize..])
    } else if -6 < n && n <= 0 {
        format!("0.{}{digits}", "0".repeat((-n) as usize))
    } else {
        let e = n - 1;
        let exp = format!("e{}{}", if e < 0 { '-' } else { '+' }, e.abs());
        if k == 1 {
            format!("{digits}{exp}")
        } else {
            format!("{}.{}{exp}", &digits[..1], &digits[1..])
        }
    };
    format!("{sign}{body}")
}

/// Canonical JSON text of a value: object keys sorted by UTF-16 code units, null members omitted,
/// numbers formatted like JavaScript, strings escaped like `JSON.stringify`.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_canonical(value, &mut out);
    out
}

fn write_canonical(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Number(n) => out.push_str(&js_number_to_string(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => out.push_str(&serde_json::to_string(s).expect("string")),
        Value::Array(items) => {
            out.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                write_canonical(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> =
                map.iter().filter(|(_, v)| !v.is_null()).collect();
            entries.sort_by(|a, b| a.0.encode_utf16().cmp(b.0.encode_utf16()));
            out.push('{');
            for (index, (key, item)) in entries.into_iter().enumerate() {
                if index > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).expect("key"));
                out.push(':');
                write_canonical(item, out);
            }
            out.push('}');
        }
    }
}

#[cfg(test)]
mod tests {
    use super::js_number_to_string as f;

    #[test]
    fn matches_javascript_formatting() {
        assert_eq!(f(5.0), "5");
        assert_eq!(f(-0.0), "0");
        assert_eq!(f(0.5), "0.5");
        assert_eq!(f(1e21), "1e+21");
        assert_eq!(f(1.5e21), "1.5e+21");
        assert_eq!(f(123456789012345680000.0), "123456789012345680000");
        assert_eq!(f(1e-7), "1e-7");
        assert_eq!(f(0.000001), "0.000001");
        assert_eq!(f(1.2345e-7), "1.2345e-7");
        assert_eq!(f(12345.678), "12345.678");
        assert_eq!(f(f64::MAX), "1.7976931348623157e+308");
    }
}
