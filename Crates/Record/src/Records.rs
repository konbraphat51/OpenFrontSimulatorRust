//! Port of the record schemas (src/core/Schemas.ts): `GameRecord`, `GameEndInfo`, `PlayerRecord`, `Turn`.

use crate::Config::GameConfig;
use crate::Intents::StampedIntent;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Port of `Turn`: the intents of one tick and, every 10th turn, the state hash after it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Turn {
    /// Turn (tick) number.
    #[serde(rename = "turnNumber")]
    pub turn_number: u32,
    /// Stamped intents in execution order.
    pub intents: Vec<StampedIntent>,
    /// `GameImpl.hash()` at the end of the turn, if recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<f64>,
}

/// Port of `PlayerRecord` (a `Player` plus `persistentID` and `stats`); only simulation fields are typed.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PlayerRecord {
    /// Client id.
    #[serde(rename = "clientID")]
    pub client_id: String,
    /// Username (never printed by tooling).
    pub username: String,
    /// Clan tag; null when none.
    #[serde(rename = "clanTag")]
    pub clan_tag: Option<String>,
    /// Cosmetics, ignored by the simulation; kept raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cosmetics: Option<Value>,
    /// Lobby creator flag.
    #[serde(
        default,
        rename = "isLobbyCreator",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_lobby_creator: Option<bool>,
    /// Friend client ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub friends: Option<Vec<String>>,
    /// Matchmade team slot.
    #[serde(default, rename = "teamIndex", skip_serializing_if = "Option::is_none")]
    pub team_index: Option<u32>,
    /// Persistent id (PII, absent or null on API records).
    #[serde(
        default,
        rename = "persistentID",
        skip_serializing_if = "Option::is_none"
    )]
    pub persistent_id: Option<String>,
    /// Per-player stats, kept raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stats: Option<Value>,
    /// Unknown keys (for example `publicID`).
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Port of `TribeSchema` (loose): a purchased tribe name in use.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tribe {
    /// Tribe name.
    pub name: String,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Port of `GameEndInfoSchema`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameEndInfo {
    /// Game id.
    #[serde(rename = "gameID")]
    pub game_id: String,
    /// Lobby creation time (ms).
    #[serde(rename = "lobbyCreatedAt")]
    pub lobby_created_at: f64,
    /// Time the lobby became visible (ms).
    #[serde(default, rename = "visibleAt", skip_serializing_if = "Option::is_none")]
    pub visible_at: Option<f64>,
    /// Publicly listed lobby.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listed: Option<bool>,
    /// Game configuration.
    pub config: GameConfig,
    /// Players of the game.
    pub players: Vec<PlayerRecord>,
    /// Purchased tribe names.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tribes: Option<Vec<Tribe>>,
    /// Start time (ms).
    pub start: f64,
    /// End time (ms).
    pub end: f64,
    /// Duration in seconds.
    pub duration: f64,
    /// Number of turns the game ran.
    pub num_turns: u32,
    /// Winner tuple (`["player", id, ...]`, `["team", name, ...]`, `["nation", name, ...]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub winner: Option<Vec<String>>,
    /// Lobby fill time (ms).
    #[serde(rename = "lobbyFillTime")]
    pub lobby_fill_time: f64,
    /// Master-scheduled lobby slot.
    #[serde(
        default,
        rename = "publicGameType",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_game_type: Option<String>,
    /// Player reports, kept raw.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reports: Option<Value>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Port of `GameRecordSchema`: the response of `GET /public/game/<id>`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GameRecord {
    /// Info block.
    pub info: GameEndInfo,
    /// Record version ("v0.0.2").
    pub version: String,
    /// Upstream git commit of the server that played the game, or "DEV".
    #[serde(rename = "gitCommit")]
    pub git_commit: String,
    /// Server subdomain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subdomain: Option<String>,
    /// Server domain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domain: Option<String>,
    /// Registered site.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub site: Option<String>,
    /// Turns; sparse until `decompress` runs.
    pub turns: Vec<Turn>,
    /// Unknown keys.
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}
