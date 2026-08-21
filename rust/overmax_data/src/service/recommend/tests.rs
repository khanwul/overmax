use std::sync::Arc;
use std::time::Duration;

use overmax_core::{Difficulty, Mode};

use crate::community::client::VArchiveDB;
use crate::service::recommend::scoring::{
    calculate_performance_rating, derive_recommend_reason, derive_recommended_level,
    derive_skill_profile, derive_top50_base_floor, retry_priority, session_flow_score,
    top50_boundary_score, GaussianSkill, SessionPlayInfo, SessionTrend, SessionTrendState,
    SkillProfile,
};
use crate::service::recommend::{
    CompositeRecommender, LocalFloorRecommender, ProviderCacheReader, RecommendContext,
    RecommendReasonKind, RecommendStrategy, SourceStatus, VaryDim,
};
use crate::service::record_manager::RecordManager;

#[test]
fn test_provider_cache_reader_fallback_when_file_missing() {
    let temp_dir = std::env::temp_dir().join("overmax_test_cache_reader_missing");
    let reader = ProviderCacheReader::new(
        "test_provider",
        "Test Provider",
        &temp_dir,
        vec![VaryDim::Mode],
        Duration::from_secs(3600),
    );

    let ctx = RecommendContext {
        song_id: 1,
        button_mode: Mode::B4,
        difficulty: Difficulty::NM,
        floor_range: 0.0,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };

    let bundle = reader.recommend(&ctx);
    assert_eq!(bundle.status, SourceStatus::Error);
    assert!(bundle.entries.is_empty());
}

