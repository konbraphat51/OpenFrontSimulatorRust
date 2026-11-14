// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/execution/nation/BotAnnexation.ts: which bots a nation attacks to have the engine annex the rest.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.

use crate::Game::Game::{PlayerType, SmallId};
use crate::Game::GameImpl::GameImpl;

/// What a bot's territory touches, seen from the player planning to take it.
struct BotSurroundings {
    bot: SmallId,
    borders_us: bool,
    /// Touches water, the map edge, free land or anyone but us and bots we may attack.
    open: bool,
    /// The other bots it borders, in discovery order.
    bots: Vec<SmallId>,
}

/// Bots that border only us and are not under our attack yet: any attack makes the engine annex them.
pub fn walled_in_bots(game: &GameImpl, player: SmallId, bots: &[SmallId]) -> Vec<SmallId> {
    let attacking = attacked_by(game, player);
    bots.iter()
        .copied()
        .filter(|&bot| {
            !attacking.contains(&bot)
                && scan(game, player, bot, true, Some(&[])).is_some_and(|s| s.borders_us)
        })
        .collect()
}

/// Bots to attack, best first: a checkerboard of bots across our front and the row behind it is
/// left to be annexed for free, and the bots walling them in go first.
pub fn plan_bot_attacks(game: &GameImpl, player: SmallId, bots: &[SmallId]) -> Vec<SmallId> {
    // Insertion-ordered map of bot -> surroundings.
    let mut region: Vec<BotSurroundings> = Vec::new();
    for &bot in bots {
        region.push(scan(game, player, bot, false, None).expect("never gives up"));
    }
    let find = |region: &[BotSurroundings], bot: SmallId| region.iter().position(|s| s.bot == bot);
    let front: Vec<SmallId> = bots
        .iter()
        .copied()
        .filter(|&bot| region[find(&region, bot).expect("in region")].borders_us)
        .collect();
    // Open bots behind the front are never kept, so only the closed ones are needed.
    let mut behind: Vec<SmallId> = Vec::new();
    for &bot in &front {
        let others = region[find(&region, bot).expect("in region")].bots.clone();
        for other in others {
            if find(&region, other).is_some() || behind.contains(&other) {
                continue;
            }
            behind.push(other);
            if let Some(s) = scan(game, player, other, true, None) {
                match find(&region, other) {
                    Some(i) => region[i] = s,
                    None => region.push(s),
                }
            }
        }
    }
    let attacking = attacked_by(game, player);

    // Largest first, bots that do not border each other; each neighbour earns a share of the land it walls in.
    let mut candidates: Vec<&BotSurroundings> = region
        .iter()
        .filter(|s| {
            !s.open
                && !s.bots.is_empty()
                && !attacking.contains(&s.bot)
                && !owns_structures(game, s.bot)
        })
        .collect();
    candidates.sort_by(|a, b| {
        game.player(b.bot)
            .num_tiles_owned()
            .cmp(&game.player(a.bot).num_tiles_owned())
    });
    let mut kept: Vec<SmallId> = Vec::new();
    let mut next_to_kept: Vec<SmallId> = Vec::new();
    let mut walled_in: Vec<(SmallId, f64)> = Vec::new();
    for s in candidates {
        if next_to_kept.contains(&s.bot) {
            continue;
        }
        kept.push(s.bot);
        let share = game.player(s.bot).num_tiles_owned() as f64 / s.bots.len() as f64;
        for &other in &s.bots {
            if !next_to_kept.contains(&other) {
                next_to_kept.push(other);
            }
            match walled_in.iter_mut().find(|(b, _)| *b == other) {
                Some(entry) => entry.1 += share,
                None => walled_in.push((other, share)),
            }
        }
    }

    // Most land per bot troop, counting the land it walls in.
    let value = |bot: SmallId| {
        let walled = walled_in
            .iter()
            .find(|(b, _)| *b == bot)
            .map_or(0.0, |(_, v)| *v);
        (game.player(bot).num_tiles_owned() as f64 + walled)
            / f64::max(1.0, game.player(bot).troops())
    };
    let by_value = |a: &SmallId, b: &SmallId| {
        value(*b)
            .partial_cmp(&value(*a))
            .unwrap_or(std::cmp::Ordering::Equal)
    };
    let mut targets: Vec<SmallId> = bots
        .iter()
        .copied()
        .filter(|&bot| {
            let s = &region[find(&region, bot).expect("in region")];
            let walled = s.borders_us && !s.open && s.bots.is_empty();
            !kept.contains(&bot) && !walled
        })
        .collect();
    targets.sort_by(by_value);
    // Behind water or the map edge, kept bots can be all we border: then they are the way on.
    if !targets
        .iter()
        .any(|&bot| region[find(&region, bot).expect("in region")].borders_us)
    {
        let mut extra: Vec<SmallId> = front
            .iter()
            .copied()
            .filter(|bot| kept.contains(bot))
            .collect();
        extra.sort_by(by_value);
        targets.extend(extra);
    }
    targets
}

/// Players our outgoing attacks target.
fn attacked_by(game: &GameImpl, player: SmallId) -> Vec<SmallId> {
    let mut targets: Vec<SmallId> = Vec::new();
    for &attack in game.player(player).outgoing_attacks() {
        let target = game.attack(attack).target();
        if target != 0 && !targets.contains(&target) {
            targets.push(target);
        }
    }
    targets
}

fn owns_structures(game: &GameImpl, player: SmallId) -> bool {
    game.player(player)
        .units()
        .iter()
        .any(|&u| game.unit_any(u).unit_type().is_structure())
}

/// A bot we may attack, or None for anyone else.
fn attackable_bot(game: &GameImpl, player: SmallId, small_id: SmallId) -> Option<SmallId> {
    if small_id != 0
        && game.player(small_id).player_type() == PlayerType::Bot
        && !game.player(player).is_friendly(small_id, false)
    {
        Some(small_id)
    } else {
        None
    }
}

/// Mirrors `PlayerExecution`'s surrounded check: every land tile next to the bot's border counts.
/// Gives up (None) once the bot turns out open if `closed_only`, or borders a bot outside `within`.
fn scan(
    game: &GameImpl,
    player: SmallId,
    bot: SmallId,
    closed_only: bool,
    within: Option<&[SmallId]>,
) -> Option<BotSurroundings> {
    let map = &game.map;
    let mut s = BotSurroundings {
        bot,
        borders_us: false,
        open: false,
        bots: Vec::new(),
    };
    let mut nbuf = [0; 4];
    let mut gave_up = false;
    for tile in game.player(bot).border_tiles().iter() {
        if gave_up {
            break;
        }
        if map.is_ocean_shore(tile) || map.is_on_edge_of_map(tile) {
            s.open = true;
            gave_up = closed_only;
            continue;
        }
        let n = map.neighbors4(tile, &mut nbuf);
        for &neighbor in &nbuf[..n] {
            let id = map.owner_id(neighbor);
            if id == bot {
                continue;
            }
            if id == player {
                s.borders_us = true;
                continue;
            }
            let other = if id == 0 {
                None
            } else {
                attackable_bot(game, player, id)
            };
            match other {
                None => {
                    s.open = true;
                    gave_up = closed_only;
                }
                Some(other) => {
                    if within.is_some_and(|w| !w.contains(&other)) {
                        gave_up = true;
                    } else if !s.bots.contains(&other) {
                        s.bots.push(other);
                    }
                }
            }
            if gave_up {
                break;
            }
        }
    }
    if gave_up { None } else { Some(s) }
}
