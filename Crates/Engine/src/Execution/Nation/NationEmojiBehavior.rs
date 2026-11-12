// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/NationEmojiBehavior.ts: nation emoji decisions.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Builds::Rules::Change;
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::FLATTENED_EMOJI_TABLE;
use crate::Execution::EmojiExecution::EmojiExecution;
use crate::Execution::Nation::SharedRandom;
use crate::Game::Game::{
    ALL_PLAYERS, EmojiRecipient, GameMode, PlayerType, Relation, SmallId, Tick,
};
use crate::Game::GameImpl::{GameImpl, Winner};
use std::collections::HashMap;

fn emoji_ids(list: &[&str]) -> Vec<f64> {
    list.iter()
        .map(|e| {
            FLATTENED_EMOJI_TABLE
                .iter()
                .position(|x| x == e)
                .map_or(-1.0, |i| i as f64)
        })
        .collect()
}

/// `EMOJI_ASSIST_ACCEPT` as emoji ids.
pub fn emoji_assist_accept() -> Vec<f64> {
    emoji_ids(&["\u{1f44d}", "\u{1f91d}", "\u{1f3af}"])
}

/// `EMOJI_ASSIST_RELATION_TOO_LOW` as emoji ids.
pub fn emoji_assist_relation_too_low() -> Vec<f64> {
    emoji_ids(&["\u{1f971}", "\u{1f926}\u{200d}\u{2642}\u{fe0f}"])
}

/// `EMOJI_ASSIST_TARGET_ME` as emoji ids.
pub fn emoji_assist_target_me() -> Vec<f64> {
    emoji_ids(&["\u{1f97a}", "\u{1f480}"])
}

/// `EMOJI_ASSIST_TARGET_ALLY` as emoji ids.
pub fn emoji_assist_target_ally() -> Vec<f64> {
    emoji_ids(&["\u{1f54a}\u{fe0f}", "\u{1f44e}"])
}

/// `EMOJI_AGGRESSIVE_ATTACK` as emoji ids.
pub fn emoji_aggressive_attack() -> Vec<f64> {
    emoji_ids(&["\u{1f608}"])
}

/// `EMOJI_ATTACK` as emoji ids.
pub fn emoji_attack() -> Vec<f64> {
    emoji_ids(&["\u{1f621}"])
}

/// `EMOJI_WARSHIP_RETALIATION` as emoji ids.
pub fn emoji_warship_retaliation() -> Vec<f64> {
    emoji_ids(&["\u{26f5}"])
}

/// `EMOJI_NUKE` as emoji ids.
pub fn emoji_nuke() -> Vec<f64> {
    emoji_ids(&["\u{2622}\u{fe0f}", "\u{1f4a5}"])
}

/// `EMOJI_GOT_INSULTED` as emoji ids.
pub fn emoji_got_insulted() -> Vec<f64> {
    emoji_ids(&[
        "\u{1f595}",
        "\u{1f621}",
        "\u{1f921}",
        "\u{1f61e}",
        "\u{1f62d}",
    ])
}

/// `EMOJI_LOVE` as emoji ids.
pub fn emoji_love() -> Vec<f64> {
    emoji_ids(&["\u{2764}\u{fe0f}", "\u{1f60a}", "\u{1f970}"])
}

/// `EMOJI_CONFUSED` as emoji ids.
pub fn emoji_confused() -> Vec<f64> {
    emoji_ids(&["\u{2753}", "\u{1f921}"])
}

/// `EMOJI_VICTORY` as emoji ids.
pub fn emoji_victory() -> Vec<f64> {
    emoji_ids(&[
        "\u{1f451}",
        "\u{1f947}",
        "\u{1f60e}",
        "\u{1f4aa}",
        "\u{1f600}",
    ])
}

/// `EMOJI_BRAG` as emoji ids.
pub fn emoji_brag() -> Vec<f64> {
    emoji_ids(&["\u{1f451}", "\u{1f947}", "\u{1f4aa}"])
}

/// `EMOJI_CHARM_ALLIES` as emoji ids.
pub fn emoji_charm_allies() -> Vec<f64> {
    emoji_ids(&["\u{1f91d}", "\u{1f607}", "\u{1f4aa}"])
}