#[test]
fn test_provider_cache_reader_valid_json() {
    let temp_dir = std::env::temp_dir().join("overmax_test_cache_reader_valid");
    let _ = std::fs::create_dir_all(&temp_dir);
    let cache_file = temp_dir.join("B4.json");

    let json_content = r#"{
        "protocol": "overmax-recommend/1",
        "entries": [
            { "song_id": 101, "mode": "4B", "diff": "SC", "score": 98.5, "reason": "14.2" },
            { "song_id": 102, "mode": "4B", "diff": "HD", "score": 95.0 }
        ]
    }"#;
    std::fs::write(&cache_file, json_content).unwrap();

    let reader = ProviderCacheReader::new(
        "test_provider",
        "Test Provider",
        &temp_dir,
        vec![VaryDim::Mode],
        Duration::from_secs(3600),
    );

    let ctx = RecommendContext {
        song_id: 1,
        button_mode: Mode::B4,
        difficulty: Difficulty::SC,
        floor_range: 0.0,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };

    let bundle = reader.recommend(&ctx);
    assert_eq!(bundle.status, SourceStatus::Ok);
    assert_eq!(bundle.entries.len(), 2);
    assert_eq!(bundle.entries[0].song_id, 101);
    assert_eq!(bundle.entries[0].floor_name.as_deref(), Some("14.2"));
    assert_eq!(bundle.entries[0].score, Some(98.5));
    assert_eq!(bundle.entries[1].song_id, 102);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_composite_recommender_with_varchive_db_preserves_provider() {
    let temp_dir = std::env::temp_dir().join("overmax_test_composite_preserve");
    let _ = std::fs::create_dir_all(&temp_dir);
    let cache_file = temp_dir.join("B4.json");

    let json_content = r#"{
        "protocol": "overmax-recommend/1",
        "entries": [
            { "song_id": 101, "mode": "4B", "diff": "SC", "score": 98.5 }
        ]
    }"#;
    std::fs::write(&cache_file, json_content).unwrap();

    let reader = ProviderCacheReader::new(
        "test_provider",
        "Test Provider",
        &temp_dir,
        vec![VaryDim::Mode],
        Duration::from_secs(3600),
    );

    let mut vdb = VArchiveDB::new();
    let song_json = serde_json::json!({
        "name": "Song 101",
        "title": "101",
        "composer": "Artist",
        "patterns": {
            "4B": {
                "SC": {
                    "level": 14,
                    "floorName": "14.2"
                }
            }
        }
    });
    vdb.songs = vec![serde_json::from_value(song_json).unwrap()];

    let db_path = temp_dir.join("record.db");
    let db = crate::store::record_db::RecordDB::new(&db_path, None);
    let rdb = Arc::new(RecordManager::new(Arc::new(db)));

    let composite = CompositeRecommender::new(Arc::new(vdb), rdb).with_provider(reader);

    let mut vdb2 = VArchiveDB::new();
    let song_json2 = serde_json::json!({
        "name": "Song 101 Updated",
        "title": "101",
        "composer": "Artist",
        "patterns": {
            "4B": {
                "SC": {
                    "level": 14,
                    "floorName": "14.2"
                }
            }
        }
    });
    vdb2.songs = vec![serde_json::from_value(song_json2).unwrap()];

    let updated_composite = composite.with_varchive_db(Arc::new(vdb2));

    let ctx = RecommendContext {
        song_id: 101,
        button_mode: Mode::B4,
        difficulty: Difficulty::SC,
        floor_range: 0.0,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };

    let panel = updated_composite.recommend_panel(&ctx);
    assert_eq!(panel.bundles.len(), 2);
    assert_eq!(panel.bundles[0].source_id, "test_provider");
    assert_eq!(panel.bundles[0].entries[0].song_name, "Song 101 Updated");
    assert_eq!(panel.bundles[1].source_id, "local_floor");

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_smart_recommend_false_sorts_by_classic_rate_and_omits_reason() {
    let mut vdb = VArchiveDB::new();
    let song1 = serde_json::json!({
        "name": "Song 1",
        "title": "1",
        "composer": "Artist A",
        "dlcCode": "RV",
        "patterns": {
            "4B": {
                "SC": {
                    "level": 13,
                    "floorName": "13.0"
                }
            }
        }
    });
    let song2 = serde_json::json!({
        "name": "Song 2",
        "title": "2",
        "composer": "Artist B",
        "dlcCode": "RV",
        "patterns": {
            "4B": {
                "SC": {
                    "level": 13,
                    "floorName": "13.0"
                }
            }
        }
    });
    let song3 = serde_json::json!({
        "name": "Song 3",
        "title": "3",
        "composer": "Artist C",
        "dlcCode": "RV",
        "patterns": {
            "4B": {
                "SC": {
                    "level": 13,
                    "floorName": "13.0"
                }
            }
        }
    });
    vdb.songs = vec![
        serde_json::from_value(song1).unwrap(),
        serde_json::from_value(song2).unwrap(),
        serde_json::from_value(song3).unwrap(),
    ];

    let temp_dir = std::env::temp_dir().join("overmax_test_classic_sort");
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("record.db");
    let mut db = crate::store::record_db::RecordDB::new(&db_path, None);
    assert!(db.initialize());
    db.upsert(2, Mode::B4, Difficulty::SC, 95.0, false, false);
    db.upsert(3, Mode::B4, Difficulty::SC, 99.0, false, false);

    let rdb = Arc::new(RecordManager::new(Arc::new(db)));
    let recommender = LocalFloorRecommender::new(Arc::new(vdb), rdb);

    let ctx = RecommendContext {
        song_id: 1,
        button_mode: Mode::B4,
        difficulty: Difficulty::SC,
        floor_range: 0.5,
        max_results: 10,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Classic,
    };

    let bundle = recommender.recommend(&ctx);
    assert_eq!(bundle.entries.len(), 2);
    assert_eq!(bundle.entries[0].song_id, 2);
    assert_eq!(bundle.entries[0].rate, Some(95.0));
    assert_eq!(bundle.entries[0].reason, None);

    assert_eq!(bundle.entries[1].song_id, 3);
    assert_eq!(bundle.entries[1].rate, Some(99.0));
    assert_eq!(bundle.entries[1].reason, None);

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_derive_recommended_level() {
    let now = 100000;
    let profile_15 = SkillProfile {
        sc: GaussianSkill::new(14.8, 1.0, 50),
        pad: GaussianSkill::new(15.0, 1.0, 0),
    };
    let empty_profile = SkillProfile {
        sc: GaussianSkill::new(0.0, 1.5, 0),
        pad: GaussianSkill::new(0.0, 2.0, 0),
    };

    // 1. Top 50 기반 초기 권장 레벨 (세션 0판): SC 14.8 -> SC 15
    assert_eq!(
        derive_recommended_level(None, &[], Difficulty::SC, &profile_15),
        Some("SC 15".to_string())
    );

    // 2. [사용자 제보 버그 검증] Top 50 SC 15 유저가 세션 첫 판으로 손풀기 SC 5.0을 플레이했을 때
    //    권장 레벨이 SC 5로 급락하지 않고 Top 50 앵커(SC 15)로 완벽 방어됨을 검증!
    let warmup_single_play = vec![SessionPlayInfo {
        rating: 66.0, // 5.0층 99.8%
        floor: 5.0,
        rate: 99.8,
        diff: Difficulty::SC,
        is_max_combo: true,
        updated_at: now - 100,
    }];
    let warmup_single_state = SessionTrendState {
        trend: SessionTrend::Steady,
        avg_rating: 66.0,
        last_floor: 5.0,
        last_diff: Difficulty::SC,
    };
    let rec_level_warmup_guard = derive_recommended_level(
        Some(&warmup_single_state),
        &warmup_single_play,
        Difficulty::SC,
        &profile_15,
    );
    assert_eq!(
        rec_level_warmup_guard,
        Some("SC 15".to_string()),
        "Single warmup play (SC 5) must NOT drag recommended level down from Top 50 anchor (SC 15)"
    );

    // 3. 실전곡 1판(SC 14.0) 추가 시 스근한 블렌딩 (75% Top 50 + 25% Session -> 14.6 -> SC 15)
    let mixed_plays = vec![
        SessionPlayInfo {
            rating: 180.0,
            floor: 14.0,
            rate: 98.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 50,
        },
        SessionPlayInfo {
            rating: 66.0,
            floor: 5.0,
            rate: 99.8,
            diff: Difficulty::SC,
            is_max_combo: true,
            updated_at: now - 300,
        },
    ];
    let mixed_state = SessionTrendState {
        trend: SessionTrend::Steady,
        avg_rating: 180.0,
        last_floor: 14.0,
        last_diff: Difficulty::SC,
    };
    assert_eq!(
        derive_recommended_level(
            Some(&mixed_state),
            &mixed_plays,
            Difficulty::SC,
            &profile_15
        ),
        Some("SC 15".to_string())
    );

    // 4. 주력 플레이 4판 누적 시 세션 데이터 100% 반영 + Climbing 모멘텀 (+0.3)
    let session_4_plays = vec![
        SessionPlayInfo {
            rating: 180.0,
            floor: 13.8,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 50,
        },
        SessionPlayInfo {
            rating: 180.0,
            floor: 13.8,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 150,
        },
        SessionPlayInfo {
            rating: 180.0,
            floor: 13.8,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 250,
        },
        SessionPlayInfo {
            rating: 180.0,
            floor: 13.8,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 350,
        },
    ];
    let climbing_state = SessionTrendState {
        trend: SessionTrend::Climbing,
        avg_rating: 180.0,
        last_floor: 13.8,
        last_diff: Difficulty::SC,
    };
    assert_eq!(
        derive_recommended_level(
            Some(&climbing_state),
            &session_4_plays,
            Difficulty::SC,
            &profile_15
        ),
        Some("SC 14".to_string()) // 13.8 + 0.3 = 14.1 -> SC 14
    );

    // 5. Top 50 기록이 없는 경우
    assert_eq!(
        derive_recommended_level(None, &[], Difficulty::SC, &empty_profile),
        None
    );

    let normal_play = vec![SessionPlayInfo {
        rating: 120.0,
        floor: 11.0,
        rate: 98.0,
        diff: Difficulty::MX,
        is_max_combo: false,
        updated_at: now - 100,
    }];
    assert_eq!(
        derive_recommended_level(None, &normal_play, Difficulty::MX, &empty_profile),
        Some("11".to_string())
    );
}

#[test]
fn test_derive_top50_base_floor() {
    let mut rank_map = std::collections::HashMap::new();
    // SC 패턴들: 101 -> 15.0, 102 -> 14.5, 103 -> 14.0
    rank_map.insert((101, Mode::B4, Difficulty::SC), 1);
    rank_map.insert((102, Mode::B4, Difficulty::SC), 2);
    rank_map.insert((103, Mode::B4, Difficulty::SC), 3);
    // 일반 패턴들: 201 -> 12.0
    rank_map.insert((201, Mode::B4, Difficulty::MX), 4);

    let summary = crate::store::record_db::VArchiveTop50Summary {
        cutoff_rating: 150.0,
        rank_map: rank_map.clone(),
        total_recorded_count: 4,
    };

    let find_floor = |sid: i32, _m: Mode, _d: Difficulty| match sid {
        101 => 15.0,
        102 => 14.5,
        103 => 14.0,
        201 => 12.0,
        _ => 0.0,
    };

    // SC 계열: [14.0, 14.5, 15.0] (홀수 개수) -> 중앙값 14.5
    let sc_floor = derive_top50_base_floor(&summary, &find_floor, Mode::B4, true);
    assert!(sc_floor.is_some());
    assert!((sc_floor.unwrap() - 14.5).abs() < 1e-6);

    // 짝수 개수 SC 케이스: [12.0, 14.0, 14.5, 15.0] -> 중앙값 (14.0 + 14.5) / 2 = 14.25
    rank_map.insert((104, Mode::B4, Difficulty::SC), 5);
    let summary_even = crate::store::record_db::VArchiveTop50Summary {
        cutoff_rating: 140.0,
        rank_map: rank_map.clone(),
        total_recorded_count: 5,
    };
    let find_floor_even = |sid: i32, m: Mode, d: Difficulty| match sid {
        104 => 12.0,
        _ => find_floor(sid, m, d),
    };
    let sc_floor_even = derive_top50_base_floor(&summary_even, &find_floor_even, Mode::B4, true);
    assert!(sc_floor_even.is_some());
    assert!((sc_floor_even.unwrap() - 14.25).abs() < 1e-6);

    // 일반 계열: [12.0] -> 중앙값 12.0
    let mx_floor = derive_top50_base_floor(&summary, &find_floor, Mode::B4, false);
    assert!(mx_floor.is_some());
    assert!((mx_floor.unwrap() - 12.0).abs() < 1e-6);

    // 없는 모드
    let b5_floor = derive_top50_base_floor(&summary, &find_floor, Mode::B5, true);
    assert!(b5_floor.is_none());
}

#[test]
fn retry_priority_weights_gap_by_recency_and_grace_period() {
    let now = 1_000_000i64;

    assert_eq!(retry_priority(99.5, None, now), 0.0);
    assert!((retry_priority(95.0, None, now) - 4.5).abs() < 1e-6);
    assert!((retry_priority(91.0, None, now) - 8.5).abs() < 1e-6);

    let played_1h_ago = now - 3600;
    assert_eq!(retry_priority(91.0, Some(played_1h_ago), now), 0.0);

    let played_6h_ago = now - 6 * 3600;
    assert_eq!(retry_priority(91.0, Some(played_6h_ago), now), 0.0);

    let played_12h_ago = now - 12 * 3600;
    assert_eq!(retry_priority(91.0, Some(played_12h_ago), now), 0.0);

    let played_7d_ago = now - (7 * 86400 + 12 * 3600);
    let p_7d = retry_priority(91.0, Some(played_7d_ago), now);
    assert!((p_7d - (8.5 * 0.5)).abs() < 0.05);

    let played_15d_ago = now - 15 * 86400;
    let p_15d = retry_priority(91.0, Some(played_15d_ago), now);
    assert!((p_15d - 8.5).abs() < 1e-6);

    let played_60d_ago = now - 60 * 86400;
    let p_60d = retry_priority(91.0, Some(played_60d_ago), now);
    assert!((p_60d - 8.5).abs() < 1e-6);
}

#[test]
fn test_top50_boundary_score_cases() {
    let cutoff = 175.0;

    assert_eq!(top50_boundary_score(Some(174.5), None, cutoff, 40), 0.0);
    assert_eq!(top50_boundary_score(Some(174.5), None, 0.0, 50), 0.0);

    assert_eq!(top50_boundary_score(Some(180.0), Some(41), cutoff, 50), 3.0);
    assert_eq!(top50_boundary_score(Some(175.0), Some(50), cutoff, 50), 5.0);
    assert_eq!(top50_boundary_score(Some(190.0), Some(10), cutoff, 50), 0.0);

    let near_cutoff = top50_boundary_score(Some(174.8), None, cutoff, 50);
    assert!(
        near_cutoff > 5.0 && near_cutoff <= 6.0,
        "delta 0.2 should be near max bonus, got {}",
        near_cutoff
    );

    let mid_cutoff = top50_boundary_score(Some(174.0), None, cutoff, 50);
    assert!(
        (mid_cutoff - 3.0).abs() < 0.1,
        "delta 1.0 should be half bonus (3.0), got {}",
        mid_cutoff
    );

    assert_eq!(top50_boundary_score(Some(172.5), None, cutoff, 50), 0.0);
    assert_eq!(top50_boundary_score(Some(176.0), None, cutoff, 50), 0.0);
}

#[test]
fn test_session_flow_score_and_timeout() {
    let now = 1_000_000i64;

    let steady_state = SessionTrendState {
        trend: SessionTrend::Steady,
        avg_rating: 160.0,
        last_floor: 13.0,
        last_diff: Difficulty::SC,
    };
    let climbing_state = SessionTrendState {
        trend: SessionTrend::Climbing,
        avg_rating: 160.0,
        last_floor: 13.0,
        last_diff: Difficulty::SC,
    };
    let recovery_state = SessionTrendState {
        trend: SessionTrend::Recovery,
        avg_rating: 160.0,
        last_floor: 13.0,
        last_diff: Difficulty::SC,
    };

    assert_eq!(session_flow_score(13.0, None, 13.0, None), 0.0);

    let s_steady_same = session_flow_score(13.0, None, 13.0, Some(&steady_state));
    assert!((s_steady_same - 2.0).abs() < 1e-6);

    let s_climb_up = session_flow_score(13.25, None, 13.0, Some(&climbing_state));
    assert!((s_climb_up - 4.0).abs() < 1e-6);

    let s_climb_down = session_flow_score(12.7, None, 13.0, Some(&climbing_state));
    assert!((s_climb_down - 0.0).abs() < 1e-6);

    let s_rec_down = session_flow_score(12.7, None, 13.0, Some(&recovery_state));
    assert!((s_rec_down - 4.0).abs() < 1e-6);

    let expired_plays = vec![SessionPlayInfo {
        rating: 160.0,
        floor: 13.0,
        rate: 98.0,
        diff: Difficulty::SC,
        is_max_combo: false,
        updated_at: now - 5 * 3600,
    }];
    assert_eq!(SessionTrend::analyze_session(&expired_plays, now), None);

    let active_plays = vec![SessionPlayInfo {
        rating: 160.0,
        floor: 13.0,
        rate: 98.0,
        diff: Difficulty::SC,
        is_max_combo: false,
        updated_at: now - 2 * 3600,
    }];
    assert!(SessionTrend::analyze_session(&active_plays, now).is_some());
}

#[test]
fn test_recommend_reason_multi_signal_convergence() {
    let skill = GaussianSkill::new(12.0, 1.0, 50);

    let r_attack = derive_recommend_reason(
        1.0,
        5.5,
        2.0,
        None,
        Some(SessionTrend::Climbing),
        12.5,
        Some(&skill),
    );
    assert_eq!(
        r_attack.map(|r| r.kind),
        Some(RecommendReasonKind::Top50Attack)
    );

    let r_defend = derive_recommend_reason(
        2.0,
        4.5,
        3.0,
        Some(45),
        Some(SessionTrend::Climbing),
        12.5,
        Some(&skill),
    );
    assert_eq!(
        r_defend.map(|r| r.kind),
        Some(RecommendReasonKind::Top50Defend)
    );

    let r_retry = derive_recommend_reason(
        8.5,
        0.0,
        2.0,
        None,
        Some(SessionTrend::Steady),
        12.0,
        Some(&skill),
    );
    assert_eq!(r_retry.map(|r| r.kind), Some(RecommendReasonKind::Retry));

    let r_climb = derive_recommend_reason(
        0.5,
        0.0,
        4.0,
        None,
        Some(SessionTrend::Climbing),
        12.5,
        Some(&skill),
    );
    assert_eq!(r_climb.map(|r| r.kind), Some(RecommendReasonKind::Climbing));

    // Recovery zone (10.0 <= 12.0 - 0.8*1.0 = 11.2)
    let r_recovery = derive_recommend_reason(
        0.5,
        0.0,
        4.0,
        None,
        Some(SessionTrend::Recovery),
        10.0,
        Some(&skill),
    );
    assert_eq!(
        r_recovery.map(|r| r.kind),
        Some(RecommendReasonKind::Recovery)
    );

    let r_none = derive_recommend_reason(
        0.2,
        0.0,
        0.5,
        None,
        Some(SessionTrend::Steady),
        12.0,
        Some(&skill),
    );
    assert_eq!(r_none, None);
}

#[test]
fn test_derive_recommend_reason() {
    let skill = GaussianSkill::new(12.0, 1.0, 50);

    let top_attack_reason = derive_recommend_reason(0.0, 5.0, 0.0, None, None, 12.0, Some(&skill));
    assert_eq!(
        top_attack_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Top50Attack)
    );

    let top_defend_reason =
        derive_recommend_reason(0.0, 4.0, 0.0, Some(45), None, 12.0, Some(&skill));
    assert_eq!(
        top_defend_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Top50Defend)
    );

    let climbing_reason = derive_recommend_reason(
        0.0,
        0.0,
        3.5,
        None,
        Some(SessionTrend::Climbing),
        12.5,
        Some(&skill),
    );
    assert_eq!(
        climbing_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Climbing)
    );

    // Recovery on low floor (10.0) -> Recovery reason
    let recovery_reason = derive_recommend_reason(
        0.0,
        0.0,
        3.5,
        None,
        Some(SessionTrend::Recovery),
        10.0,
        Some(&skill),
    );
    assert_eq!(
        recovery_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Recovery)
    );

    let retry_reason = derive_recommend_reason(5.0, 0.0, 0.0, None, None, 12.0, Some(&skill));
    assert_eq!(
        retry_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Retry)
    );

    let low_score_reason = derive_recommend_reason(1.0, 1.2, 0.5, None, None, 12.0, Some(&skill));
    assert_eq!(low_score_reason, None);
}

