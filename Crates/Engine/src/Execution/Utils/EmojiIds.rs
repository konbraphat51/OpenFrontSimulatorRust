// SPDX-License-Identifier: AGPL-3.0-only
//! Emoji id lists of nation/NationEmojiBehavior.ts that the land executions pick from.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Core::Util::FLATTENED_EMOJI_TABLE;

/// `list.map(emojiId)`: indices into the flattened emoji table (-1 when absent, like `indexOf`).
pub fn emoji_ids(list: &[&str]) -> Vec<f64> {
    list.iter()
        .map(|e| {
            FLATTENED_EMOJI_TABLE
                .iter()
                .position(|x| x == e)
                .map_or(-1.0, |i| i as f64)
        })
        .collect()
}

/// `EMOJI_ASSIST_ACCEPT`.
pub fn emoji_assist_accept() -> Vec<f64> {
    emoji_ids(&["\u{1f44d}", "\u{1f91d}", "\u{1f3af}"])
}

/// `EMOJI_ASSIST_RELATION_TOO_LOW`.
pub fn emoji_assist_relation_too_low() -> Vec<f64> {
    emoji_ids(&["\u{1f971}", "\u{1f926}\u{200d}\u{2642}\u{fe0f}"])
}

/// `EMOJI_ASSIST_TARGET_ME`.
pub fn emoji_assist_target_me() -> Vec<f64> {
    emoji_ids(&["\u{1f97a}", "\u{1f480}"])
}

/// `EMOJI_ASSIST_TARGET_ALLY`.
pub fn emoji_assist_target_ally() -> Vec<f64> {
    emoji_ids(&["\u{1f54a}\u{fe0f}", "\u{1f44e}"])
}

/// `EMOJI_LOVE`.
pub fn emoji_love() -> Vec<f64> {
    emoji_ids(&["\u{2764}\u{fe0f}", "\u{1f60a}", "\u{1f970}"])
}

/// `EMOJI_DONATION_OK`.
pub fn emoji_donation_ok() -> Vec<f64> {
    emoji_ids(&["\u{1f44d}"])
}

/// `EMOJI_DONATION_TOO_SMALL`.
pub fn emoji_donation_too_small() -> Vec<f64> {
    emoji_ids(&["\u{2753}", "\u{1f971}"])
}
