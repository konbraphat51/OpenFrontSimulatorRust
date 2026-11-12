// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/EmbargoAllExecution.ts: starts or stops embargoes against everyone.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Execution::Execution::Execution;
use crate::Game::Game::{PlayerType, SmallId, Tick};
use crate::Game::GameImpl::GameImpl;

/// Embargoes (or releases) every non-bot, non-teammate player; all work happens in `init`.
pub struct EmbargoAllExecution {
    player: SmallId,
    action: String,
}

impl EmbargoAllExecution {
    /// `new EmbargoAllExecution(player, action)`.
    pub fn new(player: SmallId, action: String) -> Self {
        Self { player, action }
    }
}

impl Execution for EmbargoAllExecution {
    fn init(&mut self, game: &mut GameImpl, _ticks: Tick) {
        if !game.player(self.player).can_embargo_all() {
            return;
        }
        let me = self.player;
        for p in game.players() {
            if p == me {
                continue;
            }
            if game.player(p).player_type() == PlayerType::Bot {
                continue;
            }
            if game.player(me).is_on_same_team(p) {
                continue;
            }
            if game.config().rules().has(Change::EmbargoAllSkipsAllies5919)
                && game.player(me).is_allied_with(p)
            {
                continue;
            }
            let has = game.player(me).has_embargo_against(p);
            if self.action == "start" {
                if !has {
                    game.player_mut(me).add_embargo(p, false);
                }
            } else if has {
                game.player_mut(me).stop_embargo(p);
            }
        }
        game.player_mut(me).record_embargo_all();
    }

    fn tick(&mut self, _game: &mut GameImpl, _ticks: Tick) {}

    fn is_active(&self) -> bool {
        false
    }

    fn active_during_spawn_phase(&self) -> bool {
        false
    }
}