#[test]
fn test_recovery_reason_gating_blocks_high_floor_rest_badge() {
    // 플레이어 스킬: mu = 12.0, sigma = 1.0 (Recovery zone: Floor <= 11.2)
    let skill = GaussianSkill::new(12.0, 1.0, 50);

    // [버그 제보 시나리오]: 세션 트렌드가 Recovery일 때, 고난도 곡(Floor 14.0)을 조회하는 상황
    // 14.0층은 고난도이므로 flow_score가 아무리 높아도 REST 뱃지가 발동되면 안 됨!
    let high_floor_reason_no_retry = derive_recommend_reason(
        0.0,                          // retry_score
        0.0,                          // top50_score
        4.0,                          // flow_score (높음)
        None,                         // rank
        Some(SessionTrend::Recovery), // trend = Recovery
        14.0,                         // cand_floor (고난도!)
        Some(&skill),
    );
    assert_eq!(
        high_floor_reason_no_retry, None,
        "High floor (14.0) MUST NOT receive REST badge even when session trend is Recovery!"
    );

    // 만약 고난도 곡에 과거 미흡 기록이 있어 retry_score가 높다면, REST 대신 TRY로 안전하게 전환되어야 함
    let high_floor_reason_with_retry = derive_recommend_reason(
        3.5,                          // retry_score (충분함)
        0.0,                          // top50_score
        4.0,                          // flow_score
        None,                         // rank
        Some(SessionTrend::Recovery), // trend = Recovery
        14.0,                         // cand_floor
        Some(&skill),
    );
    assert_eq!(
        high_floor_reason_with_retry.map(|r| r.kind),
        Some(RecommendReasonKind::Retry),
        "High floor during Recovery should gracefully fallback to Retry reason if retry_score is eligible"
    );
}

