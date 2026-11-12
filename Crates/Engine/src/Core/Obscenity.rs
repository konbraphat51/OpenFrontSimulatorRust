// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the `obscenity` v0.4.3 English `RegExpMatcher` that src/core/validations/username.ts uses to censor names.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! The term lists and character maps are dumped by `Tools/GenObscenityData.mjs`. Text is handled as UTF-16 units like the JS
//! `RegExp` without the `u` flag (`.` matches one unit, `\b` knows ASCII word characters only).

use serde::Deserialize;
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Deserialize)]
#[serde(tag = "k", rename_all = "lowercase")]
enum Node {
    Lit { chars: Vec<u32> },
    Opt { child: Box<Node> },
    Wild,
}

#[derive(Deserialize)]
struct Term {
    start: bool,
    end: bool,
    nodes: Vec<Node>,
}

#[derive(Deserialize)]
struct Collapse {
    default: f64,
    custom: Vec<(u32, f64)>,
}

#[derive(Deserialize)]
struct Dump {
    blacklist: Vec<Term>,
    whitelist: Vec<String>,
    confusables: Vec<(u32, u32)>,
    leet: Vec<(u32, u32)>,
    collapse: Collapse,
}

struct Matcher {
    blacklist: Vec<Term>,
    whitelist: Vec<Vec<u16>>,
    confusables: HashMap<u32, u32>,
    leet: HashMap<u32, u32>,
    collapse_custom: HashMap<u32, f64>,
    collapse_default: f64,
}

fn matcher() -> &'static Matcher {
    static MATCHER: OnceLock<Matcher> = OnceLock::new();
    MATCHER.get_or_init(|| {
        let dump: Dump = serde_json::from_str(include_str!("../../data/Obscenity.json"))
            .expect("Obscenity.json is valid");
        Matcher {
            blacklist: dump.blacklist,
            whitelist: dump
                .whitelist
                .iter()
                .map(|w| w.encode_utf16().collect())
                .collect(),
            confusables: dump.confusables.into_iter().collect(),
            leet: dump.leet.into_iter().collect(),
            collapse_custom: dump.collapse.custom.into_iter().collect(),
            collapse_default: dump.collapse.default,
        }
    })
}

/// State of obscenity's `CollapseDuplicatesTransformer`.
struct Collapser<'a> {
    remaining: f64,
    last: i64,
    default: f64,
    custom: &'a dyn Fn(u32) -> Option<f64>,
}

impl Collapser<'_> {
    fn transform(&mut self, c: u32) -> Option<u32> {
        if c as i64 == self.last {
            let keep = self.remaining > 0.0;
            self.remaining -= 1.0;
            return keep.then_some(c);
        }
        let threshold = (self.custom)(c).unwrap_or(self.default);
        self.remaining = threshold - 1.0;
        self.last = c as i64;
        (threshold > 0.0).then_some(c)
    }
}

/// Code points of a UTF-16 string with the index of the first unit of each (`CharacterIterator`).
fn code_points(units: &[u16]) -> Vec<(u32, usize)> {
    let mut out = Vec::with_capacity(units.len());
    let mut i = 0;
    while i < units.len() {
        let u = units[i];
        if (0xd800..0xdc00).contains(&u)
            && i + 1 < units.len()
            && (0xdc00..0xe000).contains(&units[i + 1])
        {
            let cp = 0x10000 + (((u as u32) - 0xd800) << 10) + (units[i + 1] as u32 - 0xdc00);
            out.push((cp, i));
            i += 2;
        } else {
            out.push((u as u32, i));
            i += 1;
        }
    }
    out
}

fn push_code_point(out: &mut Vec<u16>, positions: &mut Vec<usize>, cp: u32, position: usize) {
    if let Some(c) = char::from_u32(cp) {
        let mut buf = [0u16; 2];
        out.extend_from_slice(c.encode_utf16(&mut buf));
    } else {
        // A lone surrogate code point is kept as one unit.
        out.push(cp as u16);
    }
    while positions.len() < out.len() {
        positions.push(position);
    }
}

fn lower(c: u32) -> u32 {
    if (0x41..=0x5a).contains(&c) {
        c ^ 0x20
    } else {
        c
    }
}

/// The blacklist transformers: confusables, leet speak, ASCII lower case, collapse duplicates.
fn transform_blacklist(m: &Matcher, input: &[u16]) -> (Vec<usize>, Vec<u16>) {
    let custom = |c: u32| m.collapse_custom.get(&c).copied();
    let mut collapser = Collapser {
        remaining: -1.0,
        last: -1,
        default: m.collapse_default,
        custom: &custom,
    };
    let mut positions = Vec::new();
    let mut out = Vec::new();
    for (cp, position) in code_points(input) {
        let c = m.confusables.get(&cp).copied().unwrap_or(cp);
        let c = m.leet.get(&c).copied().unwrap_or(c);
        if let Some(c) = collapser.transform(lower(c)) {
            push_code_point(&mut out, &mut positions, c, position);
        }
    }
    (positions, out)
}

/// The whitelist transformers: ASCII lower case, then collapsed spaces.
fn transform_whitelist(input: &[u16]) -> (Vec<usize>, Vec<u16>) {
    let custom = |c: u32| (c == 0x20).then_some(1.0);
    let mut collapser = Collapser {
        remaining: -1.0,
        last: -1,
        default: f64::INFINITY,
        custom: &custom,
    };
    let mut positions = Vec::new();
    let mut out = Vec::new();
    for (cp, position) in code_points(input) {
        if let Some(c) = collapser.transform(lower(cp)) {
            push_code_point(&mut out, &mut positions, c, position);
        }
    }
    (positions, out)
}

