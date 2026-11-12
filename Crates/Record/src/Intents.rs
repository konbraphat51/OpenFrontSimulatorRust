// SPDX-License-Identifier: AGPL-3.0-only
//! Port of the `Intent` union (src/core/Schemas.ts): every intent a turn can carry, tagged on `type`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Port of `Intent`; field names match the wire format exactly. Anything else lands in `Unknown`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Intent {
    /// `allianceExtension`.
    #[serde(rename = "allianceExtension")]
    AllianceExtension {
        /// Player id of the ally.
        recipient: String,
    },
    /// `attack`; `targetID` null attacks terra nullius, `troops` null uses the ratio.
    #[serde(rename = "attack")]
    Attack {
        /// Target player id.
        #[serde(rename = "targetID")]
        target_id: Option<String>,
        /// Troop count.
        troops: Option<f64>,
    },
    /// `spawn`.
    #[serde(rename = "spawn")]
    Spawn {
        /// Spawn tile.
        tile: u32,
    },
    /// `boat`.
    #[serde(rename = "boat")]
    Boat {
        /// Troop count.
        troops: f64,
        /// Destination tile.
        dst: u32,
    },
    /// `allianceRequest`.
    #[serde(rename = "allianceRequest")]
    AllianceRequest {
        /// Recipient id.
        recipient: String,
    },
    /// `allianceReject`.
    #[serde(rename = "allianceReject")]
    AllianceReject {
        /// Requestor id.
        requestor: String,
    },
    /// `breakAlliance`.
    #[serde(rename = "breakAlliance")]
    BreakAlliance {
        /// Ally id.
        recipient: String,
    },
    /// `targetPlayer`.
    #[serde(rename = "targetPlayer")]
    TargetPlayer {
        /// Target id.
        target: String,
    },
    /// `emoji`; `recipient` is a player id or "AllPlayers".
    #[serde(rename = "emoji")]
    Emoji {
        /// Recipient id or "AllPlayers".
        recipient: String,
        /// Index into the flattened emoji table.
        emoji: u32,
    },
    /// `embargo`.
    #[serde(rename = "embargo")]
    Embargo {
        /// Target id.
        #[serde(rename = "targetID")]
        target_id: String,
        /// "start" or "stop".
        action: String,
    },
    /// `embargo_all`.
    #[serde(rename = "embargo_all")]
    EmbargoAll {
        /// "start" or "stop".
        action: String,
    },
    /// `donate_gold`.
    #[serde(rename = "donate_gold")]
    DonateGold {
        /// Recipient id.
        recipient: String,
        /// Gold amount, null for the default share.
        gold: Option<f64>,
    },
    /// `donate_troops`.
    #[serde(rename = "donate_troops")]
    DonateTroops {
        /// Recipient id.
        recipient: String,
        /// Troop amount, null for the default share.
        troops: Option<f64>,
    },
    /// `build_unit`.
    #[serde(rename = "build_unit")]
    BuildUnit {
        /// `UnitType` wire name.
        unit: String,
        /// Build tile.
        tile: u32,
        /// Missile silo direction.
        #[serde(
            default,
            rename = "rocketDirectionUp",
            skip_serializing_if = "Option::is_none"
        )]
        rocket_direction_up: Option<bool>,
        /// Number of units.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<u32>,
    },
    /// `upgrade_structure`.
    #[serde(rename = "upgrade_structure")]
    UpgradeStructure {
        /// `UnitType` wire name.
        unit: String,
        /// Unit id.
        #[serde(rename = "unitId")]
        unit_id: u32,
        /// Number of upgrades.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        amount: Option<u32>,
    },
    /// `cancel_attack`.
    #[serde(rename = "cancel_attack")]
    CancelAttack {
        /// Attack id.
        #[serde(rename = "attackID")]
        attack_id: String,
    },
    /// `cancel_boat`.
    #[serde(rename = "cancel_boat")]
    CancelBoat {
        /// Boat unit id.
        #[serde(rename = "unitID")]
        unit_id: u32,
    },
    /// `move_warship`.
    #[serde(rename = "move_warship")]
    MoveWarship {
        /// Warship unit ids (builds before v0.31 send a single `unitId`).
        #[serde(rename = "unitIds", alias = "unitId", deserialize_with = "one_or_many")]
        unit_ids: Vec<i64>,
        /// Target tile.
        tile: u32,
    },
    /// `delete_unit`.
    #[serde(rename = "delete_unit")]
    DeleteUnit {
        /// Unit id.
        #[serde(rename = "unitId")]
        unit_id: u32,
    },
    /// `quick_chat`.
    #[serde(rename = "quick_chat")]
    QuickChat {
        /// Recipient id.
        recipient: String,
        /// "category.key".
        #[serde(rename = "quickChatKey")]
        quick_chat_key: String,
        /// Optional target id.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// `mark_disconnected` (server internal); the marked player is the stamped `clientID`.
    #[serde(rename = "mark_disconnected")]
    MarkDisconnected {
        /// New connection state.
        #[serde(rename = "isDisconnected")]
        is_disconnected: bool,
    },
    /// `kick_player`.
    #[serde(rename = "kick_player")]
    KickPlayer {
        /// Live client id.
        #[serde(
            default,
            rename = "targetClientID",
            skip_serializing_if = "Option::is_none"
        )]
        target_client_id: Option<String>,
        /// Account public id.
        #[serde(
            default,
            rename = "targetPublicID",
            skip_serializing_if = "Option::is_none"
        )]
        target_public_id: Option<String>,
    },
    /// `toggle_pause`.
    #[serde(rename = "toggle_pause")]
    TogglePause {
        /// New pause state (default false).
        #[serde(default)]
        paused: bool,
    },
    /// `update_game_config`; the partial config is kept raw.
    #[serde(rename = "update_game_config")]
    UpdateGameConfig {
        /// Partial `GameConfig`.
        config: Value,
    },
    /// `toggle_game_start_timer`.
    #[serde(rename = "toggle_game_start_timer")]
    ToggleGameStartTimer {},
    /// An intent this crate does not know (or that fails to match its variant), kept raw.
    #[serde(untagged)]
    Unknown(Map<String, Value>),
}