#[test]
fn test_score_scale_ranges_and_dominance() {
    let now = 1_000_000i64;

    let strong_retry = retry_priority(90.0, Some(now - 30 * 86400), now);
    assert!((strong_retry - 9.5).abs() < 1e-6);

    let mid_retry = retry_priority(95.0, Some(now - 30 * 86400), now);
    assert!((mid_retry - 4.5).abs() < 1e-6);

    let same_day_retry = retry_priority(90.0, Some(now - 3600), now);
    assert_eq!(same_day_retry, 0.0);

    let max_attack = top50_boundary_score(Some(175.0), None, 175.0, 50);
    assert!((max_attack - 6.0).abs() < 1e-6);

    let max_defend = top50_boundary_score(Some(175.0), Some(50), 175.0, 50);
    assert!((max_defend - 5.0).abs() < 1e-6);

    let climbing_state = SessionTrendState {
        trend: SessionTrend::Climbing,
        avg_rating: 160.0,
        last_floor: 13.0,
        last_diff: Difficulty::SC,
    };
    let max_flow = session_flow_score(13.25, None, 13.0, Some(&climbing_state));
    assert!((max_flow - 4.0).abs() < 1e-6);

    assert!(
        strong_retry > max_attack,
        "Strong Retry (9.5) > Top-50 Attack Max (6.0)"
    );
    assert!(
        max_attack > mid_retry,
        "Top-50 Attack Max (6.0) > Mid Retry (4.5)"
    );
    assert!(
        mid_retry > max_flow,
        "Mid Retry (4.5) > Pure Flow Max (4.0)"
    );
    assert!(
        max_attack + max_flow > strong_retry,
        "Combined Attack+Flow (10.0) > Strong Retry (9.5)"
    );
}

