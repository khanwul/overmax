use overmax_core::{Difficulty, Mode};
use std::cmp::Ordering;

use super::scoring::{
    calculate_performance_rating, derive_recommend_reason, derive_recommended_level,
    derive_top50_base_floor, retry_priority, session_flow_score, top50_boundary_score,
    unplayed_challenge_score, SessionPlayInfo, SessionTrend,
};
use super::types::{
    RecommendReason, RecommendReasonKind, RecommendStrategy, StrategyFooterParams,
    StrategySortParams,
};

impl RecommendStrategy {
    pub(crate) fn sort_and_annotate<'a, 'b, F>(&self, params: StrategySortParams<'a, 'b, F>)
    where
        F: Fn(i32, Mode, Difficulty) -> f64,
    {
        match self {
            Self::Classic => {
                params
                    .candidates
                    .sort_by(|a, b| match (a.is_played(), b.is_played()) {
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (true, true) => a
                            .rate
                            .partial_cmp(&b.rate)
                            .unwrap_or(Ordering::Equal)
                            .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal)),
                        (false, false) => a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal),
                    });
                params.candidates.truncate(params.max_results);
            }
            Self::Smart => {
                let top50 = params.rdb.get_varchive_top50_summary(params.button_mode);
                let recent_plays = params.rdb.get_recent_records(params.button_mode, 10);
                let session_play_infos: Vec<SessionPlayInfo> = recent_plays
                    .iter()
                    .map(|r| {
                        let floor = (params.find_floor)(r.song_id, r.button_mode, r.difficulty);
                        let rating = calculate_performance_rating(floor, r.rate);
                        SessionPlayInfo {
                            rating,
                            floor,
                            rate: r.rate,
                            diff: r.difficulty,
                            is_max_combo: r.is_max_combo,
                            updated_at: r.updated_at,
                        }
                    })
                    .collect();
                let session_trend_state =
                    SessionTrend::analyze_session(&session_play_infos, params.now_unix);
                let session_trend = session_trend_state.as_ref().map(|s| s.trend);

                params.candidates.sort_by(|a, b| {
                    let pa = if a.is_played() {
                        let a_rank = top50.rank_map.get(&(a.song_id, a.mode, a.diff)).copied();
                        retry_priority(a.rate.unwrap_or(0.0), a.updated_at, params.now_unix)
                            + top50_boundary_score(
                                a.varchive_rating,
                                a_rank,
                                top50.cutoff_rating,
                                top50.total_recorded_count,
                            )
                            + session_flow_score(
                                a.floor,
                                a.rate,
                                params.ref_floor,
                                session_trend_state.as_ref(),
                            )
                    } else {
                        unplayed_challenge_score(
                            a.floor,
                            params.ref_floor,
                            session_trend_state.as_ref(),
                        )
                    };

                    let pb = if b.is_played() {
                        let b_rank = top50.rank_map.get(&(b.song_id, b.mode, b.diff)).copied();
                        retry_priority(b.rate.unwrap_or(0.0), b.updated_at, params.now_unix)
                            + top50_boundary_score(
                                b.varchive_rating,
                                b_rank,
                                top50.cutoff_rating,
                                top50.total_recorded_count,
                            )
                            + session_flow_score(
                                b.floor,
                                b.rate,
                                params.ref_floor,
                                session_trend_state.as_ref(),
                            )
                    } else {
                        unplayed_challenge_score(
                            b.floor,
                            params.ref_floor,
                            session_trend_state.as_ref(),
                        )
                    };

                    pb.partial_cmp(&pa)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal))
                });

                params.candidates.truncate(params.max_results);

                for c in params.candidates.iter_mut() {
                    if c.is_played() {
                        let rank = top50.rank_map.get(&(c.song_id, c.mode, c.diff)).copied();
                        let retry_score =
                            retry_priority(c.rate.unwrap_or(0.0), c.updated_at, params.now_unix);
                        let top50_score = top50_boundary_score(
                            c.varchive_rating,
                            rank,
                            top50.cutoff_rating,
                            top50.total_recorded_count,
                        );
                        let flow_score = session_flow_score(
                            c.floor,
                            c.rate,
                            params.ref_floor,
                            session_trend_state.as_ref(),
                        );
                        c.reason = derive_recommend_reason(
                            retry_score,
                            top50_score,
                            flow_score,
                            rank,
                            session_trend,
                        );
                    } else {
                        let unplayed_score = unplayed_challenge_score(
                            c.floor,
                            params.ref_floor,
                            session_trend_state.as_ref(),
                        );
                        if unplayed_score >= 1.5 {
                            c.reason = Some(RecommendReason {
                                kind: RecommendReasonKind::Unplayed,
                                detail: "미플레이 첫 클리어 도전 추천".to_string(),
                                rank: None,
                            });
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn derive_footer_level<F>(
        &self,
        params: StrategyFooterParams<'_, F>,
    ) -> Option<String>
    where
        F: Fn(i32, Mode, Difficulty) -> f64,
    {
        match self {
            Self::Classic => None,
            Self::Smart => {
                let top50 = params.rdb.get_varchive_top50_summary(params.button_mode);
                let recent_plays = params.rdb.get_recent_records(params.button_mode, 10);
                let session_play_infos: Vec<SessionPlayInfo> = recent_plays
                    .iter()
                    .map(|r| {
                        let floor = (params.find_floor)(r.song_id, r.button_mode, r.difficulty);
                        let rating = calculate_performance_rating(floor, r.rate);
                        SessionPlayInfo {
                            rating,
                            floor,
                            rate: r.rate,
                            diff: r.difficulty,
                            is_max_combo: r.is_max_combo,
                            updated_at: r.updated_at,
                        }
                    })
                    .collect();
                let session_trend_state =
                    SessionTrend::analyze_session(&session_play_infos, params.now_unix);

                let is_sc = params.current_diff.is_sc();
                let top50_base_floor =
                    derive_top50_base_floor(&top50, &params.find_floor, params.button_mode, is_sc);

                derive_recommended_level(
                    session_trend_state.as_ref(),
                    &session_play_infos,
                    params.current_diff,
                    top50_base_floor,
                )
            }
        }
    }
}
