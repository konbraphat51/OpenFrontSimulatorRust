// SPDX-License-Identifier: AGPL-3.0-only
//! Unit tests of the rule sets: edge files, build table and class chains.

#[test]
fn every_edge_file_names_known_changes() {
    for (class, _) in EDGE_FILES {
        assert!(edge_of(class).is_some(), "{class}");
    }
}

use super::*;

#[test]
fn baseline_and_unknown() {
    let r = Rules::for_commit("4e837520e886").unwrap();
    assert!(r.has(Change::DetMath5191) && r.has(Change::Sfc32PseudoRandom4233));
    assert_eq!(r.class_id(), "C000");
    assert!(Rules::for_commit("4e837520e886deadbeef").is_ok());
    for commit in BASELINE_CLASS_COMMITS {
        assert_eq!(Rules::for_commit(commit).unwrap().class_id(), "C000");
    }
    assert!(matches!(
        Rules::for_commit("abc"),
        Err(RulesError::UnknownBuild { .. })
    ));
    assert!(matches!(
        Rules::for_commit("4e837520e885"),
        Err(RulesError::UnknownBuild { .. })
    ));
}

#[test]
fn every_build_resolves_to_its_class() {
    let table = class_table();
    assert_eq!(table.builds.len(), 159);
    for (build, class) in known_builds() {
        assert!(table.parents.contains_key(class), "{build} -> {class}");
        match Rules::for_commit(build) {
            Ok(r) => assert_eq!(r.class_id(), class),
            Err(RulesError::NotPorted { class: c, .. }) => assert_eq!(c, class),
            Err(e) => panic!("{build}: {e}"),
        }
    }
    assert!(Rules::for_commit("115da0327634").is_err());
}

#[test]
fn unported_class_errors_with_class() {
    let e = Rules::for_commit("115da0327634").unwrap_err();
    assert_eq!(
        e.to_string(),
        "build 115da0327634 (class C080) not ported yet"
    );
}

#[test]
fn builds_doc_lists_resolve() {
    // Every 12-hex commit mentioned in Docs/Builds.md is a known build.
    let docs = include_str!("../../../../../Docs/Builds.md");
    let table = class_table();
    let mut seen = 0;
    for word in docs.split(|c: char| !c.is_ascii_hexdigit()) {
        if word.len() == 12 && table.builds.contains_key(word) {
            seen += 1;
        }
    }
    assert!(seen >= 100, "only {seen} builds found in Builds.md");
}

#[test]
fn era_d_classes_are_ported_with_the_expected_changes() {
    for n in 0..=12 {
        assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
    }
    assert!(is_ported("C040"));
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    assert!(rules("3f5633b92c95").has(Change::AttackRetreatNoTargets5823));
    assert!(!rules("824d1412be58").has(Change::AttackRetreatNoTargets5823));
    let c006 = rules("7c27263390d8");
    assert!(!c006.has(Change::ChannelIslandsTribes5564));
    assert!(c006.has(Change::InternalBorderAnnexFix5545));
    let c007 = rules("82e5fce9502b");
    assert!(!c007.has(Change::InternalBorderAnnexFix5545));
    assert!(!c007.has(Change::SaturationMidpoints5587));
    assert!(c007.has(Change::TrainSaturation500Pr5462));
    assert!(!rules("ece79bfd3348").has(Change::NukeFriendlyKillNoCredit5525));
    assert!(!rules("222e4078982e").has(Change::TrainSaturation500Pr5462));
    let c011 = rules("1e973bb534b8");
    assert!(!c011.has(Change::TeamCountMinTwo5436) && !c011.has(Change::AttackSpeedDivisor5425));
    let c012 = rules("e910d727d2dd");
    assert!(c012.has(Change::NewMapThemes5547) && c012.has(Change::ChannelIslandsTribes5564));
    assert!(!rules("4e837520e886").has(Change::NewMapThemes5547));
}

#[test]
fn era_f_classes_are_ported_without_detmath() {
    for n in 13..=22 {
        assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
    }
    assert!(!is_ported("C080"));
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    let c013 = rules("8b45be57542f");
    assert!(!c013.has(Change::DetMath5191) && c013.has(Change::Sfc32PseudoRandom4233));
    assert!(!c013.has(Change::AttackLogicRework5173) && !c013.has(Change::SpawnPhase20s5347));
    assert!(c013.has(Change::StationClusterSame5096) && c013.has(Change::Overtime5062));
    assert!(!rules("0cb90ccb7478").has(Change::StationClusterSame5096));
    assert!(!rules("2d5baafdd0cc").has(Change::Overtime5062));
    assert!(!rules("e9e10703e818").has(Change::BulkPurchase4640));
    assert!(rules("20c813f06a40").has(Change::BulkPurchase4640));
}

#[test]
fn v35_classes_are_ported_up_to_c087() {
    for n in 81..=87 {
        assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
    }
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    let c081 = rules("dfd420fd7836");
    assert!(c081.has(Change::NationsLikeHumans5899) && c081.has(Change::SeveredLandAnnex5955));
    assert!(!c081.has(Change::NationAiSplit6087));
    let c083 = rules("a2d20f8e5f32");
    assert!(c083.has(Change::NationAiSplit6087) && c083.has(Change::NationsLikeHumans5899));
    assert!(!rules("4e837520e886").has(Change::NationsLikeHumans5899));
    assert!(rules("7ee96860c717").has(Change::TutorialSilo6242));
    assert!(!rules("c144d134d150").has(Change::TutorialSilo6242));
    assert!(rules("80bff198b68f").has(Change::TeamFriendGroups6259));
}