#[test]
fn test_performance_rating_realistic_distribution() {
    let low_standard = calculate_performance_rating(3.0, 99.5);
    assert!(
        low_standard > 35.0 && low_standard < 45.0,
        "floor 3 rate 99.5% expected 35~45, got {}",
        low_standard
    );

    let mid_high = calculate_performance_rating(8.0, 99.8);
    assert!(
        mid_high > 100.0 && mid_high < 110.0,
        "floor 8 rate 99.8% expected 100~110, got {}",
        mid_high
    );

    let high_standard = calculate_performance_rating(13.0, 97.0);
    assert!(
        high_standard > 120.0 && high_standard < 140.0,
        "floor 13 rate 97% expected 120~140, got {}",
        high_standard
    );

    let max = calculate_performance_rating(15.0, 100.0);
    assert!((max - 200.0).abs() < 0.01);

    let retry_target = calculate_performance_rating(13.0, 90.0);
    assert!(
        retry_target > 60.0 && retry_target < 80.0,
        "floor 13 rate 90% expected 60~80, got {}",
        retry_target
    );

    assert_eq!(calculate_performance_rating(0.0, 99.0), 0.0);
    assert_eq!(calculate_performance_rating(13.0, 0.0), 0.0);
}

#[test]
fn test_session_trend_various_patterns() {
    let now = 1_000_000i64;

    let gradual_climb = vec![
        SessionPlayInfo {
            rating: 170.0,
            floor: 13.0,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 300,
        },
        SessionPlayInfo {
            rating: 155.0,
            floor: 12.0,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        },
        SessionPlayInfo {
            rating: 105.0,
            floor: 8.0,
            rate: 99.8,
            diff: Difficulty::SC,
            is_max_combo: true,
            updated_at: now - 900,
        },
    ];
    let state = SessionTrend::analyze_session(&gradual_climb, now);
    assert_eq!(
        state.as_ref().map(|s| s.trend),
        Some(SessionTrend::Climbing)
    );
    let avg = state.as_ref().unwrap().avg_rating;
    assert!(
        (avg - 162.5).abs() < 0.1,
        "avg_rating expected 162.5, got {}",
        avg
    );

    let declining = vec![
        SessionPlayInfo {
            rating: 102.0,
            floor: 14.0,
            rate: 88.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 300,
        },
        SessionPlayInfo {
            rating: 160.0,
            floor: 13.0,
            rate: 98.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        },
        SessionPlayInfo {
            rating: 155.0,
            floor: 12.0,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 900,
        },
    ];
    let state = SessionTrend::analyze_session(&declining, now);
    assert_eq!(
        state.as_ref().map(|s| s.trend),
        Some(SessionTrend::Recovery)
    );

    let steady = vec![
        SessionPlayInfo {
            rating: 162.0,
            floor: 13.0,
            rate: 98.5,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 300,
        },
        SessionPlayInfo {
            rating: 160.0,
            floor: 13.0,
            rate: 98.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        },
        SessionPlayInfo {
            rating: 159.0,
            floor: 13.0,
            rate: 97.8,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 900,
        },
    ];
    let state = SessionTrend::analyze_session(&steady, now);
    assert_eq!(state.as_ref().map(|s| s.trend), Some(SessionTrend::Steady));

    let mc_climb = vec![
        SessionPlayInfo {
            rating: 162.0,
            floor: 13.0,
            rate: 98.5,
            diff: Difficulty::SC,
            is_max_combo: true,
            updated_at: now - 300,
        },
        SessionPlayInfo {
            rating: 160.0,
            floor: 13.0,
            rate: 98.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        },
    ];
    let state = SessionTrend::analyze_session(&mc_climb, now);
    assert_eq!(
        state.as_ref().map(|s| s.trend),
        Some(SessionTrend::Climbing)
    );
}

