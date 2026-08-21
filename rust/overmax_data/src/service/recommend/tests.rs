use std::sync::Arc;
use std::time::Duration;

use overmax_core::{Difficulty, Mode};

use crate::community::client::VArchiveDB;
use crate::service::recommend::scoring::{
    calculate_performance_rating, derive_recommend_reason, derive_recommended_level,
    retry_priority, session_flow_score, top50_boundary_score, SessionPlayInfo, SessionTrend,
    SessionTrendState,
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
    let base_plays = vec![
        SessionPlayInfo {
            rating: 150.0,
            floor: 12.0,
            rate: 98.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 100,
        },
        SessionPlayInfo {
            rating: 155.0,
            floor: 12.0,
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: true,
            updated_at: now - 300,
        },
    ];

    let climbing_state = SessionTrendState {
        trend: SessionTrend::Climbing,
        avg_rating: 152.5,
        last_floor: 12.0,
        last_diff: Difficulty::SC,
    };
    let rec_level =
        derive_recommended_level(Some(&climbing_state), &base_plays, Difficulty::SC, 12.0);
    assert_eq!(rec_level, Some("SC 12".to_string()));

    let high_plays = vec![SessionPlayInfo {
        rating: 170.0,
        floor: 13.8,
        rate: 99.0,
        diff: Difficulty::SC,
        is_max_combo: false,
        updated_at: now - 100,
    }];
    let climbing_high = SessionTrendState {
        trend: SessionTrend::Climbing,
        avg_rating: 170.0,
        last_floor: 13.8,
        last_diff: Difficulty::SC,
    };
    let rec_level_high =
        derive_recommended_level(Some(&climbing_high), &high_plays, Difficulty::SC, 13.8);
    assert_eq!(rec_level_high, Some("SC 14".to_string()));

    let recovery_state = SessionTrendState {
        trend: SessionTrend::Recovery,
        avg_rating: 152.5,
        last_floor: 12.0,
        last_diff: Difficulty::SC,
    };
    let rec_level_low =
        derive_recommended_level(Some(&recovery_state), &base_plays, Difficulty::SC, 12.0);
    assert_eq!(rec_level_low, Some("SC 12".to_string()));

    let warmup_mixed_plays = vec![
        SessionPlayInfo {
            rating: 100.0,
            floor: 8.0,
            rate: 99.8,
            diff: Difficulty::SC,
            is_max_combo: true,
            updated_at: now - 100,
        },
        SessionPlayInfo {
            rating: 165.0,
            floor: 13.0,
            rate: 98.5,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        },
    ];
    let warmup_state = SessionTrendState {
        trend: SessionTrend::Steady,
        avg_rating: 165.0,
        last_floor: 8.0,
        last_diff: Difficulty::SC,
    };
    let rec_level_warmup = derive_recommended_level(
        Some(&warmup_state),
        &warmup_mixed_plays,
        Difficulty::SC,
        13.0,
    );
    assert_eq!(
        rec_level_warmup,
        Some("SC 13".to_string()),
        "Warmup play (floor 8) must not drag recommended level down from core floor 13"
    );
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
    let r_attack = derive_recommend_reason(1.0, 5.5, 2.0, None, Some(SessionTrend::Climbing));
    assert_eq!(
        r_attack.map(|r| r.kind),
        Some(RecommendReasonKind::Top50Attack)
    );

    let r_defend = derive_recommend_reason(2.0, 4.5, 3.0, Some(45), Some(SessionTrend::Climbing));
    assert_eq!(
        r_defend.map(|r| r.kind),
        Some(RecommendReasonKind::Top50Defend)
    );

    let r_retry = derive_recommend_reason(8.5, 0.0, 2.0, None, Some(SessionTrend::Steady));
    assert_eq!(r_retry.map(|r| r.kind), Some(RecommendReasonKind::Retry));

    let r_climb = derive_recommend_reason(0.5, 0.0, 4.0, None, Some(SessionTrend::Climbing));
    assert_eq!(r_climb.map(|r| r.kind), Some(RecommendReasonKind::Climbing));

    let r_recovery = derive_recommend_reason(0.5, 0.0, 4.0, None, Some(SessionTrend::Recovery));
    assert_eq!(
        r_recovery.map(|r| r.kind),
        Some(RecommendReasonKind::Recovery)
    );

    let r_none = derive_recommend_reason(0.2, 0.0, 0.5, None, Some(SessionTrend::Steady));
    assert_eq!(r_none, None);
}

#[test]
fn test_derive_recommend_reason() {
    let top_attack_reason = derive_recommend_reason(0.0, 5.0, 0.0, None, None);
    assert_eq!(
        top_attack_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Top50Attack)
    );

    let top_defend_reason = derive_recommend_reason(0.0, 4.0, 0.0, Some(45), None);
    assert_eq!(
        top_defend_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Top50Defend)
    );

    let climbing_reason =
        derive_recommend_reason(0.0, 0.0, 3.5, None, Some(SessionTrend::Climbing));
    assert_eq!(
        climbing_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Climbing)
    );

    let recovery_reason =
        derive_recommend_reason(0.0, 0.0, 3.5, None, Some(SessionTrend::Recovery));
    assert_eq!(
        recovery_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Recovery)
    );

    let retry_reason = derive_recommend_reason(5.0, 0.0, 0.0, None, None);
    assert_eq!(
        retry_reason.as_ref().map(|r| r.kind),
        Some(RecommendReasonKind::Retry)
    );

    let low_score_reason = derive_recommend_reason(1.0, 1.2, 0.5, None, None);
    assert_eq!(low_score_reason, None);
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