/// `EMOJI_CLOWN` as emoji ids.
pub fn emoji_clown() -> Vec<f64> {
    emoji_ids(&["\u{1f921}", "\u{1f926}\u{200d}\u{2642}\u{fe0f}"])
}

/// `EMOJI_RAT` as emoji ids.
pub fn emoji_rat() -> Vec<f64> {
    emoji_ids(&["\u{1f400}"])
}

/// `EMOJI_OVERWHELMED` as emoji ids.
pub fn emoji_overwhelmed() -> Vec<f64> {
    emoji_ids(&[
        "\u{1f480}",
        "\u{1f198}",
        "\u{1f631}",
        "\u{1f97a}",
        "\u{1f62d}",
        "\u{1f61e}",
        "\u{1fae1}",
        "\u{1f44b}",
    ])
}

/// `EMOJI_CONGRATULATE` as emoji ids.
pub fn emoji_congratulate() -> Vec<f64> {
    emoji_ids(&["\u{1f44f}"])
}

/// `EMOJI_SCARED_OF_THREAT` as emoji ids.
pub fn emoji_scared_of_threat() -> Vec<f64> {
    emoji_ids(&["\u{1f64f}", "\u{1f97a}"])
}

/// `EMOJI_BORED` as emoji ids.
pub fn emoji_bored() -> Vec<f64> {
    emoji_ids(&["\u{1f971}"])
}

/// `EMOJI_HANDSHAKE` as emoji ids.
pub fn emoji_handshake() -> Vec<f64> {
    emoji_ids(&["\u{1f91d}"])
}

/// `EMOJI_DONATION_OK` as emoji ids.
pub fn emoji_donation_ok() -> Vec<f64> {
    emoji_ids(&["\u{1f44d}"])
}

/// `EMOJI_DONATION_TOO_SMALL` as emoji ids.
pub fn emoji_donation_too_small() -> Vec<f64> {
    emoji_ids(&["\u{2753}", "\u{1f971}"])
}

/// `EMOJI_GREET` as emoji ids.
pub fn emoji_greet() -> Vec<f64> {
    emoji_ids(&["\u{1f44b}"])
}

/// First player of `players` with the most tiles (stable descending sort, element 0).
fn most_tiles(game: &GameImpl, players: &[SmallId]) -> Option<SmallId> {
    let mut best: Option<SmallId> = None;
    for &p in players {
        match best {
            Some(b) if game.player(p).num_tiles_owned() <= game.player(b).num_tiles_owned() => {}
            _ => best = Some(p),
        }
    }
    best
}

/// Decides when a nation sends emojis.
pub struct NationEmojiBehavior {
    random: SharedRandom,
    player: SmallId,
    last_emoji_sent: HashMap<SmallId, Tick>,
    game_over: bool,
}

impl NationEmojiBehavior {
    /// `new NationEmojiBehavior(random, game, player)`.
    pub fn new(random: SharedRandom, _game: &GameImpl, player: SmallId) -> Self {
        Self {
            random,
            player,
            last_emoji_sent: HashMap::new(),
            game_over: false,
        }
    }

    /// `maybeSendCasualEmoji()`.
    pub fn maybe_send_casual_emoji(&mut self, game: &mut GameImpl) {
        if self.game_over && game.config().rules().has(Change::NationAiRework2984) {
            return;
        }
        self.check_overwhelmed_by_attacks(game);
        self.check_very_small_attack(game);
        self.congratulate_winner(game);
        self.brag(game);
        self.charm_allies(game);
        self.annoy_traitors(game);
        self.find_rat(game);
        self.greet_nearby_players(game);
    }

    fn chance(&self, odds: f64) -> bool {
        self.random.borrow_mut().chance(odds)
    }

    fn check_overwhelmed_by_attacks(&mut self, game: &mut GameImpl) {
        if !self.chance(16.0) {
            return;
        }
        let incoming = game.player(self.player).incoming_attacks();
        if incoming.is_empty() {
            return;
        }
        let incoming_troops = incoming
            .iter()
            .fold(0.0, |sum, &a| sum + game.attack(a).troops());
        let our_troops = game.player(self.player).troops();
        // Overwhelmed when incoming troops are at least 3x ours.
        if incoming_troops >= our_troops * 3.0 {
            self.send_emoji(game, EmojiRecipient::All, &emoji_overwhelmed());
        }
    }