#[test]
fn v36_classes_add_their_changes() {
    assert!(is_ported("C088") && is_ported("C089"));
    let c087 = Rules::for_commit("09aa423bceb9").unwrap();
    assert!(!c087.has(Change::PlayerClusterRewrite36) && !c087.has(Change::RailNetworkHash36));
    let c088 = Rules::for_commit("0211f17b71ef").unwrap();
    assert!(c088.has(Change::PlayerClusterRewrite36) && c088.has(Change::MirvTargetGrid36));
    assert!(!c088.has(Change::TribeMapData361));
    let c089 = Rules::for_commit("bc3e3334c573").unwrap();
    assert!(c089.has(Change::PlayerClusterRewrite36) && c089.has(Change::TribeMapData361));
}

#[test]
fn era_s_head_classes_switch_the_generator_and_drop_meta_changes() {
    for c in ["C036", "C037", "C038", "C039"] {
        assert!(is_ported(c), "{c}");
    }
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    let c036 = rules("26d8a314aec4");
    assert!(!c036.has(Change::Sfc32PseudoRandom4233) && !c036.has(Change::AiTroopCap4239));
    assert!(!c036.has(Change::TradeShipSigmoid400Meta) && c036.has(Change::SpawnPhaseTimer3199));
    let c039 = rules("f17ca9bd6551");
    assert!(c039.has(Change::TradeShipSigmoid400Meta) && !c039.has(Change::Sfc32PseudoRandom4233));
    let c037 = rules("aeb8d60224e3");
    assert!(!c037.has(Change::SpawnPhaseTimer3199) && !c037.has(Change::AttackForEachNeighbor3820));
    assert!(
        !c037.has(Change::TradeShipSigmoid400Meta) && !c037.has(Change::AbstractAStarMinHeap4149)
    );
    assert!(!rules("5d6c48ebf23f").has(Change::SpawnPhaseTimer3199));
}

#[test]
fn era_f_tail_classes_remove_their_changes() {
    for n in 23..=35 {
        assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
    }
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    let c022 = rules("7a7ca5be8ff8");
    assert!(c022.has(Change::ImpassableTerrain4340) && !c022.has(Change::DoomsdayRotLadder4893));
    let c023 = rules("dd1277e245b5");
    assert!(!c023.has(Change::ImpassableTerrain4340) && !c023.has(Change::NeighborOrderNswe4495));
    assert!(!c023.has(Change::SamTargetingFixes4770) && !c023.has(Change::WarshipVeterancy4433));
    assert!(c023.has(Change::SpawnTeleportGuard4657));
    assert!(!rules("b716adb7e2f1").has(Change::SpawnTeleportGuard4657));
    assert!(!rules("580460c9692a").has(Change::DoomsdayClock4469));
    assert!(rules("53e1a5b03e35").has(Change::DoomsdayClock4469));
    let c029 = rules("0c4c7d7993c9");
    assert!(
        !c029.has(Change::ClusterOffsetFromId4426) && !c029.has(Change::NearbySkipsFallout4422)
    );
    assert!(rules("d7b781f155dc").has(Change::RailroadMaxSizeLimit4337));
    assert!(!rules("d7b781f155dc").has(Change::RailroadMaxSizeDiagonal4406));
    assert!(!rules("bb2f2f01ee88").has(Change::RailroadMaxSizeLimit4337));
    assert!(rules("661d96ba280f").has(Change::RailroadMaxSizeLimit4337));
    assert!(!rules("305534cc6579").has(Change::UnitsConstructedOnce4320));
    assert!(!rules("5be72db060fc").has(Change::AiBoatTroopsUnified4297));
    assert!(is_ported("C040"));
}

#[test]
fn v30_classes_drop_v31_changes_and_their_own_betas() {
    for n in 40..=48 {
        assert!(is_ported(&format!("C{n:03}")), "C{n:03}");
    }
    let rules = |commit: &str| Rules::for_commit(commit).unwrap();
    let c040 = rules("d809c25d1cdd");
    assert!(!c040.has(Change::NextIntFloorsArgs31) && !c040.has(Change::NationStructureRework31));
    assert!(!c040.has(Change::WarshipDocking31) && !c040.has(Change::NukeSpeed8V31));
    assert!(c040.has(Change::SamCooldown120V30) && c040.has(Change::TrainTradeRetune30));
    assert!(
        !c040.has(Change::BouncingNukes30) && rules("f2507c38979c").has(Change::BouncingNukes30)
    );
    assert!(rules("3e7088cb2a2b").has(Change::ClusterLocalEnemyBox3455));
    assert!(!rules("dd2c239aa166").has(Change::ClusterLocalEnemyBox3455));
    assert!(!rules("5fb7f75f3d5b").has(Change::NoPublicDonation3437));
    assert!(!rules("ebe7f76cd312").has(Change::TrainTradeRetune30));
    assert!(!rules("741a38c62b29").has(Change::AftergameTeammateNukes3410));
    let c047 = rules("df5fad1a1f6d");
    assert!(!c047.has(Change::SamCooldown120V30) && c047.has(Change::TrainSpawnRate25Beta2));
    let c048 = rules("339ace0bd677");
    assert!(
        !c048.has(Change::TrainSpawnRate25Beta2) && !c048.has(Change::NationCreationConfigurable30)
    );
}
