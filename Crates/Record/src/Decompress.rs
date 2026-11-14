// SPDX-License-Identifier: AGPL-3.0-only
//! Port of `decompressGameRecord` (src/core/Util.ts): expands sparse turns into one turn per tick.

use crate::Records::{GameRecord, Turn};

impl GameRecord {
    /// Port of `decompressGameRecord`: fills the turn numbers skipped by the sparse list with empty turns
    /// and pads up to `info.num_turns`, keeping upstream's loop semantics (including duplicate turn numbers).
    pub fn decompress(&mut self) {
        let mut turns: Vec<Turn> = Vec::with_capacity(self.info.num_turns as usize);
        let mut last_turn_num: i64 = -1;
        for turn in std::mem::take(&mut self.turns) {
            while last_turn_num < turn.turn_number as i64 - 1 {
                last_turn_num += 1;
                turns.push(Self::empty_turn(last_turn_num as u32));
            }
            last_turn_num = turn.turn_number as i64;
            turns.push(turn);
        }
        for i in turns.len() as u32..self.info.num_turns {
            turns.push(Self::empty_turn(i));
        }
        self.turns = turns;
    }

    fn empty_turn(turn_number: u32) -> Turn {
        Turn {
            turn_number,
            intents: Vec::new(),
            hash: None,
        }
    }
}