fn is_word_unit(u: u16) -> bool {
    matches!(u, 0x30..=0x39 | 0x41..=0x5a | 0x61..=0x7a | 0x5f)
}

fn at_boundary(text: &[u16], pos: usize) -> bool {
    let before = pos > 0 && is_word_unit(text[pos - 1]);
    let after = pos < text.len() && is_word_unit(text[pos]);
    before != after
}

/// Backtracking match of nodes `idx..` at `pos`; returns the end of the first (greedy) match.
fn match_nodes(nodes: &[Node], end_boundary: bool, text: &[u16], pos: usize) -> Option<usize> {
    let Some((first, rest)) = nodes.split_first() else {
        return (!end_boundary || at_boundary(text, pos)).then_some(pos);
    };
    match first {
        Node::Lit { chars } => {
            let mut p = pos;
            for &cp in chars {
                let mut buf = [0u16; 2];
                let units: &[u16] = match char::from_u32(cp) {
                    Some(c) => c.encode_utf16(&mut buf),
                    None => {
                        buf[0] = cp as u16;
                        &buf[..1]
                    }
                };
                if text.len() < p + units.len() || &text[p..p + units.len()] != units {
                    return None;
                }
                p += units.len();
            }
            match_nodes(rest, end_boundary, text, p)
        }
        Node::Wild => {
            if pos < text.len() {
                match_nodes(rest, end_boundary, text, pos + 1)
            } else {
                None
            }
        }
        Node::Opt { child } => {
            // Greedy: try with the child first, then without it.
            let with: Vec<&Node> = std::iter::once(child.as_ref()).collect();
            if let Some(end) = match_with_child(with[0], rest, end_boundary, text, pos) {
                return Some(end);
            }
            match_nodes(rest, end_boundary, text, pos)
        }
    }
}

fn match_with_child(
    child: &Node,
    rest: &[Node],
    end_boundary: bool,
    text: &[u16],
    pos: usize,
) -> Option<usize> {
    let next = match child {
        Node::Lit { chars } => {
            let mut p = pos;
            for &cp in chars {
                let mut buf = [0u16; 2];
                let units: &[u16] = match char::from_u32(cp) {
                    Some(c) => c.encode_utf16(&mut buf),
                    None => {
                        buf[0] = cp as u16;
                        &buf[..1]
                    }
                };
                if text.len() < p + units.len() || &text[p..p + units.len()] != units {
                    return None;
                }
                p += units.len();
            }
            p
        }
        Node::Wild => {
            if pos < text.len() {
                pos + 1
            } else {
                return None;
            }
        }
        Node::Opt { .. } => return None,
    };
    match_nodes(rest, end_boundary, text, next)
}

/// Matches of one term in order, as (start, end) unit ranges (`String.matchAll` with the `gs` flags).
fn term_matches(term: &Term, text: &[u16]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i <= text.len() {
        if term.start && !at_boundary(text, i) {
            i += 1;
            continue;
        }
        match match_nodes(&term.nodes, term.end, text, i) {
            Some(end) if end > i => {
                out.push((i, end));
                i = end;
            }
            _ => i += 1,
        }
    }
    out
}

fn adjust_end(input: &[u16], orig_end: usize) -> usize {
    if orig_end + 1 < input.len()
        && (0xd800..0xdc00).contains(&input[orig_end])
        && (0xdc00..0xe000).contains(&input[orig_end + 1])
    {
        orig_end + 1
    } else {
        orig_end
    }
}

/// `matcher.hasMatch(input)`: any blacklisted match not inside a whitelisted interval.
pub fn has_match(input: &str) -> bool {
    let m = matcher();
    let units: Vec<u16> = input.encode_utf16().collect();

    // Whitelisted intervals (inclusive original indices).
    let (w_positions, w_text) = transform_whitelist(&units);
    let mut intervals: Vec<(usize, usize)> = Vec::new();
    for term in &m.whitelist {
        let mut last_end = 0;
        while let Some(start) = find(&w_text, term, last_end) {
            let orig_end = adjust_end(&units, w_positions[start + term.len() - 1]);
            intervals.push((w_positions[start], orig_end));
            last_end = start + term.len();
        }
    }
    intervals.sort_by_key(|i| i.0);
    let whitelisted = |lo: usize, hi: usize| {
        for &(a, b) in &intervals {
            if a > lo {
                break;
            }
            if a <= lo && hi <= b {
                return true;
            }
        }
        false
    };

    let (positions, text) = transform_blacklist(m, &units);
    for term in &m.blacklist {
        for (start, end) in term_matches(term, &text) {
            let orig_start = positions[start];
            let orig_end = adjust_end(&units, positions[end - 1]);
            if !whitelisted(orig_start, orig_end) {
                return true;
            }
        }
    }
    false
}

fn find(haystack: &[u16], needle: &[u16], from: usize) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (from..=haystack.len() - needle.len()).find(|&i| &haystack[i..i + needle.len()] == needle)
}

#[cfg(test)]
mod tests {
    use super::has_match;

    /// The matcher agrees with upstream's on generated names (leet speak, case, duplicates, noise, whitelisted words).
    #[test]
    fn agrees_with_upstream_matcher() {
        let cases: Vec<(String, bool)> =
            serde_json::from_str(include_str!("../../tests/Golden/Obscenity.json"))
                .expect("golden parses");
        let mut wrong = Vec::new();
        for (name, expected) in &cases {
            if has_match(name) != *expected {
                wrong.push(name.clone());
            }
        }
        assert!(
            wrong.is_empty(),
            "{} of {} differ: {:?}",
            wrong.len(),
            cases.len(),
            &wrong[..wrong.len().min(5)]
        );
    }
}