    fn check_very_small_attack(&mut self, game: &mut GameImpl) {
        if !self.chance(8.0) {
            return;
        }
        let incoming = game.player(self.player).incoming_attacks();
        if incoming.is_empty() {
            return;
        }
        let our_troops = game.player(self.player).troops();
        if our_troops <= 0.0 {
            return;
        }
        for attack in incoming {
            let attacker = game.attack(attack).attacker();
            if game.player(attacker).player_type() != PlayerType::Human {
                continue;
            }
            if game.attack(attack).troops() < our_troops * 0.1 {
                let list = if self.chance(2.0) {
                    emoji_confused()
                } else {
                    emoji_bored()
                };
                self.maybe_send_emoji(game, EmojiRecipient::Player(attacker), &list);
            }
        }
    }

    /// `congratulateWinner` before PR 2984: the winner is whoever holds the tile share needed to win.
    fn congratulate_winner_v29(&mut self, game: &mut GameImpl) {
        if self.game_over {
            return;
        }
        let percent_to_win = game
            .config()
            .percentage_tiles_owned_to_win(game.elapsed_game_seconds());
        let tiles_without_fallout = game.num_land_tiles() - game.num_tiles_with_fallout();
        let is_team_game = game.config().game_config().game_mode == GameMode::Team;
        if is_team_game {
            let mut team_tiles: Vec<(String, f64)> = Vec::new();
            for p in game.players() {
                let Some(team) = game.player(p).team().cloned() else {
                    continue;
                };
                let tiles = game.player(p).num_tiles_owned() as f64;
                match team_tiles.iter_mut().find(|(t, _)| *t == team) {
                    Some(entry) => entry.1 += tiles,
                    None => team_tiles.push((team, tiles)),
                }
            }
            // Stable descending sort: the first team of the maximum wins ties.
            team_tiles.sort_by(|a, b| b.1.total_cmp(&a.1));
            let Some((winning_team, winning_tiles)) = team_tiles.first().cloned() else {
                return;
            };
            if (winning_tiles / tiles_without_fallout) * 100.0 < percent_to_win {
                return;
            }
            self.game_over = true;
            if game.player(self.player).team() == Some(&winning_team) {
                return;
            }
            self.send_emoji(game, EmojiRecipient::All, &emoji_congratulate());
        } else {
            let mut sorted = game.players();
            sorted.sort_by(|&a, &b| {
                game.player(b)
                    .num_tiles_owned()
                    .cmp(&game.player(a).num_tiles_owned())
            });
            let Some(&first_place) = sorted.first() else {
                return;
            };
            let first_place_percent =
                (game.player(first_place).num_tiles_owned() as f64 / tiles_without_fallout) * 100.0;
            if first_place_percent < percent_to_win {
                return;
            }
            self.game_over = true;
            if game.player(first_place).player_type() != PlayerType::Human {
                return;
            }
            let nations: Vec<SmallId> = game
                .players()
                .into_iter()
                .filter(|&p| game.player(p).player_type() == PlayerType::Nation)
                .collect();
            if most_tiles(game, &nations) != Some(self.player) {
                return;
            }
            self.send_emoji(
                game,
                EmojiRecipient::Player(first_place),
                &emoji_congratulate(),
            );
        }
    }

