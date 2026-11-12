// SPDX-License-Identifier: AGPL-3.0-only
//! Port of src/core/game/TeamAssignment.ts: team list resolution and player to team assignment.
//! Upstream: OpenFrontIO (© OpenFront and Contributors), translated to Rust.
//!
//! Upstream returns a `Map<PlayerInfo, Team | "kicked">` whose insertion order decides the players'
//! small ids; here the result is a vector of `(input index, team)` in that same order, with `None`
//! standing for "kicked". `assignTeamsLobbyPreview` is client-only and not ported.

use super::Game::{
    COLORED_TEAMS, DUOS, HUMANS_VS_NATIONS, PlayerInfo, PlayerType, QUADS, TRIOS, Team,
    TeamCountConfig,
};
use crate::Core::PseudoRandom::PseudoRandom;
use crate::Core::Util::simple_hash;
use std::collections::{HashMap, HashSet};

fn colored(name: &str) -> Team {
    debug_assert!(COLORED_TEAMS.contains(&name));
    name.to_string()
}

/// `getMaxTeamSize`.
pub fn get_max_team_size(num_players: usize, num_teams: usize) -> usize {
    num_players.div_ceil(num_teams)
}

/// `assignTeams`; the result is in upstream insertion order, `None` is "kicked".
pub fn assign_teams(
    players: &[PlayerInfo],
    teams: &[Team],
    is_duos_trios_quads: bool,
    max_team_size: Option<usize>,
) -> Vec<(usize, Option<Team>)> {
    let max_team_size =
        max_team_size.unwrap_or_else(|| get_max_team_size(players.len(), teams.len()));
    let mut result: Vec<(usize, Option<Team>)> = Vec::new();
    let mut team_player_count: HashMap<Team, usize> = HashMap::new();

    // Server-pinned team slots are honored first and seed the counts.
    let mut unpinned: Vec<usize> = Vec::new();
    for (i, p) in players.iter().enumerate() {
        let pinned = p.team_index.and_then(|ti| teams.get(ti as usize));
        match pinned {
            None => unpinned.push(i),
            Some(team) => {
                result.push((i, Some(team.clone())));
                *team_player_count.entry(team.clone()).or_insert(0) += 1;
            }
        }
    }

    // Clans are strict: one team together, overflow members are kicked.
    let mut clan_order: Vec<String> = Vec::new();
    let mut clan_groups: HashMap<String, Vec<usize>> = HashMap::new();
    let mut non_clan_players: Vec<usize> = Vec::new();
    for &i in &unpinned {
        match players[i].clan_tag.as_deref() {
            Some(tag) if !tag.is_empty() => {
                if !clan_groups.contains_key(tag) {
                    clan_order.push(tag.to_string());
                }
                clan_groups.entry(tag.to_string()).or_default().push(i);
            }
            _ => non_clan_players.push(i),
        }
    }
    let mut sorted_clans: Vec<&Vec<usize>> = clan_order.iter().map(|t| &clan_groups[t]).collect();
    sorted_clans.sort_by_key(|c| std::cmp::Reverse(c.len()));
    for clan in sorted_clans {
        let mut team: Option<&Team> = None;
        let mut team_size = 0usize;
        for t in teams {
            let p = team_player_count.get(t).copied().unwrap_or(0);
            if team.is_some() && team_size <= p {
                continue;
            }
            team_size = p;
            team = Some(t);
        }
        let Some(team) = team else { continue };
        for &player in clan {
            if team_size < max_team_size {
                team_size += 1;
                result.push((player, Some(team.clone())));
            } else {
                result.push((player, None));
            }
        }
        team_player_count.insert(team.clone(), team_size);
    }

    // Friend edges are a soft preference.
    let present: HashSet<&str> = players
        .iter()
        .filter_map(|p| p.client_id.as_deref())
        .collect();
    let mut friend_graph: HashMap<&str, HashSet<&str>> = HashMap::new();
    for p in players {
        let Some(cid) = p.client_id.as_deref() else {
            continue;
        };
        for f in &p.friends {
            if !present.contains(f.as_str()) {
                continue;
            }
            friend_graph.entry(cid).or_default().insert(f.as_str());
            friend_graph.entry(f.as_str()).or_default().insert(cid);
        }
    }

    let mut team_by_client_id: HashMap<&str, Team> = HashMap::new();
    for (i, team) in &result {
        if let (Some(cid), Some(team)) = (players[*i].client_id.as_deref(), team) {
            team_by_client_id.insert(cid, team.clone());
        }
    }

    let mut place_player = |i: usize, result: &mut Vec<(usize, Option<Team>)>| {
        let p = &players[i];
        let my_friends = p.client_id.as_deref().and_then(|cid| friend_graph.get(cid));
        let mut best_team: Option<&Team> = None;
        let mut best_friend_count: i64 = -1;
        let mut best_size: f64 = if is_duos_trios_quads {
            -1.0
        } else {
            f64::INFINITY
        };
        for t in teams {
            let size = team_player_count.get(t).copied().unwrap_or(0);
            if size >= max_team_size {
                continue;
            }
            let mut friends_on_team = 0i64;
            if let Some(friends) = my_friends {
                for f in friends {
                    if team_by_client_id.get(f) == Some(t) {
                        friends_on_team += 1;
                    }
                }
            }
            let size_f = size as f64;
            if friends_on_team > best_friend_count
                || (friends_on_team == best_friend_count
                    && (if is_duos_trios_quads {
                        size_f > best_size
                    } else {
                        size_f < best_size
                    }))
            {
                best_friend_count = friends_on_team;
                best_size = size_f;
                best_team = Some(t);
            }
        }
        let Some(best) = best_team else {
            result.push((i, None));
            return;
        };
        *team_player_count.entry(best.clone()).or_insert(0) += 1;
        result.push((i, Some(best.clone())));
        if let Some(cid) = p.client_id.as_deref() {
            team_by_client_id.insert(cid, best.clone());
        }
    };

    let mut nation_players: Vec<usize> = non_clan_players
        .iter()
        .copied()
        .filter(|&i| players[i].player_type == PlayerType::Nation)
        .collect();
    if !nation_players.is_empty() {
        let mut random = PseudoRandom::new(simple_hash(&players[nation_players[0]].id) as f64);
        nation_players = random.shuffle_array(&nation_players);
    }
    let other_players: Vec<usize> = non_clan_players
        .iter()
        .copied()
        .filter(|&i| players[i].player_type != PlayerType::Nation)
        .collect();
    for i in other_players.into_iter().chain(nation_players) {
        place_player(i, &mut result);
    }
    result
}

