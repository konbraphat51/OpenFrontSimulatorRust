// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationUtils.ts: helpers shared by nation behaviours.
//! STUB(owner=N1): replaced at merge.

use crate::Game::Game::SmallId;
use crate::Game::GameImpl::GameImpl;

/// `findJuiciestTarget(game, candidates)`.
pub fn find_juiciest_target(_game: &GameImpl, _candidates: &[SmallId]) -> Option<SmallId> {
    panic!("not ported: NationUtils::find_juiciest_target");
}
