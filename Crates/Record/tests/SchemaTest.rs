//! Unit tests of intent fallback, turn expansion and start-info blanking on synthetic data.

#![allow(non_snake_case)]

use ofsim_record::Config::GameType;
use ofsim_record::Intents::{Intent, StampedIntent};
use ofsim_record::Records::GameRecord;
use ofsim_record::StartInfo::GameStartInfo;
use serde_json::{Value, json};

fn sample_record(game_type: &str, anonymize: bool) -> Value {
    json!({
        "version": "v0.0.2",
        "gitCommit": "4e837520e886b255b1b020e9c89639dbb9d54038",
        "info": {
            "gameID": "abcd1234", "lobbyCreatedAt": 1, "start": 2, "end": 3, "duration": 1, "num_turns": 6,
            "lobbyFillTime": 0,
            "config": {
                "gameMap": "World", "difficulty": "Medium", "donateGold": false, "donateTroops": false,
                "gameType": game_type, "gameMode": "Free For All", "gameMapSize": "Normal", "nations": "default",
                "bots": 0, "infiniteGold": false, "infiniteTroops": false, "instantBuild": false,
                "randomSpawn": false, "anonymizeNames": anonymize, "futureOption": 7
            },
            "players": [{"clientID": "p1", "username": "x", "clanTag": "AB", "friends": ["p2"], "stats": {}}]
        },
        "turns": [
            {"turnNumber": 2, "intents": [{"type": "spawn", "clientID": "p1", "tile": 5}]},
            {"turnNumber": 3, "intents": [], "hash": 12.5},
            {"turnNumber": 3, "intents": []}
        ]
    })
}

#[test]
fn unknown_intents_are_kept_raw() {
    let value = json!({"type": "future_intent", "clientID": "p1", "x": 1});
    let stamped: StampedIntent = serde_json::from_value(value.clone()).unwrap();
    assert!(stamped.intent.is_unknown());
    assert_eq!(stamped.intent.type_name(), "future_intent");
    assert_eq!(serde_json::to_value(&stamped).unwrap(), value);
}

#[test]
fn known_intent_parses() {
    let stamped: StampedIntent = serde_json::from_value(
        json!({"type": "attack", "clientID": "p1", "targetID": null, "troops": 12.5}),
    )
    .unwrap();
    assert_eq!(
        stamped.intent,
        Intent::Attack {
            target_id: None,
            troops: Some(12.5)
        }
    );
}

#[test]
fn decompress_mirrors_upstream_loop() {
    let mut record: GameRecord = serde_json::from_value(sample_record("Public", false)).unwrap();
    record.decompress();
    let numbers: Vec<u32> = record.turns.iter().map(|t| t.turn_number).collect();
    // Gaps filled up to turn 2, duplicate number 3 kept as is, then padded to num_turns = 6.
    assert_eq!(numbers, vec![0, 1, 2, 3, 3, 5]);
}

#[test]
fn wire_start_info_blanks_like_upstream() {
    let public: GameRecord = serde_json::from_value(sample_record("Public", true)).unwrap();
    let start = GameStartInfo::from_record(&public);
    assert_eq!(start.players[0].clan_tag, None);
    assert_eq!(start.players[0].friends, None);
    assert_eq!(start.config.extra["futureOption"], json!(7));

    let single: GameRecord = serde_json::from_value(sample_record("Singleplayer", true)).unwrap();
    assert_eq!(single.info.config.game_type, GameType::Singleplayer);
    let start = GameStartInfo::from_record(&single);
    assert_eq!(start.players[0].clan_tag.as_deref(), Some("AB"));
    assert_eq!(
        start.players[0].friends.as_deref(),
        Some(&["p2".to_string()][..])
    );
}