    fn congratulate_winner(&mut self, game: &mut GameImpl) {
        if !game.config().rules().has(Change::NationAiRework2984) {
            self.congratulate_winner_v29(game);
            return;
        }
        let Some(winner) = game.winner().cloned() else {
            return;
        };
        self.game_over = true;
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            let won = match &winner {
                Winner::Player(p) => *p == self.player,
                Winner::Team(t) => game.player(self.player).team() == Some(t),
            };
            if won {
                // Half of the winning nations celebrate.
                if self.chance(2.0) {
                    self.send_emoji(game, EmojiRecipient::All, &emoji_victory());
                }
                return;
            }
        }
        let is_team_game = game.config().game_config().game_mode == GameMode::Team;
        if is_team_game {
            // Do not congratulate our own team.
            if !game.config().rules().has(Change::NationsLikeHumans5899)
                && let Winner::Team(t) = &winner
                && game.player(self.player).team() == Some(t)
            {
                return;
            }
            self.send_emoji(game, EmojiRecipient::All, &emoji_congratulate());
        } else {
            let Winner::Player(winner) = winner else {
                return;
            };
            // Only the largest nation congratulates.
            let nations: Vec<SmallId> = game
                .players()
                .into_iter()
                .filter(|&p| game.player(p).player_type() == PlayerType::Nation)
                .collect();
            if most_tiles(game, &nations) != Some(self.player) {
                return;
            }
            self.send_emoji(game, EmojiRecipient::Player(winner), &emoji_congratulate());
        }
    }

    fn brag(&mut self, game: &mut GameImpl) {
        if self.game_over && !game.config().rules().has(Change::NationAiRework2984) {
            return;
        }
        if !self.chance(300.0) {
            return;
        }
        let players = game.players();
        if most_tiles(game, &players) != Some(self.player) {
            return;
        }
        self.send_emoji(game, EmojiRecipient::All, &emoji_brag());
    }

    fn charm_allies(&mut self, game: &mut GameImpl) {
        if !self.chance(250.0) {
            return;
        }
        let human_allies: Vec<SmallId> = game
            .player(self.player)
            .allies()
            .into_iter()
            .filter(|&p| game.player(p).player_type() == PlayerType::Human)
            .collect();
        if human_allies.is_empty() {
            return;
        }
        let ally = *self.random.borrow_mut().rand_element(&human_allies);
        let list = if self.chance(3.0) {
            emoji_love()
        } else {
            emoji_charm_allies()
        };
        self.send_emoji(game, EmojiRecipient::Player(ally), &list);
    }

    fn annoy_traitors(&mut self, game: &mut GameImpl) {
        if game.config().rules().has(Change::NationsLikeHumans5899) {
            if !self.chance(3.0) {
                return;
            }
            for traitor in game.players() {
                if game.player(traitor).player_type() != PlayerType::Human
                    || !game.player(traitor).is_traitor()
                {
                    continue;
                }
                // Only one nation mocks each traitor, otherwise every nation on the map piles on.
                if self.traitor_mocker(game, traitor) != Some(self.player) {
                    continue;
                }
                self.send_emoji(game, EmojiRecipient::Player(traitor), &emoji_clown());
            }
            return;
        }
        if !self.chance(40.0) {
            return;
        }
        let me = self.player;
        let traitors: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                game.player(p).player_type() == PlayerType::Human
                    && !game.player(p).is_friendly(me, false)
                    && game.player(p).is_traitor()
            })
            .collect();
        if traitors.is_empty() {
            return;
        }
        let traitor = *self.random.borrow_mut().rand_element(&traitors);
        self.send_emoji(game, EmojiRecipient::Player(traitor), &emoji_clown());
    }

    /// The most hostile nation (usually the betrayed one), first in player order on ties.
    fn traitor_mocker(&self, game: &GameImpl, traitor: SmallId) -> Option<SmallId> {
        let mut mocker: Option<SmallId> = None;
        for p in game.players() {
            if game.player(p).player_type() != PlayerType::Nation
                || game.player(p).is_friendly(traitor, false)
            {
                continue;
            }
            if mocker.is_none_or(|m| {
                (game.player(p).relation(traitor) as u8) < (game.player(m).relation(traitor) as u8)
            }) {
                mocker = Some(p);
            }
        }
        mocker
    }

    fn find_rat(&mut self, game: &mut GameImpl) {
        // Ignore the first 10 minutes (everybody is small early on).
        if game.ticks() < 6000 {
            return;
        }
        if !self.chance(10000.0) {
            return;
        }
        let threshold = game.num_land_tiles() * 0.01;
        let small_players: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| {
                let n = game.player(p).num_tiles_owned() as f64;
                game.player(p).player_type() == PlayerType::Human && n < threshold && n > 0.0
            })
            .collect();
        if small_players.is_empty() {
            return;
        }
        let small = *self.random.borrow_mut().rand_element(&small_players);
        self.send_emoji(game, EmojiRecipient::Player(small), &emoji_rat());
    }

    fn greet_nearby_players(&mut self, game: &mut GameImpl) {
        if !game.config().rules().has(Change::NationBehaviorUpdate3246) {
            return;
        }
        // Only in the first minute.
        if game.ticks() > 600 {
            return;
        }
        if !self.chance(250.0) {
            return;
        }
        let nearby_humans: Vec<SmallId> = game
            .player(self.player)
            .nearby()
            .into_iter()
            .filter(|&p| p != 0 && game.player(p).player_type() == PlayerType::Human)
            .collect();
        if nearby_humans.is_empty() {
            return;
        }
        let neighbor = *self.random.borrow_mut().rand_element(&nearby_humans);
        self.send_emoji(game, EmojiRecipient::Player(neighbor), &emoji_greet());
    }

    /// `maybeSendEmoji(otherPlayer, emojisList)`; emoji ids are indices of `FLATTENED_EMOJI_TABLE`.
    pub fn maybe_send_emoji(&mut self, game: &mut GameImpl, other: EmojiRecipient, emojis: &[f64]) {
        if !self.should_send_emoji(game, other, true) {
            return;
        }
        self.send_emoji(game, other, emojis);
    }

    /// `maybeSendAttackEmoji(otherPlayer)`.
    pub fn maybe_send_attack_emoji(&mut self, game: &mut GameImpl, other: SmallId) {
        if !self.should_send_emoji(game, EmojiRecipient::Player(other), true) {
            return;
        }
        // A good relation means we probably attack first (aggressive), else we retaliate.
        if game.player(self.player).relation(other) >= Relation::Neutral {
            if !self.chance(2.0) {
                return;
            }
            self.send_emoji(
                game,
                EmojiRecipient::Player(other),
                &emoji_aggressive_attack(),
            );
            return;
        }
        if !self.chance(4.0) {
            return;
        }
        self.send_emoji(game, EmojiRecipient::Player(other), &emoji_attack());
    }

    /// `sendEmoji(otherPlayer, emojisList)`.
    pub fn send_emoji(&mut self, game: &mut GameImpl, other: EmojiRecipient, emojis: &[f64]) {
        if !self.should_send_emoji(game, other, false) {
            return;
        }
        if !game.player(self.player).can_send_emoji(other) {
            return;
        }
        let recipient_id = match other {
            EmojiRecipient::All => ALL_PLAYERS.to_string(),
            EmojiRecipient::Player(p) => game.player(p).id().to_string(),
        };
        let emoji = *self.random.borrow_mut().rand_element(emojis);
        game.add_execution(Box::new(EmojiExecution::new(
            self.player,
            recipient_id,
            emoji,
        )));
    }

    fn should_send_emoji(
        &mut self,
        game: &GameImpl,
        other: EmojiRecipient,
        limit_emojis_by_time: bool,
    ) -> bool {
        let EmojiRecipient::Player(other) = other else {
            return true;
        };
        if game.player(self.player).player_type() == PlayerType::Bot {
            return false;
        }
        if game.player(other).player_type() != PlayerType::Human {
            return false;
        }
        if limit_emojis_by_time {
            let last_sent = self
                .last_emoji_sent
                .get(&other)
                .map_or(-300.0, |&t| t as f64);
            if game.ticks() as f64 - last_sent <= 300.0 {
                return false;
            }
            self.last_emoji_sent.insert(other, game.ticks());
        }
        true
    }
}

/// `respondToMIRV`: a MIRV target may answer with an overwhelmed emoji.
pub fn respond_to_mirv(game: &mut GameImpl, random: &mut PseudoRandom, mirv_target: SmallId) {
    if !random.chance(8.0) {
        return;
    }
    if !game.player(mirv_target).can_send_emoji(EmojiRecipient::All) {
        return;
    }
    let e = *random.rand_element(&emoji_overwhelmed());
    game.add_execution(Box::new(EmojiExecution::new(
        mirv_target,
        ALL_PLAYERS.to_string(),
        e,
    )));
}