impl Intent {
    /// The wire `type` of the intent ("unknown" for `Unknown` without a type string).
    pub fn type_name(&self) -> &str {
        match self {
            Self::AllianceExtension { .. } => "allianceExtension",
            Self::Attack { .. } => "attack",
            Self::Spawn { .. } => "spawn",
            Self::Boat { .. } => "boat",
            Self::AllianceRequest { .. } => "allianceRequest",
            Self::AllianceReject { .. } => "allianceReject",
            Self::BreakAlliance { .. } => "breakAlliance",
            Self::TargetPlayer { .. } => "targetPlayer",
            Self::Emoji { .. } => "emoji",
            Self::Embargo { .. } => "embargo",
            Self::EmbargoAll { .. } => "embargo_all",
            Self::DonateGold { .. } => "donate_gold",
            Self::DonateTroops { .. } => "donate_troops",
            Self::BuildUnit { .. } => "build_unit",
            Self::UpgradeStructure { .. } => "upgrade_structure",
            Self::CancelAttack { .. } => "cancel_attack",
            Self::CancelBoat { .. } => "cancel_boat",
            Self::MoveWarship { .. } => "move_warship",
            Self::DeleteUnit { .. } => "delete_unit",
            Self::QuickChat { .. } => "quick_chat",
            Self::MarkDisconnected { .. } => "mark_disconnected",
            Self::KickPlayer { .. } => "kick_player",
            Self::TogglePause { .. } => "toggle_pause",
            Self::UpdateGameConfig { .. } => "update_game_config",
            Self::ToggleGameStartTimer {} => "toggle_game_start_timer",
            Self::Unknown(map) => map.get("type").and_then(Value::as_str).unwrap_or("unknown"),
        }
    }

    /// True for the `Unknown` fallback.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown(_))
    }
}

/// Port of `StampedIntent`: an intent with the server-stamped `clientID`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StampedIntent {
    /// Sender client id.
    #[serde(rename = "clientID")]
    pub client_id: String,
    /// The intent itself (flattened next to `clientID`).
    #[serde(flatten)]
    pub intent: Intent,
}

/// Accepts one id or a list of ids.
fn one_or_many<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<i64>, D::Error> {
    #[derive(serde::Deserialize)]
    #[serde(untagged)]
    enum OneOrMany {
        One(i64),
        Many(Vec<i64>),
    }
    Ok(match OneOrMany::deserialize(d)? {
        OneOrMany::One(id) => vec![id],
        OneOrMany::Many(ids) => ids,
    })
}
