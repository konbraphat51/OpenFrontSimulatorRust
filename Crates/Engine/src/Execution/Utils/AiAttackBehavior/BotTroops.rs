// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/utils/AiAttackBehavior.ts (part: bottroops): bot attack sizing and donations.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use super::*;

impl AiAttackBehavior {
    pub(super) fn calculate_bot_attack_troops(
        &mut self,
        game: &GameImpl,
        target: SmallId,
        max_troops: f64,
    ) -> f64 {
        let old_accounting = !game.has(Change::NationExpansionCap5290);
        if game.config().game_config().difficulty == Difficulty::Easy {
            if old_accounting {
                self.bot_attack_troops_sent += max_troops;
            }
            return max_troops;
        }
        if Self::v2(game) && game.config().game_config().difficulty != Difficulty::Medium {
            // Top up what already attacks the bot, or wait until we can send it all.
            let troops = self.bot_attack_troops_needed(game, target) * 1.3
                - self.our_stack_against(game, target);
            return if troops <= max_troops { troops } else { 0.0 };
        }
        let target_troops = game.player(target).troops();
        let mut troops = target_troops * 4.0;
        // Keep the reserve: do not send more than `max_troops`.
        if troops > max_troops {
            // Too few troops left for a big enough bot attack: skip it.
            if max_troops < target_troops * 2.0 {
                troops = 0.0;
            } else {
                troops = max_troops;
            }
        }
        if old_accounting {
            self.bot_attack_troops_sent += troops;
        }
        troops
    }

    /// Keeps the attack at its cheapest loss rate until the bot collapses below 100 tiles.
    pub(super) fn bot_attack_troops_needed(&self, game: &GameImpl, bot: SmallId) -> f64 {
        let stack = f64::max(1.0, game.player(bot).troops() / CHEAPEST_TROOP_RATIO);
        let tiles = f64::max(0.0, game.player(bot).num_tiles_owned() as f64 - 100.0);
        f64::max(stack, tiles * self.bot_tile_loss(game, bot, stack))
    }

    /// Troops lost per tile at that stack: Impossible reads the terrain on a 6x6 grid over the
    /// bot's land, Hard assumes highland.
    pub(super) fn bot_tile_loss(&self, game: &GameImpl, bot: SmallId, stack: f64) -> f64 {
        let mut terrains: Vec<TerrainType> = Vec::new();
        let bx = game.player(bot).largest_cluster_bounding_box;
        let difficulty = game.config().game_config().difficulty;
        if difficulty == Difficulty::Impossible
            && let Some((min, max)) = bx
        {
            let width = (max.x - min.x + 1) as f64;
            let height = (max.y - min.y + 1) as f64;
            for i in 0..6 {
                for j in 0..6 {
                    let x = min.x + (((2 * i + 1) as f64 * width) / 12.0).floor() as i32;
                    let y = min.y + (((2 * j + 1) as f64 * height) / 12.0).floor() as i32;
                    let tile = game.tile_ref(x, y);
                    if game.map.owner_id(tile) == bot {
                        terrains.push(game.map.terrain_type(tile));
                    }
                }
            }
        }
        if terrains.is_empty() {
            terrains.push(TerrainType::Highland);
        }
        let me = game.player(self.player);
        let b = game.player(bot);
        let mut loss = 0.0;
        for &terrain in &terrains {
            loss += game
                .config()
                .attack_logic(&AttackLogicInput {
                    terrain,
                    attack_troops: stack,
                    attacker: AttackLogicAttacker {
                        player_type: me.player_type(),
                        num_tiles: me.num_tiles_owned() as f64,
                    },
                    defender: Some(AttackLogicDefender {
                        player_type: b.player_type(),
                        num_tiles: b.num_tiles_owned() as f64,
                        troops: b.troops(),
                        is_traitor: b.is_traitor(),
                        is_disconnected_teammate: false,
                    }),
                    defender_has_defense_post: false,
                    fallout_ratio: None,
                    border_size: 1.0,
                })
                .attacker_troop_loss;
        }
        loss / terrains.len() as f64
    }

    /// Land attacks on the same target merge, so these troops join the next one we send, while the
    /// target's attacks on us cancel out against it.
    pub(super) fn our_stack_against(&self, game: &GameImpl, target: SmallId) -> f64 {
        let mut troops = 0.0;
        for &attack in game.player(self.player).outgoing_attacks() {
            let a = game.attack(attack);
            if a.target() == target && a.source_tile().is_none() {
                troops += a.troops();
            }
        }
        for attack in game.player(self.player).incoming_attacks() {
            if game.attack(attack).attacker() == target {
                troops -= game.attack(attack).troops();
            }
        }
        troops
    }

    pub(super) fn donate_troops(&mut self, game: &mut GameImpl) -> bool {
        // Only in team games.
        if game.config().game_config().game_mode != GameMode::Team {
            return false;
        }
        // Not in public games (to balance HvN).
        if game.config().game_config().game_type == GameType::Public
            && game.has(Change::NoPublicDonation3437)
        {
            return false;
        }
        if !game.config().donate_troops() {
            return false;
        }
        if !Self::v2(game) && game.winner().is_some() {
            return false;
        }
        match game.config().game_config().difficulty {
            Difficulty::Easy => return false,
            Difficulty::Medium => {
                if !self.chance(4.0) {
                    return false;
                }
            }
            Difficulty::Hard => {
                if !self.chance(2.0) {
                    return false;
                }
            }
            Difficulty::Impossible => {}
        }

        // Teammates currently in combat.
        let teammates: Vec<SmallId> = game
            .players()
            .into_iter()
            .filter(|&p| game.player(self.player).is_on_same_team(p))
            .filter(|&p| {
                !game.player(p).incoming_attacks().is_empty()
                    || !game.player(p).outgoing_attacks().is_empty()
            })
            .collect();
        if teammates.is_empty() {
            return false;
        }

        // Lowest troop percentage first.
        let mut with_percentage: Vec<(SmallId, f64)> = teammates
            .into_iter()
            .map(|t| {
                let max_troops = game.player(t).max_troops();
                (t, game.player(t).troops() / f64::max(max_troops, 1.0))
            })
            .collect();
        sort_by_key_f64(&mut with_percentage, |&(_, p)| p);

        let selected = with_percentage
            .iter()
            .map(|&(t, _)| t)
            .find(|&t| game.player(self.player).can_donate_troops(t));
        let Some(selected) = selected else {
            return false;
        };

        // Donate a portion of our troops, keeping the reserve.
        let max_troops = game.player(self.player).max_troops();
        let troops_to_keep = max_troops * self.reserve_ratio;
        let available_troops = self.home_troops(game) - troops_to_keep;
        if available_troops < 1.0 {
            return false;
        }
        let selected_id = game.player(selected).id().to_string();
        let exec = Box::new(DonateTroopsExecution::new(
            self.player,
            selected_id,
            Some(available_troops),
        ));
        self.send(game, exec, available_troops);
        true
    }
}
