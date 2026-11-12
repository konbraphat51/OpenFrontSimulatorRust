//! Port of `GameStartInfo` and `toWireGameStartInfo` (src/core/Util.ts, src/core/Schemas.ts).

use crate::Config::{GameConfig, GameType};
use crate::Records::{GameRecord, PlayerRecord, Tribe};
use serde::{Deserialize, Serialize};

/// Port of `PlayerSchema`: a player as the engine's `createGameRunner` consumes it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StartPlayer {
    /// Client id.
    #[serde(rename = "clientID")]
    pub client_id: String,
    /// Username.
    pub username: String,
    /// Clan tag (blanked by the wire conversion when disabled).
    #[serde(rename = "clanTag")]
    pub clan_tag: Option<String>,
    /// Lobby creator flag.
    #[serde(
        default,
        rename = "isLobbyCreator",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_lobby_creator: Option<bool>,
    /// Friend client ids (blanked by the wire conversion when names are anonymized).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friends: Option<Vec<String>>,
    /// Matchmade team slot.
    #[serde(default, rename = "teamIndex", skip_serializing_if = "Option::is_none")]
    pub team_index: Option<u32>,
}

impl From<&PlayerRecord> for StartPlayer {
    fn from(player: &PlayerRecord) -> Self {
        Self {
            client_id: player.client_id.clone(),
            username: player.username.clone(),
            clan_tag: player.clan_tag.clone(),
            is_lobby_creator: player.is_lobby_creator,
            friends: player.friends.clone(),
            team_index: player.team_index,
        }
    }
}

/// Port of `GameStartInfo`: everything `createGameRunner` reads (cosmetics do not reach the simulation).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameStartInfo {
    /// Game id (seeds the game's `PseudoRandom`).
    #[serde(rename = "gameID")]
    pub game_id: String,
    /// Lobby creation time (ms).
    #[serde(rename = "lobbyCreatedAt")]
    pub lobby_created_at: f64,
    /// Publicly listed lobby (unused by the simulation).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listed: Option<bool>,
    /// Game configuration.
    pub config: GameConfig,
    /// Players in `info.players` order (this order assigns small ids and `nextID()` draws).
    pub players: Vec<StartPlayer>,
    /// Purchased tribe names (`Executor` receives their names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tribes: Option<Vec<Tribe>>,
}

impl GameStartInfo {
    /// Builds the start info of a record exactly as the replay path does: `toWireGameStartInfo(record.info)`.
    ///
    /// Singleplayer records are used as recorded. Otherwise clan tags are blanked when `disableClanTags` or
    /// `anonymizeNames` is set, and friends are blanked when `anonymizeNames` is set.
    pub fn from_record(record: &GameRecord) -> Self {
        let info = &record.info;
        let mut start = Self {
            game_id: info.game_id.clone(),
            lobby_created_at: info.lobby_created_at,
            listed: info.listed,
            config: info.config.clone(),
            players: info.players.iter().map(StartPlayer::from).collect(),
            tribes: info.tribes.clone(),
        };
        start.apply_wire_blanking();
        start
    }

    /// Port of the blanking in `toWireGameStartInfo`.
    fn apply_wire_blanking(&mut self) {
        if self.config.game_type == GameType::Singleplayer {
            return;
        }
        let anonymize = self.config.anonymize_names.unwrap_or(false);
        let blank_clan_tags = self.config.disable_clan_tags.unwrap_or(false) || anonymize;
        let blank_friends = anonymize;
        for player in &mut self.players {
            if blank_clan_tags {
                player.clan_tag = None;
            }
            if blank_friends {
                player.friends = None;
            }
        }
    }
}