/// `resolveTeamsList`.
pub fn resolve_teams_list(
    config: TeamCountConfig,
    total_players: usize,
    min_two_teams: bool,
) -> Vec<Team> {
    if config == TeamCountConfig::HumansVsNations {
        let _ = HUMANS_VS_NATIONS;
        return vec![colored("Humans"), colored("Nations")];
    }
    let num_teams: f64 = match config {
        TeamCountConfig::Count(n) => n as f64,
        other => {
            let divisor = match other {
                TeamCountConfig::Duos => 2.0,
                TeamCountConfig::Trios => 3.0,
                TeamCountConfig::Quads => 4.0,
                _ => unreachable!(),
            };
            let _ = (DUOS, TRIOS, QUADS);
            let count = (total_players as f64 / divisor).ceil();
            if min_two_teams {
                f64::max(2.0, count)
            } else {
                count
            }
        }
    };
    assert!(num_teams >= 2.0, "Too few teams: {num_teams}");
    let num_teams = num_teams as usize;
    if num_teams < 8 {
        let mut teams = vec![colored("Red"), colored("Blue")];
        if num_teams >= 3 {
            teams.push(colored("Yellow"));
        }
        if num_teams >= 4 {
            teams.push(colored("Green"));
        }
        if num_teams >= 5 {
            teams.push(colored("Purple"));
        }
        if num_teams >= 6 {
            teams.push(colored("Orange"));
        }
        if num_teams >= 7 {
            teams.push(colored("Teal"));
        }
        return teams;
    }
    (0..num_teams).map(|i| format!("Team {}", i + 1)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(id: &str, clan: Option<&str>) -> PlayerInfo {
        PlayerInfo::new(
            id.to_string(),
            PlayerType::Human,
            Some(format!("c{id}")),
            id.to_string(),
            false,
            clan.map(str::to_string),
            vec![],
            None,
            None,
        )
    }

    #[test]
    fn team_lists() {
        assert_eq!(
            resolve_teams_list(TeamCountConfig::Count(2), 10, true),
            ["Red", "Blue"]
        );
        assert_eq!(
            resolve_teams_list(TeamCountConfig::Duos, 7, true),
            ["Red", "Blue", "Yellow", "Green"]
        );
        assert_eq!(
            resolve_teams_list(TeamCountConfig::Count(8), 0, true)[7],
            "Team 8"
        );
        assert_eq!(
            resolve_teams_list(TeamCountConfig::HumansVsNations, 3, true),
            ["Humans", "Nations"]
        );
    }

    #[test]
    fn clans_stay_together_and_overflow_is_kicked() {
        let players = vec![
            info("a", Some("X")),
            info("b", Some("X")),
            info("c", Some("X")),
            info("d", None),
        ];
        let teams = resolve_teams_list(TeamCountConfig::Count(2), 4, true);
        let result = assign_teams(&players, &teams, false, Some(2));
        let clan: Vec<_> = result.iter().take(3).collect();
        assert_eq!(clan[0].1, clan[1].1);
        assert_eq!(clan[2].1, None);
        assert!(result[3].1.is_some());
    }
}