#[test]
fn test_candidate_ranking_integration_multi_signal_hierarchy() {
    let now = LocalFloorRecommender::now_unix();

    let mut vdb = VArchiveDB::new();
    let make_song = |id: i32, name: &str, floor_str: Option<&str>, dlc: &str| {
        let patterns = if let Some(f) = floor_str {
            serde_json::json!({
                "4B": {
                    "SC": {
                        "level": 13,
                        "floorName": f
                    }
                }
            })
        } else {
            serde_json::json!({
                "4B": {
                    "SC": {
                        "level": 13
                    }
                }
            })
        };
        serde_json::json!({
            "name": name,
            "title": id.to_string(),
            "composer": "Artist",
            "dlcCode": dlc,
            "patterns": patterns
        })
    };

    vdb.songs = vec![
        serde_json::from_value(make_song(0, "Current Song", Some("13.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(1, "Combo (Top50+Flow)", Some("13.25"), "RV")).unwrap(),
        serde_json::from_value(make_song(2, "Strong Retry", Some("13.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(3, "Mid Retry", Some("13.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(4, "Pure Flow", Some("13.25"), "RV")).unwrap(),
        serde_json::from_value(make_song(5, "Weak Signal", Some("13.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(6, "Unplayed Regular", Some("13.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(7, "Unplayed New Target", Some("13.15"), "P1")).unwrap(),
        serde_json::from_value(make_song(
            8,
            "Unowned DLC Song",
            Some("13.15"),
            "UNOWNED_COLLAB",
        ))
        .unwrap(),
        serde_json::from_value(make_song(999, "Session Core 11", Some("11.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(998, "Session Core 10", Some("10.0"), "RV")).unwrap(),
        serde_json::from_value(make_song(997, "Session Warmup 7", Some("7.0"), "RV")).unwrap(),
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("rec_ranking_integration.db");
    let steam_id = "76561198000000001";
    let mut db = crate::store::record_db::RecordDB::new(&db_path, Some(steam_id));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();

    for i in 101..=149 {
        let rating = 174.0 + ((i - 101) as f64 * 0.4);
        let json = serde_json::json!({
            "score": 99.0,
            "maxCombo": true,
            "updatedAt": "2026-08-01T00:00:00.000Z",
            "rating": rating,
        });
        conn.execute(
            "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![steam_id, i.to_string(), "4B", "SC", json.to_string()],
        )
        .unwrap();
    }

    let json_song4 = serde_json::json!({
        "score": 99.5,
        "maxCombo": true,
        "updatedAt": "2026-08-01T00:00:00.000Z",
        "rating": 188.0,
    });
    conn.execute(
        "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![steam_id, "4", "4B", "SC", json_song4.to_string()],
    )
    .unwrap();

    let json_song1 = serde_json::json!({
        "score": 98.5,
        "maxCombo": false,
        "updatedAt": "2026-08-01T00:00:00.000Z",
        "rating": 173.8,
    });
    conn.execute(
        "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![steam_id, "1", "4B", "SC", json_song1.to_string()],
    )
    .unwrap();

    let session_events = [
        (999, "SC", 99.5, now - 300),
        (998, "SC", 98.0, now - 600),
        (997, "SC", 99.8, now - 900),
    ];
    for (sid, diff, rate, ts) in session_events {
        conn.execute(
            "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![steam_id, sid.to_string(), "4B", diff, rate, 0, ts],
        ).unwrap();
    }

    let long_ago = now - 15 * 86400;
    let older = now - 5 * 3600;
    let records = [
        (1, 98.5, long_ago),
        (2, 91.0, long_ago),
        (3, 95.0, long_ago),
        (4, 99.5, long_ago),
        (5, 99.4, older),
    ];
    for (sid, rate, ts) in records {
        conn.execute(
            "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![steam_id, sid.to_string(), "4B", "SC", rate, 0, ts],
        ).unwrap();
    }
    drop(conn);

    let rdb = Arc::new(RecordManager::new(Arc::new(db)));
    rdb.refresh();
    let recommender = LocalFloorRecommender::new(Arc::new(vdb), rdb);

    let ctx = RecommendContext {
        song_id: 0,
        button_mode: Mode::B4,
        difficulty: Difficulty::SC,
        floor_range: 0.5,
        max_results: 10,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };

    let bundle = recommender.recommend(&ctx);
    let entries = bundle.entries;

    assert_eq!(
        entries.len(),
        7,
        "Total 7 candidates (Song 0 and unowned Song 8 excluded)"
    );

    assert_eq!(entries[0].song_id, 1);
    assert_eq!(
        entries[0].reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Top50Attack)
    );

    assert_eq!(entries[1].song_id, 2);
    assert_eq!(
        entries[1].reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Retry)
    );

    assert_eq!(entries[2].song_id, 7);
    assert_eq!(
        entries[2].reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Unplayed)
    );
    assert_eq!(entries[2].rate, None);

    assert_eq!(entries[3].song_id, 3);
    assert_eq!(
        entries[3].reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Retry)
    );

    assert_eq!(entries[4].song_id, 4);
    assert_eq!(
        entries[4].reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Climbing)
    );

    assert_eq!(entries[5].song_id, 6);
    assert_eq!(entries[5].rate, None);
    assert_eq!(entries[5].reason, None);

    assert_eq!(entries[6].song_id, 5);
    assert_eq!(entries[6].reason, None);
}

#[test]
fn test_gaussian_skill_zones() {
    let skill = GaussianSkill::new(12.0, 1.0, 50);

    // mu = 12.0, sigma = 1.0
    // Recovery: Floor <= 12.0 - 0.8*1.0 = 11.2
    assert!(skill.is_recovery_zone(11.2));
    assert!(skill.is_recovery_zone(10.0));
    assert!(!skill.is_recovery_zone(11.3));
    assert!(!skill.is_recovery_zone(14.0)); // 고난도는 절대 recovery 아님!

    // Climbing: 12.0 <= Floor <= 12.0 + 1.2*1.0 = 13.2
    assert!(skill.is_climbing_zone(12.0));
    assert!(skill.is_climbing_zone(12.5));
    assert!(skill.is_climbing_zone(13.2));
    assert!(!skill.is_climbing_zone(11.9));
    assert!(!skill.is_climbing_zone(13.5));

    // Core: |Floor - 12.0| <= 1.0 -> 11.0 ~ 13.0
    assert!(skill.is_core_zone(11.0));
    assert!(skill.is_core_zone(12.0));
    assert!(skill.is_core_zone(13.0));
    assert!(!skill.is_core_zone(10.9));
    assert!(!skill.is_core_zone(13.1));
}

#[test]
fn test_skill_profile_estimation_and_cross_track_fallback() {
    use std::collections::HashMap;

    let mut rank_map = HashMap::new();
    // 4B SC 5곡 (Floor: 12.0, 12.2, 12.4, 12.6, 12.8) -> Mean = 12.4
    for i in 1..=5 {
        rank_map.insert((i, Mode::B4, Difficulty::SC), i as usize);
    }

    let top50 = crate::store::record_db::VArchiveTop50Summary {
        total_recorded_count: 5,
        cutoff_rating: 175.0,
        rank_map,
    };

    let find_floor = |sid: i32, _m: Mode, _d: Difficulty| -> f64 {
        match sid {
            1 => 12.0,
            2 => 12.2,
            3 => 12.4,
            4 => 12.6,
            5 => 12.8,
            _ => 0.0,
        }
    };

    let profile = derive_skill_profile(&top50, &find_floor, Mode::B4);

    // SC Profile
    assert_eq!(profile.sc.sample_count, 5);
    assert!((profile.sc.mu - 12.4).abs() < 0.001);
    assert!(profile.sc.sigma >= 0.8 && profile.sc.sigma <= 2.5);

    // Pad Profile (Cross-track fallback from SC mu 12.4 -> 13.4 clamped, inheriting SC sample strength)
    assert_eq!(profile.pad.sample_count, 5);
    assert!((profile.pad.mu - 13.4).abs() < 0.001);
}

#[test]
fn test_8b_end_of_moonlight_scenario_gating_and_footer() {
    let steam_id = "test_user_8b";
    let now = 1787325000i64;

    let mut vdb = VArchiveDB::new();
    let make_8b_song = |id: i32, name: &str, floor_str: &str, level: i32| {
        serde_json::json!({
            "name": name,
            "title": id.to_string(),
            "composer": "Artist",
            "dlcCode": "RV",
            "patterns": {
                "8B": {
                    "SC": {
                        "level": level,
                        "floorName": floor_str
                    }
                }
            }
        })
    };

    vdb.songs = vec![
        serde_json::from_value(make_8b_song(115, "End of the Moonlight", "9.3", 9)).unwrap(),
        serde_json::from_value(make_8b_song(118, "Extreme Z4", "11.2", 11)).unwrap(),
        serde_json::from_value(make_8b_song(119, "FEAR", "11.1", 11)).unwrap(),
        serde_json::from_value(make_8b_song(156, "Ya! Party!", "12.2", 12)).unwrap(),
        serde_json::from_value(make_8b_song(136, "NB RANGER", "12.1", 12)).unwrap(),
        serde_json::from_value(make_8b_song(10, "Easy Warmup", "5.0", 5)).unwrap(),
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("rec_8b_scenario.db");
    let mut db = crate::store::record_db::RecordDB::new(&db_path, Some(steam_id));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();

    // Top 50에 118(11.2), 119(11.1), 156(12.2), 136(12.1) 등이 등록됨 -> 실력 mu ~ 11.6
    let top_records = [(118, 172.0), (119, 174.0), (156, 178.0), (136, 176.0)];
    for (sid, rating) in top_records {
        let json = serde_json::json!({
            "score": 990000,
            "maxCombo": true,
            "updatedAt": "2026-08-01T00:00:00.000Z",
            "rating": rating,
        });
        conn.execute(
            "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![steam_id, sid.to_string(), "8B", "SC", json.to_string()],
        )
        .unwrap();
    }

    // 최근 플레이 세션:
    // 1. FEAR (11.1층) 99.73% MC (Rating ~147.0)
    // 2. End of Moonlight (9.3층) 99.57% No MC (Rating ~121.3) -> 상대 성과 급락으로 Recovery 트렌드 유발!
    let session_events = [
        (119, "8B", "SC", 99.73, 1, now - 300),
        (115, "8B", "SC", 99.57, 0, now - 100),
    ];
    for (sid, mode, diff, rate, mc, ts) in session_events {
        conn.execute(
            "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![steam_id, sid.to_string(), mode, diff, rate, mc, ts],
        ).unwrap();
    }

    // 과거 고난도 곡 기록들 (Ya! Party 99.01%)
    conn.execute(
        "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
         VALUES (?1, '156', '8B', 'SC', 99.01, 0, ?2)",
        rusqlite::params![steam_id, now - 50],
    ).unwrap();

    drop(conn);

    let rdb = Arc::new(RecordManager::new(Arc::new(db)));
    rdb.refresh();
    let recommender = LocalFloorRecommender::new(Arc::new(vdb), rdb);

    // [검증 1]: 사용자가 고난도 곡 (Ya! Party! 8B SC 12.2) 위에 커서를 두고 추천을 조회할 때
    let ctx_high = RecommendContext {
        song_id: 156,
        button_mode: Mode::B8,
        difficulty: Difficulty::SC,
        floor_range: 0.5,
        max_results: 10,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let bundle_high = recommender.recommend(&ctx_high);
    for entry in &bundle_high.entries {
        if let Some(reason) = &entry.reason {
            assert_ne!(
                reason.kind,
                RecommendReasonKind::Recovery,
                "Song {} (Floor {:?}) MUST NOT have Recovery(REST) badge on high level screen!",
                entry.song_id,
                entry.floor_name
            );
        }
    }

    // [검증 2]: Footer 추천 레벨은 커서 위치에 무관하게 SC 11 또는 SC 12로 일관되게 표시
    let footer_high = recommender.floor_summary(&ctx_high);
    assert_eq!(
        footer_high.recommended_level,
        Some("SC 11".to_string()).or(Some("SC 12".to_string()))
    );

    // [검증 3]: 저난도 손풀기 곡 (Floor 5.0)을 볼 때도 Footer 추천 레벨은 동일하게 유지
    let ctx_low = RecommendContext {
        song_id: 10,
        button_mode: Mode::B8,
        difficulty: Difficulty::SC,
        floor_range: 0.5,
        max_results: 10,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let footer_low = recommender.floor_summary(&ctx_low);
    assert_eq!(
        footer_low.recommended_level, footer_high.recommended_level,
        "Footer level MUST NOT change between low and high cursor selections!"
    );
}

#[test]
fn test_pad_patterns_maintain_consistent_footer_level_across_nm_hd_mx() {
    let steam_id = "test_user_pad";
    let _now = 1787325000i64;

    let mut vdb = VArchiveDB::new();
    // Song 1: NM 4 (공식만), HD 8 (공식만), MX 12 (공식만), SC 12 (12.2)
    // Song 2: NM 5 (공식만), HD 9 (비공식 3.2), MX 13 (비공식 5.1), SC 11 (11.1)
    let make_full_song = |id: i32, name: &str| {
        serde_json::json!({
            "name": name,
            "title": id.to_string(),
            "composer": "Artist",
            "dlcCode": "RV",
            "patterns": {
                "4B": {
                    "NM": { "level": 4 },
                    "HD": { "level": if id == 1 { 8 } else { 9 }, "floorName": if id == 2 { Some("3.2") } else { None } },
                    "MX": { "level": if id == 1 { 12 } else { 13 }, "floorName": if id == 2 { Some("5.1") } else { None } },
                    "SC": { "level": if id == 1 { 12 } else { 11 }, "floorName": if id == 1 { "12.2" } else { "11.1" } }
                }
            }
        })
    };

    vdb.songs = vec![
        serde_json::from_value(make_full_song(1, "Song 1")).unwrap(),
        serde_json::from_value(make_full_song(2, "Song 2")).unwrap(),
    ];

    let temp_dir = tempfile::tempdir().unwrap();
    let db_path = temp_dir.path().join("rec_pad_consistency.db");
    let mut db = crate::store::record_db::RecordDB::new(&db_path, Some(steam_id));
    assert!(db.initialize());

    let conn = db.open_conn().unwrap();
    // Top 50에 SC 12.2 기록 (Rating 175.0) -> SC 실력 mu ~ 12.2, Pad 실력 mu ~ 13.2
    let json = serde_json::json!({
        "score": 995000,
        "maxCombo": true,
        "updatedAt": "2026-08-01T00:00:00.000Z",
        "rating": 175.0,
    });
    conn.execute(
        "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
         VALUES (?1, '1', '4B', 'SC', ?2)",
        rusqlite::params![steam_id, json.to_string()],
    )
    .unwrap();
    drop(conn);

    let rdb = Arc::new(RecordManager::new(Arc::new(db)));
    rdb.refresh();
    let recommender = LocalFloorRecommender::new(Arc::new(vdb), rdb);

    // 1. SC 커서 조회 -> "SC 12"
    let ctx_sc1 = RecommendContext {
        song_id: 1,
        button_mode: Mode::B4,
        difficulty: Difficulty::SC,
        floor_range: 0.5,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let footer_sc1 = recommender.floor_summary(&ctx_sc1);
    assert_eq!(footer_sc1.recommended_level, Some("SC 12".to_string()));

    // 2. 패드 패턴 NM 조회 (Song 1 NM - 공식 레벨 4) -> Pad 추천 "13"
    let ctx_nm = RecommendContext {
        song_id: 1,
        button_mode: Mode::B4,
        difficulty: Difficulty::NM,
        floor_range: 0.5,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let footer_nm = recommender.floor_summary(&ctx_nm);

    // 3. 패드 패턴 HD 조회 (Song 2 HD - 비공식 floor 3.2 존재) -> Pad 추천 동일하게 "13" 유지!
    let ctx_hd = RecommendContext {
        song_id: 2,
        button_mode: Mode::B4,
        difficulty: Difficulty::HD,
        floor_range: 0.5,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let footer_hd = recommender.floor_summary(&ctx_hd);

    // 4. 패드 패턴 MX 조회 (Song 2 MX - 비공식 floor 5.1 존재) -> Pad 추천 동일하게 "13" 유지!
    let ctx_mx = RecommendContext {
        song_id: 2,
        button_mode: Mode::B4,
        difficulty: Difficulty::MX,
        floor_range: 0.5,
        max_results: 6,
        same_mode_only: true,
        v_id: None,
        strategy: RecommendStrategy::Smart,
    };
    let footer_mx = recommender.floor_summary(&ctx_mx);

    // [핵심 검증]: NM, HD, MX 어느 패턴을 가리키든 Pad 추천 레벨은 100% 동일하게 일관성 유지!
    assert_eq!(
        footer_nm.recommended_level,
        Some("13".to_string()),
        "NM cursor recommendation"
    );
    assert_eq!(
        footer_hd.recommended_level, footer_nm.recommended_level,
        "HD cursor MUST match NM recommendation"
    );
    assert_eq!(
        footer_mx.recommended_level, footer_nm.recommended_level,
        "MX cursor MUST match NM recommendation"
    );
}
