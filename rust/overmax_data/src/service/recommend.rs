use crate::community::client::VArchiveDB;
use crate::service::record_manager::{RecordManager, RecordSource};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use overmax_core::{Difficulty, Mode, RecordKey};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendReasonKind {
    /// V-Archive Top-50 컷라인 돌파 타깃
    Top50Attack,
    /// V-Archive Top-50 41~50위 수성
    Top50Defend,
    /// 세션 상승 모멘텀 상위 난이도 도전
    Climbing,
    /// 세션 회복/손풀기
    Recovery,
    /// 방치된 90~99% 기록 재도전
    Retry,
}

impl RecommendReasonKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Top50Attack => "TOP",
            Self::Top50Defend => "DEF",
            Self::Climbing => "UP",
            Self::Recovery => "REST",
            Self::Retry => "TRY",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendReason {
    pub kind: RecommendReasonKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RecommendEntry {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub button_mode: overmax_core::Mode,
    pub difficulty: overmax_core::Difficulty,
    pub level: Option<u32>,
    pub floor: Option<f64>,
    pub floor_name: Option<String>,
    pub rate: Option<f64>,
    pub is_max_combo: bool,
    pub score: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<RecommendReason>,
}

impl RecommendEntry {
    pub fn is_played(&self) -> bool {
        self.rate.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecommendResult {
    pub entries: Vec<RecommendEntry>,
    pub avg_rate: f64,
    pub has_record_count: usize,
    pub total_count: usize,
    pub recommended_level: Option<String>,
}

impl RecommendResult {
    pub fn empty() -> Self {
        Self {
            entries: Vec::new(),
            avg_rate: -1.0,
            has_record_count: 0,
            total_count: 0,
            recommended_level: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct RecommendContext {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub floor_range: f64,
    pub max_results: usize,
    pub same_mode_only: bool,
    pub v_id: Option<String>,
    pub smart_recommend: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VaryDim {
    SongId,
    Mode,
    Diff,
    VId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStatus {
    Ok,
    Stale,
    Skipped,
    Error,
}

#[derive(Debug, Clone)]
pub struct RecommendBundle {
    pub source_id: String,
    pub source_label: String,
    pub entries: Vec<RecommendEntry>,
    pub status: SourceStatus,
}

#[derive(Clone, Debug, Default)]
pub struct LocalRecommendFooter {
    pub avg_rate: f64,
    pub has_record_count: usize,
    pub total_count: usize,
    pub recommended_level: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RecommendPanel {
    pub bundles: Vec<RecommendBundle>,
    pub local_footer: Option<LocalRecommendFooter>,
}

impl RecommendPanel {
    pub fn as_legacy_result(&self) -> RecommendResult {
        let entries = self
            .bundles
            .first()
            .map(|b| b.entries.clone())
            .unwrap_or_default();
        let footer = &self.local_footer;
        RecommendResult {
            entries,
            avg_rate: footer.as_ref().map(|f| f.avg_rate).unwrap_or(-1.0),
            has_record_count: footer.as_ref().map(|f| f.has_record_count).unwrap_or(0),
            total_count: footer.as_ref().map(|f| f.total_count).unwrap_or(0),
            recommended_level: footer.as_ref().and_then(|f| f.recommended_level.clone()),
        }
    }
}

pub trait RecommendationSource: Send + Sync {
    fn source_id(&self) -> &str;
    fn source_label(&self) -> &str;
    fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle;
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FloorCacheKey {
    pub button_mode: overmax_core::Mode,
    pub scale_type: String,
    pub floor_millis: i64,
}

#[derive(Debug, Clone)]
pub struct FloorRateSummary {
    pub total_count: usize,
    pub has_record_count: usize,
    pub rate_sum: f64,
}

impl FloorRateSummary {
    pub fn new(total_count: usize) -> Self {
        Self {
            total_count,
            has_record_count: 0,
            rate_sum: 0.0,
        }
    }

    pub fn avg_rate(&self) -> f64 {
        if self.has_record_count == 0 {
            return -1.0;
        }
        self.rate_sum / self.has_record_count as f64
    }
}

pub struct LocalFloorRecommender {
    vdb: Arc<VArchiveDB>,
    pub(crate) rdb: Arc<RecordManager>,
    floor_rate_cache: Mutex<HashMap<FloorCacheKey, FloorRateSummary>>,
    floor_rate_dirty: Mutex<HashMap<FloorCacheKey, bool>>,
    floor_patterns: Mutex<HashMap<FloorCacheKey, Vec<RecordKey>>>,
    record_to_floor_key: Mutex<HashMap<RecordKey, FloorCacheKey>>,
    cache_index_ready: AtomicBool,
}

struct CandidateSearchParams {
    target_song_id: i32,
    target_mode: Mode,
    target_diff: Difficulty,
    ref_floor: f64,
    use_official: bool,
    ref_diff_grp: &'static str,
    floor_range: f64,
    same_mode_only: bool,
}

/// 재도전 목표 정확도. 100.0(퍼펙트) 대신 현실적인 재도전 유인 지점으로 설정.
const RETRY_TARGET_RATE: f64 = 99.5;
/// 이 시간 이내에 플레이한 곡은 재도전 우선순위에서 사실상 제외(당일 반복 추천 억제).
const RETRY_RECENCY_GRACE_HOURS: f64 = 12.0;
/// 이 일수가 지나면 격차(rate_gap)를 100% 반영(가중치 1.0로 포화).
const RETRY_RECENCY_RAMP_DAYS: f64 = 14.0;

/// 재도전 우선순위 = (목표 정확도 - 현재 rate) * 최근성 가중치.
/// `updated_at`이 없으면(레거시 데이터 등) 최근성 가중 없이 격차만 반환한다.
fn retry_priority(rate: f64, updated_at: Option<i64>, now_unix: i64) -> f64 {
    let rate_gap = (RETRY_TARGET_RATE - rate).max(0.0);
    let Some(updated_at) = updated_at else {
        return rate_gap;
    };
    let days_since = ((now_unix - updated_at).max(0) as f64) / 86400.0;
    let grace_days = RETRY_RECENCY_GRACE_HOURS / 24.0;
    let recency_weight = ((days_since - grace_days) / RETRY_RECENCY_RAMP_DAYS).clamp(0.0, 1.0);
    rate_gap * recency_weight
}

/// Top-50 방어 대상 시작 순위 (41위~50위)
const TOP50_DEFENSE_RANK_START: usize = 41;
/// 컷라인 기준 돌파 후보 탐색 델타 (컷라인 - 2.0 이내)
const TOP50_ATTACK_RATING_DELTA: f64 = 2.0;
/// 41~50위 수성 보너스 최대치
const TOP50_DEFENSE_BONUS: f64 = 5.0;
/// 51위 이하 컷라인 최근접 돌파 보너스 최대치
const TOP50_ATTACK_MAX_BONUS: f64 = 6.0;

/// Top-50 경계 추천 점수 계산 순수 함수.
/// - rank가 Some(41..=50): 컷라인 방어 타깃 (순위가 50위에 가까울수록 높은 보너스 3.0 ~ 5.0)
/// - rank가 None이거나 51위 이상이고 rating이 cutoff_rating - 2.0 이상: 컷라인 돌파 타깃 (0.0 ~ 6.0)
/// - Top-50 데이터가 부족하거나(50개 미만) 무관한 구간: 0.0
fn top50_boundary_score(
    varchive_rating: Option<f64>,
    rank: Option<usize>,
    cutoff_rating: f64,
    total_top_count: usize,
) -> f64 {
    if total_top_count < 50 || cutoff_rating <= 0.0 {
        return 0.0;
    }

    if let Some(r) = rank {
        if (TOP50_DEFENSE_RANK_START..=50).contains(&r) {
            // 41위(3.0) -> 50위(5.0) 로 갈수록 위기감이 크므로 가중치 상승
            let t = (r - TOP50_DEFENSE_RANK_START) as f64 / (50 - TOP50_DEFENSE_RANK_START) as f64;
            return 3.0 + (t * (TOP50_DEFENSE_BONUS - 3.0));
        }
    }

    if let Some(rating) = varchive_rating {
        if rating < cutoff_rating && rating >= (cutoff_rating - TOP50_ATTACK_RATING_DELTA) {
            // 컷라인에 가까울수록 (delta가 0에 가까울수록) 높은 점수
            let delta = cutoff_rating - rating;
            let ratio = 1.0 - (delta / TOP50_ATTACK_RATING_DELTA).clamp(0.0, 1.0);
            return ratio * TOP50_ATTACK_MAX_BONUS;
        }
    }

    0.0
}

/// 난이도(Floor, 0.0 ~ 15.0)와 정확도(Rate, %)를 기반으로 0 ~ 200점 만점 스케일의 Performance Rating을 계산한다.
pub fn calculate_performance_rating(floor: f64, rate: f64) -> f64 {
    if floor <= 0.0 || rate <= 0.0 {
        return 0.0;
    }
    let max_rating = floor * (200.0 / 15.0);
    let rate_ratio = if rate >= 100.0 {
        1.0
    } else if rate >= 99.0 {
        0.95 + ((rate - 99.0) / 1.0) * 0.05
    } else if rate >= 97.0 {
        0.75 + ((rate - 97.0) / 2.0) * 0.20
    } else if rate >= 90.0 {
        0.40 + ((rate - 90.0) / 7.0) * 0.35
    } else {
        (rate / 90.0).max(0.0) * 0.40
    };
    max_rating * rate_ratio
}

/// 상대 상승 모멘텀 판별 레이팅 편차 기준치 (개인 세션 평균 대비 +3.0점 이상)
const MOMENTUM_CLIMB_RATING_DELTA: f64 = 3.0;
/// 상대 저조/회복 판별 레이팅 편차 기준치 (개인 세션 평균 대비 -6.0점 미만)
const MOMENTUM_RECOVERY_RATING_DELTA: f64 = -6.0;
/// 세션 모멘텀 추천 최대 보너스
const MOMENTUM_MAX_BONUS: f64 = 4.0;
/// 세션 만료 유효 윈도우 시간 (4시간)
const SESSION_IDLE_TIMEOUT_HOURS: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionTrend {
    /// 직전 성과 상승 추세 (개인 평균 레이팅 대비 +3.0점 이상 또는 맥콤) -> 상위 난이도(+0.1 ~ +0.4) 도전 권장
    Climbing,
    /// 직전 성과 평이/세션 시작 (개인 평균 레이팅 대비 -6.0 ~ +3.0점) -> 동급 난이도(±0.15) 안정화 권장
    Steady,
    /// 직전 성과 저조 (개인 평균 레이팅 대비 -6.0점 미만) -> 살짝 낮은 난이도(-0.2 ~ -0.5) 회복 권장
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionPlayInfo {
    pub rating: f64,
    pub floor: f64,
    pub diff: Difficulty,
    pub is_max_combo: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionTrendState {
    pub trend: SessionTrend,
    pub avg_rating: f64,
    pub last_floor: f64,
    pub last_diff: Difficulty,
}

impl SessionTrend {
    /// 최근 플레이 레이팅 기록들로부터 플레이어 개인의 세션 평균 대비 상대적 퍼포먼스 추세를 산출한다.
    pub fn analyze_session(
        session_plays: &[SessionPlayInfo],
        now_unix: i64,
    ) -> Option<SessionTrendState> {
        let last_play = session_plays.first()?;
        let elapsed_hours = ((now_unix - last_play.updated_at).max(0) as f64) / 3600.0;
        if elapsed_hours > SESSION_IDLE_TIMEOUT_HOURS {
            return None;
        }

        // 유효 세션 윈도우(4시간 이내) 내 플레이들만 추출
        let valid_plays: Vec<&SessionPlayInfo> = session_plays
            .iter()
            .filter(|r| {
                ((now_unix - r.updated_at).max(0) as f64) / 3600.0 <= SESSION_IDLE_TIMEOUT_HOURS
                    && r.rating > 0.0
            })
            .collect();

        if valid_plays.is_empty() {
            return None;
        }

        let avg_rating =
            valid_plays.iter().map(|r| r.rating).sum::<f64>() / valid_plays.len() as f64;
        let delta = last_play.rating - avg_rating;

        let trend = if valid_plays.len() == 1 {
            Self::Steady
        } else if delta >= MOMENTUM_CLIMB_RATING_DELTA || (last_play.is_max_combo && delta >= 0.0) {
            Self::Climbing
        } else if delta >= MOMENTUM_RECOVERY_RATING_DELTA {
            Self::Steady
        } else {
            Self::Recovery
        };

        Some(SessionTrendState {
            trend,
            avg_rating,
            last_floor: last_play.floor,
            last_diff: last_play.diff,
        })
    }

    /// 레거시 호환 래퍼
    pub fn from_session_plays(session_plays: &[SessionPlayInfo], now_unix: i64) -> Option<Self> {
        Self::analyze_session(session_plays, now_unix).map(|s| s.trend)
    }
}

/// 직전 플레이 성과와 후보 곡의 Floor 및 Rate 관계를 평가하여 세션 모멘텀 가중치를 반환한다.
fn session_flow_score(
    cand_floor: f64,
    cand_rate: Option<f64>,
    ref_floor: f64,
    trend_state: Option<&SessionTrendState>,
) -> f64 {
    let Some(state) = trend_state else {
        return 0.0;
    };

    let delta = cand_floor - ref_floor;

    // 1. 동일 Floor 내부 정렬 (floor_range: 0.0 또는 근접 층) -> 상대 Performance Rating 기반 정규화
    if delta.abs() <= 0.05 {
        if let Some(rate) = cand_rate {
            let cand_rating = calculate_performance_rating(cand_floor, rate);
            let rating_delta = cand_rating - state.avg_rating;

            return match state.trend {
                SessionTrend::Climbing => {
                    // 컨디션 쾌조/상승: 내 평균 실력 대비 살짝 미달인 정복 대상 미숙달곡 (-12.0 ~ -1.5)
                    if (-12.0..=-1.5).contains(&rating_delta) {
                        let center = -6.0;
                        let dist = (rating_delta - center).abs();
                        (1.0 - (dist / 6.0)).max(0.0) * MOMENTUM_MAX_BONUS
                    } else {
                        0.0
                    }
                }
                SessionTrend::Recovery => {
                    // 컨디션 저조/손풀기: 내 실력으로 안정적으로 고득점을 낼 수 있는 검증된 안정곡 (+1.5 이상)
                    if rating_delta >= 1.5 {
                        let ratio = ((rating_delta - 1.5) / 8.0).clamp(0.0, 1.0);
                        ratio * MOMENTUM_MAX_BONUS
                    } else {
                        0.0
                    }
                }
                SessionTrend::Steady => {
                    // 안정 순항: 평균 실력 부근 (±4.0 이내)
                    if rating_delta.abs() <= 4.0 {
                        (1.0 - (rating_delta.abs() / 4.0)) * (MOMENTUM_MAX_BONUS * 0.75)
                    } else {
                        0.0
                    }
                }
            };
        }
        return match state.trend {
            SessionTrend::Steady => MOMENTUM_MAX_BONUS * 0.5,
            _ => 0.0,
        };
    }

    // 2. 서로 다른 Floor 간 정렬 (floor_range > 0일 때의 모멘텀 가중치)
    match state.trend {
        SessionTrend::Climbing => {
            if (0.0..=0.4).contains(&delta) && delta > 0.0 {
                let center = 0.25;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.25)).max(0.0) * MOMENTUM_MAX_BONUS
            } else {
                0.0
            }
        }
        SessionTrend::Steady => {
            if delta.abs() <= 0.15 {
                (1.0 - (delta.abs() / 0.15)) * (MOMENTUM_MAX_BONUS * 0.75)
            } else {
                0.0
            }
        }
        SessionTrend::Recovery => {
            if (-0.5..0.0).contains(&delta) {
                let center = -0.3;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.3)).max(0.0) * MOMENTUM_MAX_BONUS
            } else {
                0.0
            }
        }
    }
}

/// 최근 플레이 기록 및 세션 모멘텀을 기반으로 인게임 공식 레벨 라벨(예: "SC 13", "12")을 도출한다.
fn derive_recommended_level(
    trend_state: Option<&SessionTrendState>,
    current_diff: Difficulty,
    current_floor: f64,
) -> Option<String> {
    let base_floor = trend_state.map(|s| s.last_floor).unwrap_or(current_floor);
    let is_sc = trend_state
        .map(|s| s.last_diff == Difficulty::SC)
        .unwrap_or(current_diff == Difficulty::SC);

    let target_floor = match trend_state.map(|s| s.trend) {
        Some(SessionTrend::Climbing) => base_floor + 0.3,
        Some(SessionTrend::Recovery) => (base_floor - 0.3).max(1.0),
        Some(SessionTrend::Steady) | None => base_floor,
    };

    let level_num = target_floor.round() as u32;
    let clamped_level = level_num.clamp(1, 15);

    if is_sc {
        Some(format!("SC {}", clamped_level))
    } else {
        Some(format!("{}", clamped_level))
    }
}

/// 후보 곡의 각 레인별 점수를 바탕으로 가장 지배적인 추천 사유를 도출한다.
fn derive_recommend_reason(
    retry_score: f64,
    top50_score: f64,
    flow_score: f64,
    varchive_rank: Option<usize>,
    trend: Option<SessionTrend>,
) -> Option<RecommendReason> {
    const REASON_THRESHOLD: f64 = 1.5;

    let max_score = retry_score.max(top50_score).max(flow_score);
    if max_score < REASON_THRESHOLD {
        return None;
    }

    if (top50_score - max_score).abs() < 0.001 && top50_score >= REASON_THRESHOLD {
        if let Some(rank) = varchive_rank {
            if (41..=50).contains(&rank) {
                return Some(RecommendReason {
                    kind: RecommendReasonKind::Top50Defend,
                    detail: format!("Top-50 수성 방어 타깃 (현재 {}위)", rank),
                });
            }
        }
        return Some(RecommendReason {
            kind: RecommendReasonKind::Top50Attack,
            detail: "Top-50 컷라인 돌파 추천 타깃".to_string(),
        });
    }

    if (flow_score - max_score).abs() < 0.001 && flow_score >= REASON_THRESHOLD {
        match trend {
            Some(SessionTrend::Climbing) => {
                return Some(RecommendReason {
                    kind: RecommendReasonKind::Climbing,
                    detail: "세션 상승 모멘텀 상위 난이도 도전".to_string(),
                });
            }
            Some(SessionTrend::Recovery) => {
                return Some(RecommendReason {
                    kind: RecommendReasonKind::Recovery,
                    detail: "세션 회복/손풀기 적정 난이도".to_string(),
                });
            }
            _ => {}
        }
    }

    if (retry_score - max_score).abs() < 0.001 && retry_score >= REASON_THRESHOLD {
        return Some(RecommendReason {
            kind: RecommendReasonKind::Retry,
            detail: "방치된 기록 경신 재도전 추천".to_string(),
        });
    }

    None
}

struct RawCandidate<'a> {
    song_id: i32,
    song: &'a crate::community::client::Song,
    mode: overmax_core::Mode,
    diff: overmax_core::Difficulty,
    level: Option<u32>,
    floor: f64,
    floor_name: Option<Arc<str>>,
    rate: Option<f64>,
    is_max_combo: bool,
    updated_at: Option<i64>,
    varchive_rating: Option<f64>,
    reason: Option<RecommendReason>,
}

impl<'a> RawCandidate<'a> {
    fn is_played(&self) -> bool {
        self.rate.is_some()
    }

    fn into_entry(self) -> RecommendEntry {
        RecommendEntry {
            song_id: self.song_id,
            song_name: self.song.name.to_string(),
            composer: self.song.composer.to_string(),
            button_mode: self.mode,
            difficulty: self.diff,
            level: self.level,
            floor: Some(self.floor),
            floor_name: self.floor_name.map(|s| s.to_string()),
            rate: self.rate,
            is_max_combo: self.is_max_combo,
            score: None,
            reason: self.reason,
        }
    }
}

impl LocalFloorRecommender {
    fn now_unix() -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    }

    pub fn new(vdb: Arc<VArchiveDB>, rdb: Arc<RecordManager>) -> Self {
        Self {
            vdb,
            rdb,
            floor_rate_cache: Mutex::new(HashMap::new()),
            floor_rate_dirty: Mutex::new(HashMap::new()),
            floor_patterns: Mutex::new(HashMap::new()),
            record_to_floor_key: Mutex::new(HashMap::new()),
            cache_index_ready: AtomicBool::new(false),
        }
    }

    fn parse_floor_value(floor_name: Option<&Arc<str>>) -> Option<f64> {
        floor_name.and_then(|s| s.parse::<f64>().ok())
    }

    fn floor_to_millis(floor: f64) -> i64 {
        (floor * 1000.0).round() as i64
    }

    fn diff_group(diff: Difficulty) -> &'static str {
        if diff.is_sc() {
            "SC"
        } else {
            "NHM"
        }
    }

    pub fn floor_summary(&self, ctx: &RecommendContext) -> LocalRecommendFooter {
        let current_song = match self.vdb.search_by_id(ctx.song_id) {
            Some(s) => s,
            None => return LocalRecommendFooter::default(),
        };

        let current_pattern =
            current_song.patterns[ctx.button_mode as usize][ctx.difficulty as usize].as_ref();

        let p = match current_pattern {
            Some(p) => p,
            None => return LocalRecommendFooter::default(),
        };

        let ref_floor = Self::parse_floor_value(p.floor_name.as_ref());
        let use_official = ref_floor.is_none();

        let final_ref_floor = if let Some(floor) = ref_floor {
            floor
        } else {
            p.level.unwrap_or(0) as f64
        };

        let summary = self.get_summary_from_cache(
            ctx.button_mode,
            ctx.difficulty,
            final_ref_floor,
            use_official,
            ctx.floor_range,
            ctx.same_mode_only,
        );

        let recommended_level = if ctx.smart_recommend {
            let recent_plays = self.rdb.get_recent_records(ctx.button_mode, 10);
            let session_play_infos: Vec<SessionPlayInfo> = recent_plays
                .iter()
                .map(|r| {
                    let floor = self.find_pattern_floor(
                        r.song_id,
                        r.button_mode,
                        r.difficulty,
                        use_official,
                    );
                    let rating = calculate_performance_rating(floor, r.rate);
                    SessionPlayInfo {
                        rating,
                        floor,
                        diff: r.difficulty,
                        is_max_combo: r.is_max_combo,
                        updated_at: r.updated_at,
                    }
                })
                .collect();
            let now_unix = Self::now_unix();
            let session_trend_state = SessionTrend::analyze_session(&session_play_infos, now_unix);
            derive_recommended_level(
                session_trend_state.as_ref(),
                ctx.difficulty,
                final_ref_floor,
            )
        } else {
            None
        };

        LocalRecommendFooter {
            avg_rate: summary.avg_rate(),
            has_record_count: summary.has_record_count,
            total_count: summary.total_count,
            recommended_level,
        }
    }

    fn get_candidates<'a>(&'a self, params: CandidateSearchParams) -> Vec<RawCandidate<'a>> {
        let modes_to_check: &[Mode] = if params.same_mode_only {
            std::slice::from_ref(&params.target_mode)
        } else {
            &Mode::ALL
        };

        let mut candidates = Vec::new();

        for song in &self.vdb.songs {
            let sid = match song.title.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };

            for &mode in modes_to_check {
                for diff in Difficulty::ALL {
                    if let Some(p) = song.patterns[mode as usize][diff as usize].as_ref() {
                        let cand_floor_val = Self::parse_floor_value(p.floor_name.as_ref());

                        let final_cand_floor = if params.use_official {
                            if cand_floor_val.is_some()
                                || Self::diff_group(diff) != params.ref_diff_grp
                            {
                                None
                            } else {
                                Some(p.level.unwrap_or(0) as f64)
                            }
                        } else {
                            cand_floor_val
                        };

                        let Some(final_cand_floor) = final_cand_floor else {
                            continue;
                        };

                        if (final_cand_floor - params.ref_floor).abs() > params.floor_range {
                            continue;
                        }

                        if sid == params.target_song_id
                            && mode == params.target_mode
                            && diff == params.target_diff
                        {
                            continue;
                        }

                        candidates.push(RawCandidate {
                            song_id: sid,
                            song,
                            mode,
                            diff,
                            level: p.level,
                            floor: final_cand_floor,
                            floor_name: p.floor_name.clone(),
                            rate: None,
                            is_max_combo: false,
                            updated_at: None,
                            varchive_rating: None,
                            reason: None,
                        });
                    }
                }
            }
        }
        candidates
    }

    fn find_pattern_floor(
        &self,
        song_id: i32,
        mode: Mode,
        diff: Difficulty,
        use_official: bool,
    ) -> f64 {
        let song_id_str = song_id.to_string();
        for s in &self.vdb.songs {
            if s.title == song_id_str {
                if let Some(p) = s.get_pattern(mode, diff) {
                    if use_official {
                        return p
                            .level
                            .map(|lvl| lvl as f64)
                            .or_else(|| Self::parse_floor_value(p.floor_name.as_ref()))
                            .unwrap_or(0.0);
                    } else {
                        return Self::parse_floor_value(p.floor_name.as_ref())
                            .or_else(|| p.level.map(|lvl| lvl as f64))
                            .unwrap_or(0.0);
                    }
                }
                break;
            }
        }
        0.0
    }

    fn merge_record_rates(&self, candidates: &mut [RawCandidate<'_>]) {
        if !self.rdb.is_ready() {
            return;
        }

        let mut unique_ids = Vec::new();
        for c in candidates.iter() {
            if !unique_ids.contains(&c.song_id) {
                unique_ids.push(c.song_id);
            }
        }

        let rate_map = self.rdb.get_rate_map(&unique_ids);
        let updated_at_map = self.rdb.get_local_updated_at_map(&unique_ids);
        let varchive_rating_map = self.rdb.get_varchive_rating_map(&unique_ids);

        for entry in candidates.iter_mut() {
            let key = (entry.song_id, entry.mode, entry.diff);
            if let Some(&(rate, is_max_combo)) = rate_map.get(&key) {
                entry.rate = Some(rate as f64);
                entry.is_max_combo = is_max_combo;
            }
            entry.updated_at = updated_at_map.get(&key).copied();
            entry.varchive_rating = varchive_rating_map.get(&key).copied();
        }
    }

    fn build_floor_cache_index(&self) {
        let mut floor_patterns = HashMap::new();
        let mut record_to_floor_key = HashMap::new();

        for song in &self.vdb.songs {
            let song_id = match song.title.parse::<i32>() {
                Ok(id) => id,
                Err(_) => continue,
            };
            for mode in Mode::ALL {
                for diff in Difficulty::ALL {
                    if let Some(p) = &song.patterns[mode as usize][diff as usize] {
                        let floor_val;
                        let scale_type;
                        if let Some(f) = Self::parse_floor_value(p.floor_name.as_ref()) {
                            floor_val = f;
                            scale_type = "UNOFFICIAL".to_string();
                        } else {
                            if let Some(level) = p.level {
                                floor_val = level as f64;
                            } else {
                                continue;
                            }
                            scale_type = if diff.is_sc() {
                                "OFFICIAL_SC".to_string()
                            } else {
                                "OFFICIAL_NHM".to_string()
                            };
                        }

                        let key = FloorCacheKey {
                            button_mode: mode,
                            scale_type,
                            floor_millis: Self::floor_to_millis(floor_val),
                        };
                        let record_key = (song_id, mode, diff);
                        floor_patterns
                            .entry(key.clone())
                            .or_insert_with(Vec::new)
                            .push(record_key);
                        record_to_floor_key.insert(record_key, key);
                    }
                }
            }
        }

        let mut cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
        let mut patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);
        let mut record_to_key_guard = overmax_core::lock_or_recover(&self.record_to_floor_key);

        *patterns_guard = floor_patterns;
        *record_to_key_guard = record_to_floor_key;

        cache_guard.clear();
        dirty_guard.clear();
        for (key, entries) in patterns_guard.iter() {
            cache_guard.insert(key.clone(), FloorRateSummary::new(entries.len()));
            dirty_guard.insert(key.clone(), true);
        }

        self.cache_index_ready.store(true, AtomicOrdering::SeqCst);
    }

    fn ensure_floor_rate_cache(&self) {
        if !self.cache_index_ready.load(AtomicOrdering::SeqCst) {
            self.build_floor_cache_index();
        }

        let (full_dirty, dirty_keys) = self.rdb.consume_dirty_info();
        {
            let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
            let patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);
            let record_to_key_guard = overmax_core::lock_or_recover(&self.record_to_floor_key);

            if full_dirty {
                for key in patterns_guard.keys() {
                    dirty_guard.insert(key.clone(), true);
                }
            } else {
                for record_key in &dirty_keys {
                    if let Some(floor_key) = record_to_key_guard.get(record_key) {
                        dirty_guard.insert(floor_key.clone(), true);
                    }
                }
            }
        }

        let dirty_floor_keys: Vec<FloorCacheKey> = {
            let dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
            dirty_guard
                .iter()
                .filter(|(_, &is_dirty)| is_dirty)
                .map(|(k, _)| k.clone())
                .collect()
        };

        if dirty_floor_keys.is_empty() {
            return;
        }

        let mut all_song_ids = Vec::new();
        for song in &self.vdb.songs {
            if let Ok(song_id) = song.title.parse::<i32>() {
                all_song_ids.push(song_id);
            }
        }
        all_song_ids.sort_unstable();
        all_song_ids.dedup();

        let rate_map = self.rdb.get_rate_map(&all_song_ids);

        let mut cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        let mut dirty_guard = overmax_core::lock_or_recover(&self.floor_rate_dirty);
        let patterns_guard = overmax_core::lock_or_recover(&self.floor_patterns);

        for key in &dirty_floor_keys {
            let entries = patterns_guard.get(key).cloned().unwrap_or_default();
            let mut summary = FloorRateSummary::new(entries.len());
            for record_key in &entries {
                if let Some(&(rate, _)) = rate_map.get(record_key) {
                    if rate > 0.0 {
                        summary.has_record_count += 1;
                        summary.rate_sum += rate as f64;
                    }
                }
            }
            cache_guard.insert(key.clone(), summary);
            dirty_guard.insert(key.clone(), false);
        }
    }

    fn get_summary_from_cache(
        &self,
        button_mode: Mode,
        difficulty: Difficulty,
        ref_floor: f64,
        use_official: bool,
        floor_range: f64,
        same_mode_only: bool,
    ) -> FloorRateSummary {
        self.ensure_floor_rate_cache();

        let scale_type = if use_official {
            if difficulty.is_sc() {
                "OFFICIAL_SC"
            } else {
                "OFFICIAL_NHM"
            }
        } else {
            "UNOFFICIAL"
        };

        let mut total = 0;
        let mut has_record = 0;
        let mut rate_sum = 0.0;

        let cache_guard = overmax_core::lock_or_recover(&self.floor_rate_cache);
        for (key, summary) in cache_guard.iter() {
            if same_mode_only && key.button_mode != button_mode {
                continue;
            }
            if key.scale_type != scale_type {
                continue;
            }

            let key_floor = key.floor_millis as f64 / 1000.0;
            if (key_floor - ref_floor).abs() > floor_range {
                continue;
            }

            total += summary.total_count;
            has_record += summary.has_record_count;
            rate_sum += summary.rate_sum;
        }

        FloorRateSummary {
            total_count: total,
            has_record_count: has_record,
            rate_sum,
        }
    }
}

impl RecommendationSource for LocalFloorRecommender {
    fn source_id(&self) -> &str {
        "local_floor"
    }

    fn source_label(&self) -> &str {
        "유사 구간"
    }

    fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle {
        let current_song = match self.vdb.search_by_id(ctx.song_id) {
            Some(s) => s,
            None => {
                return RecommendBundle {
                    source_id: self.source_id().to_string(),
                    source_label: self.source_label().to_string(),
                    entries: Vec::new(),
                    status: SourceStatus::Ok,
                }
            }
        };

        let current_pattern =
            current_song.patterns[ctx.button_mode as usize][ctx.difficulty as usize].as_ref();

        let p = match current_pattern {
            Some(p) => p,
            None => {
                return RecommendBundle {
                    source_id: self.source_id().to_string(),
                    source_label: self.source_label().to_string(),
                    entries: Vec::new(),
                    status: SourceStatus::Ok,
                }
            }
        };

        let ref_floor = Self::parse_floor_value(p.floor_name.as_ref());
        let use_official = ref_floor.is_none();

        let (final_ref_floor, ref_diff_grp) = if let Some(floor) = ref_floor {
            (floor, "")
        } else {
            (
                p.level.unwrap_or(0) as f64,
                Self::diff_group(ctx.difficulty),
            )
        };

        let mut candidates = self.get_candidates(CandidateSearchParams {
            target_song_id: ctx.song_id,
            target_mode: ctx.button_mode,
            target_diff: ctx.difficulty,
            ref_floor: final_ref_floor,
            use_official,
            ref_diff_grp,
            floor_range: ctx.floor_range,
            same_mode_only: ctx.same_mode_only,
        });

        self.merge_record_rates(&mut candidates);

        if !ctx.smart_recommend {
            candidates.sort_by(|a, b| match (a.is_played(), b.is_played()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (true, true) => a
                    .rate
                    .partial_cmp(&b.rate)
                    .unwrap_or(Ordering::Equal)
                    .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal)),
                (false, false) => a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal),
            });
            candidates.truncate(ctx.max_results);
            let final_entries = candidates.into_iter().map(|c| c.into_entry()).collect();
            return RecommendBundle {
                source_id: self.source_id().to_string(),
                source_label: self.source_label().to_string(),
                entries: final_entries,
                status: SourceStatus::Ok,
            };
        }

        let top50 = self.rdb.get_varchive_top50_summary(ctx.button_mode);
        let recent_plays = self.rdb.get_recent_records(ctx.button_mode, 10);
        let session_play_infos: Vec<SessionPlayInfo> = recent_plays
            .iter()
            .map(|r| {
                let floor =
                    self.find_pattern_floor(r.song_id, r.button_mode, r.difficulty, use_official);
                let rating = calculate_performance_rating(floor, r.rate);
                SessionPlayInfo {
                    rating,
                    floor,
                    diff: r.difficulty,
                    is_max_combo: r.is_max_combo,
                    updated_at: r.updated_at,
                }
            })
            .collect();
        let now_unix = Self::now_unix();
        let session_trend_state = SessionTrend::analyze_session(&session_play_infos, now_unix);
        let session_trend = session_trend_state.as_ref().map(|s| s.trend);

        candidates.sort_by(|a, b| {
            match (a.is_played(), b.is_played()) {
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (true, true) => {
                    let a_rank = top50.rank_map.get(&(a.song_id, a.mode, a.diff)).copied();
                    let b_rank = top50.rank_map.get(&(b.song_id, b.mode, b.diff)).copied();
                    let pa = retry_priority(a.rate.unwrap_or(0.0), a.updated_at, now_unix)
                        + top50_boundary_score(
                            a.varchive_rating,
                            a_rank,
                            top50.cutoff_rating,
                            top50.total_recorded_count,
                        )
                        + session_flow_score(
                            a.floor,
                            a.rate,
                            final_ref_floor,
                            session_trend_state.as_ref(),
                        );
                    let pb = retry_priority(b.rate.unwrap_or(0.0), b.updated_at, now_unix)
                        + top50_boundary_score(
                            b.varchive_rating,
                            b_rank,
                            top50.cutoff_rating,
                            top50.total_recorded_count,
                        )
                        + session_flow_score(
                            b.floor,
                            b.rate,
                            final_ref_floor,
                            session_trend_state.as_ref(),
                        );
                    // 우선순위 내림차순 정렬: cmp(b, a) 형태로 비교
                    pb.partial_cmp(&pa)
                        .unwrap_or(Ordering::Equal)
                        .then_with(|| a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal))
                }
                (false, false) => a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal),
            }
        });

        candidates.truncate(ctx.max_results);

        let final_entries = candidates
            .into_iter()
            .map(|mut c| {
                if c.is_played() {
                    let rank = top50.rank_map.get(&(c.song_id, c.mode, c.diff)).copied();
                    let retry_score = retry_priority(c.rate.unwrap_or(0.0), c.updated_at, now_unix);
                    let top50_score = top50_boundary_score(
                        c.varchive_rating,
                        rank,
                        top50.cutoff_rating,
                        top50.total_recorded_count,
                    );
                    let flow_score = session_flow_score(
                        c.floor,
                        c.rate,
                        final_ref_floor,
                        session_trend_state.as_ref(),
                    );
                    c.reason = derive_recommend_reason(
                        retry_score,
                        top50_score,
                        flow_score,
                        rank,
                        session_trend,
                    );
                }
                c.into_entry()
            })
            .collect();

        RecommendBundle {
            source_id: self.source_id().to_string(),
            source_label: self.source_label().to_string(),
            entries: final_entries,
            status: SourceStatus::Ok,
        }
    }
}

pub struct ProviderCacheReader {
    pub source_id: String,
    pub source_label: String,
    pub cache_dir: PathBuf,
    pub vary: Vec<VaryDim>,
    pub ttl: Duration,
}

impl ProviderCacheReader {
    pub fn new(
        source_id: impl Into<String>,
        source_label: impl Into<String>,
        cache_dir: impl Into<PathBuf>,
        vary: Vec<VaryDim>,
        ttl: Duration,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            source_label: source_label.into(),
            cache_dir: cache_dir.into(),
            vary,
            ttl,
        }
    }

    pub fn cache_key(&self, ctx: &RecommendContext) -> String {
        if self.vary.is_empty() {
            return "global".to_string();
        }

        let mut parts = Vec::new();
        for dim in &self.vary {
            match dim {
                VaryDim::SongId => parts.push(ctx.song_id.to_string()),
                VaryDim::Mode => parts.push(format!("{:?}", ctx.button_mode)),
                VaryDim::Diff => parts.push(format!("{:?}", ctx.difficulty)),
                VaryDim::VId => parts.push(ctx.v_id.clone().unwrap_or_default()),
            }
        }
        parts.join("_")
    }
}

impl RecommendationSource for ProviderCacheReader {
    fn source_id(&self) -> &str {
        &self.source_id
    }

    fn source_label(&self) -> &str {
        &self.source_label
    }

    fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle {
        let key = self.cache_key(ctx);
        let cache_path = self.cache_dir.join(format!("{}.json", key));

        let metadata = match std::fs::metadata(&cache_path) {
            Ok(m) => m,
            Err(_) => {
                return RecommendBundle {
                    source_id: self.source_id.clone(),
                    source_label: self.source_label.clone(),
                    entries: Vec::new(),
                    status: SourceStatus::Error,
                };
            }
        };

        let elapsed = metadata
            .modified()
            .ok()
            .and_then(|t| t.elapsed().ok())
            .unwrap_or(Duration::MAX);

        let is_stale = elapsed > self.ttl;

        let content = match std::fs::read_to_string(&cache_path) {
            Ok(c) => c,
            Err(_) => {
                return RecommendBundle {
                    source_id: self.source_id.clone(),
                    source_label: self.source_label.clone(),
                    entries: Vec::new(),
                    status: SourceStatus::Error,
                };
            }
        };

        #[derive(Deserialize)]
        struct ProviderPayload {
            protocol: String,
            #[serde(default)]
            entries: Vec<ProviderEntry>,
        }

        #[derive(Deserialize)]
        struct ProviderEntry {
            song_id: i32,
            mode: String,
            diff: String,
            #[serde(default)]
            reason: Option<String>,
            #[serde(default)]
            score: Option<f64>,
        }

        let payload: ProviderPayload = match serde_json::from_str(&content) {
            Ok(p) => p,
            Err(_) => {
                return RecommendBundle {
                    source_id: self.source_id.clone(),
                    source_label: self.source_label.clone(),
                    entries: Vec::new(),
                    status: SourceStatus::Error,
                };
            }
        };

        if payload.protocol != "overmax-recommend/1" || payload.entries.is_empty() {
            return RecommendBundle {
                source_id: self.source_id.clone(),
                source_label: self.source_label.clone(),
                entries: Vec::new(),
                status: SourceStatus::Error,
            };
        }

        let mut entries = Vec::new();
        for pe in payload.entries {
            let Some(mode) = Mode::from_str(&pe.mode) else {
                continue;
            };
            let Some(diff) = Difficulty::from_str(&pe.diff) else {
                continue;
            };
            entries.push(RecommendEntry {
                song_id: pe.song_id,
                song_name: String::new(),
                composer: String::new(),
                button_mode: mode,
                difficulty: diff,
                level: None,
                floor: None,
                floor_name: pe.reason,
                rate: None,
                is_max_combo: false,
                score: pe.score,
                reason: None,
            });
        }

        RecommendBundle {
            source_id: self.source_id.clone(),
            source_label: self.source_label.clone(),
            entries,
            status: if is_stale {
                SourceStatus::Stale
            } else {
                SourceStatus::Ok
            },
        }
    }
}

#[derive(Clone)]
pub struct CompositeRecommender {
    local: Arc<LocalFloorRecommender>,
    provider: Option<Arc<ProviderCacheReader>>,
}

impl CompositeRecommender {
    pub fn new(vdb: Arc<VArchiveDB>, rdb: Arc<RecordManager>) -> Self {
        Self {
            local: Arc::new(LocalFloorRecommender::new(vdb, rdb)),
            provider: None,
        }
    }

    pub fn with_provider(mut self, provider: ProviderCacheReader) -> Self {
        self.provider = Some(Arc::new(provider));
        self
    }

    /// Re-binds the recommender with a newly updated VArchiveDB while preserving
    /// existing providers and record manager references.
    pub fn with_varchive_db(&self, new_vdb: Arc<VArchiveDB>) -> Self {
        Self {
            local: Arc::new(LocalFloorRecommender::new(new_vdb, self.local.rdb.clone())),
            provider: self.provider.clone(),
        }
    }

    pub fn local(&self) -> &Arc<LocalFloorRecommender> {
        &self.local
    }

    pub fn recommend_panel(&self, ctx: &RecommendContext) -> RecommendPanel {
        let local_bundle = self.local.recommend(ctx);
        let local_footer = self.local.floor_summary(ctx);

        let provider_bundle = self.provider.as_ref().map(|p| {
            let mut b = p.recommend(ctx);
            self.enrich_bundle(&mut b);
            b
        });

        let bundles = match provider_bundle {
            Some(b) if b.status == SourceStatus::Ok && !b.entries.is_empty() => {
                vec![b, local_bundle]
            }
            _ => vec![local_bundle],
        };

        RecommendPanel {
            bundles,
            local_footer: Some(local_footer),
        }
    }

    fn enrich_bundle(&self, bundle: &mut RecommendBundle) {
        let vdb = &self.local.vdb;
        let rdb = &self.local.rdb;

        let mut enriched_entries = Vec::new();
        let mut unique_ids = Vec::new();

        for entry in &bundle.entries {
            let Some(song) = vdb.search_by_id(entry.song_id) else {
                continue;
            };

            let pattern =
                song.patterns[entry.button_mode as usize][entry.difficulty as usize].as_ref();
            let level = pattern.and_then(|p| p.level);
            let floor_val = pattern
                .and_then(|p| LocalFloorRecommender::parse_floor_value(p.floor_name.as_ref()));

            if !unique_ids.contains(&entry.song_id) {
                unique_ids.push(entry.song_id);
            }

            enriched_entries.push(RecommendEntry {
                song_id: entry.song_id,
                song_name: song.name.to_string(),
                composer: song.composer.to_string(),
                button_mode: entry.button_mode,
                difficulty: entry.difficulty,
                level,
                floor: floor_val,
                floor_name: entry.floor_name.clone(),
                rate: None,
                is_max_combo: false,
                score: entry.score,
                reason: None,
            });
        }

        if rdb.is_ready() && !unique_ids.is_empty() {
            let rate_map = rdb.get_rate_map(&unique_ids);
            for entry in &mut enriched_entries {
                if let Some(&(rate, is_max_combo)) =
                    rate_map.get(&(entry.song_id, entry.button_mode, entry.difficulty))
                {
                    entry.rate = Some(rate as f64);
                    entry.is_max_combo = is_max_combo;
                }
            }
        }

        enriched_entries.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        bundle.entries = enriched_entries;
    }

    pub fn recommend(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
        floor_range: f64,
        max_results: usize,
        same_mode_only: bool,
    ) -> RecommendResult {
        let ctx = RecommendContext {
            song_id,
            button_mode,
            difficulty,
            floor_range,
            max_results,
            same_mode_only,
            v_id: None,
            smart_recommend: true,
        };
        self.recommend_panel(&ctx).as_legacy_result()
    }
}

pub type Recommender = CompositeRecommender;

#[cfg(test)]
mod tests {
    use super::*;

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
            smart_recommend: true,
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
            "source": "test_provider",
            "entries": [
                { "song_id": 10, "mode": "4B", "diff": "SC", "reason": "test", "score": 99.5 }
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
            difficulty: Difficulty::NM,
            floor_range: 0.0,
            max_results: 6,
            same_mode_only: true,
            v_id: None,
            smart_recommend: true,
        };

        let bundle = reader.recommend(&ctx);
        assert_eq!(bundle.status, SourceStatus::Ok);
        assert_eq!(bundle.entries.len(), 1);
        assert_eq!(bundle.entries[0].song_id, 10);
        assert_eq!(bundle.entries[0].button_mode, Mode::B4);
        assert_eq!(bundle.entries[0].difficulty, Difficulty::SC);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_composite_recommender_with_varchive_db_preserves_provider() {
        let vdb1 = Arc::new(VArchiveDB::new());
        let vdb2 = Arc::new(VArchiveDB::new());
        let temp_dir = std::env::temp_dir().join(format!("rec_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let record_db = Arc::new(crate::store::record_db::RecordDB::new(
            temp_dir.join("rec.db"),
            None,
        ));
        let rdb = Arc::new(RecordManager::new(record_db));

        let reader = ProviderCacheReader::new(
            "custom_source",
            "Custom Source",
            &temp_dir,
            vec![],
            Duration::from_secs(60),
        );

        let recommender = CompositeRecommender::new(vdb1, rdb).with_provider(reader);
        assert!(recommender.provider.is_some());

        let rebound = recommender.with_varchive_db(vdb2);
        assert!(rebound.provider.is_some());
        assert_eq!(
            rebound.provider.as_ref().unwrap().source_id(),
            "custom_source"
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn retry_priority_weights_gap_by_recency_and_grace_period() {
        let now = 1_000_000i64;

        // 1시간 전 플레이 (12시간 유예 이내) -> 가중치 거의 0
        let just_played = retry_priority(90.0, Some(now - 3600), now);
        assert!(just_played < 0.01);

        // 유예(12h) + 램프(14일)를 완전히 지남 -> 가중치 1.0, 격차 100% 반영
        let long_ago = retry_priority(90.0, Some(now - (14 * 86400 + 13 * 3600)), now);
        assert!((long_ago - 9.5).abs() < 0.05); // 99.5 - 90.0 = 9.5

        // 이미 목표치 이상 -> 격차 0, 최근성과 무관하게 0
        let maxed = retry_priority(99.5, Some(now - 30 * 86400), now);
        assert_eq!(maxed, 0.0);

        // updated_at 없음(레거시 로우) -> 최근성 가중 없이 순수 격차만 반환 (기존 동작과 호환)
        let no_timestamp = retry_priority(95.0, None, now);
        assert_eq!(no_timestamp, 4.5);
    }

    #[test]
    fn test_top50_boundary_score_cases() {
        let cutoff = 172.96;
        let count = 50;

        // Case 1: 41위 (방어 하한, 3.0점)
        let rank_41 = top50_boundary_score(Some(173.78), Some(41), cutoff, count);
        assert!((rank_41 - 3.0).abs() < 0.01);

        // Case 2: 50위 (방어 상한, 5.0점)
        let rank_50 = top50_boundary_score(Some(172.96), Some(50), cutoff, count);
        assert!((rank_50 - 5.0).abs() < 0.01);

        // Case 3: 1위 (상위 1~40위 안정권 -> 0.0)
        let rank_1 = top50_boundary_score(Some(185.85), Some(1), cutoff, count);
        assert_eq!(rank_1, 0.0);

        // Case 4: 51위 이하 컷라인 직하 (cutoff - 0.1) -> 높은 돌파 보너스 (~5.7점)
        let near_attack = top50_boundary_score(Some(cutoff - 0.1), None, cutoff, count);
        assert!(near_attack > 5.5 && near_attack <= 6.0);

        // Case 5: 컷라인 - 2.0 초과 격차 (cutoff - 3.0) -> 0.0
        let far_attack = top50_boundary_score(Some(cutoff - 3.0), None, cutoff, count);
        assert_eq!(far_attack, 0.0);

        // Case 6: Top-50 미만 (데이터 부족, count < 50) -> 항상 0.0
        let insufficient = top50_boundary_score(Some(cutoff - 0.1), None, cutoff, 30);
        assert_eq!(insufficient, 0.0);
    }

    #[test]
    fn test_session_flow_score_and_timeout() {
        let now = 1_000_000i64;

        // 1. calculate_performance_rating 기본 검증
        let r_15_perfect = calculate_performance_rating(15.0, 100.0);
        assert!((r_15_perfect - 200.0).abs() < 0.01);
        let r_15_99 = calculate_performance_rating(15.0, 99.0);
        assert!((r_15_99 - 190.0).abs() < 0.01);
        let r_8_high = calculate_performance_rating(8.0, 99.8);
        assert!(r_8_high > 105.0 && r_8_high < 107.0);

        // 2. 세션 만료 테스트 (5시간 전 플레이 -> timeout -> None)
        let expired_play = vec![SessionPlayInfo {
            rating: 180.0,
            floor: 14.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 5 * 3600,
        }];
        assert_eq!(SessionTrend::analyze_session(&expired_play, now), None);

        // 3. 세션 첫 판 (1개 플레이 -> Steady)
        let first_play = vec![SessionPlayInfo {
            rating: 150.0,
            floor: 12.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 600,
        }];
        let first_state = SessionTrend::analyze_session(&first_play, now);
        assert_eq!(
            first_state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Steady)
        );

        // 4. 난이도 정규화 시뮬레이션:
        // Case A: 15층 97.0% 달성 (레이팅 150.0점) vs 세션 평균 (140.0점) -> +10.0점 편차 -> Climbing!
        let pro_climbing = vec![
            SessionPlayInfo {
                rating: 150.0, // 15층 97.0% (고난도 도전 성공)
                floor: 15.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 135.0,
                floor: 13.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 135.0,
                floor: 13.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 900,
            },
        ];
        let climb_state = SessionTrend::analyze_session(&pro_climbing, now);
        assert_eq!(
            climb_state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Climbing)
        );
        // Climbing: +0.25 도전곡에 최고 보너스(4.0)
        let climb_score = session_flow_score(12.25, None, 12.0, climb_state.as_ref());
        assert!((climb_score - 4.0).abs() < 0.01);

        // Case B: 8층 99.8% 달성 (레이팅 105.6점, 단순 손풀기) vs 세션 평균 (140.0점) -> -34.4점 -> Recovery/Warmup
        let warmup_play = vec![
            SessionPlayInfo {
                rating: 105.6, // 8층 99.8% 손풀기
                floor: 8.0,
                diff: Difficulty::SC,
                is_max_combo: true,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 150.0,
                floor: 13.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 160.0,
                floor: 13.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 900,
            },
        ];
        let warmup_state = SessionTrend::analyze_session(&warmup_play, now);
        assert_eq!(
            warmup_state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Recovery)
        );
        // Recovery: -0.3 회복곡에 최고 보너스(4.0)
        let rec_score = session_flow_score(11.7, None, 12.0, warmup_state.as_ref());
        assert!((rec_score - 4.0).abs() < 0.01);
    }

    #[test]
    fn test_derive_recommended_level() {
        let state_climb = SessionTrendState {
            trend: SessionTrend::Climbing,
            avg_rating: 170.0,
            last_floor: 12.8,
            last_diff: Difficulty::SC,
        };
        // 12.8 + 0.3 = 13.1 -> round 13 -> "SC 13"
        assert_eq!(
            derive_recommended_level(Some(&state_climb), Difficulty::SC, 12.8),
            Some("SC 13".to_string())
        );

        let state_recovery = SessionTrendState {
            trend: SessionTrend::Recovery,
            avg_rating: 170.0,
            last_floor: 13.2,
            last_diff: Difficulty::SC,
        };
        // 13.2 - 0.3 = 12.9 -> round 13 -> "SC 13", 13.0 - 0.3 = 12.7 -> round 13
        assert_eq!(
            derive_recommended_level(Some(&state_recovery), Difficulty::SC, 13.0),
            Some("SC 13".to_string())
        );

        let state_normal = SessionTrendState {
            trend: SessionTrend::Steady,
            avg_rating: 140.0,
            last_floor: 11.0,
            last_diff: Difficulty::MX,
        };
        assert_eq!(
            derive_recommended_level(Some(&state_normal), Difficulty::MX, 11.0),
            Some("11".to_string())
        );
    }

    #[test]
    fn test_derive_recommend_reason() {
        // Case 1: Top-50 방어 (45위)
        let reason_def = derive_recommend_reason(1.0, 4.5, 0.0, Some(45), None);
        assert_eq!(
            reason_def,
            Some(RecommendReason {
                kind: RecommendReasonKind::Top50Defend,
                detail: "Top-50 수성 방어 타깃 (현재 45위)".to_string(),
            })
        );

        // Case 2: Top-50 돌파 (컷라인 직하)
        let reason_att = derive_recommend_reason(2.0, 5.8, 1.0, None, None);
        assert_eq!(
            reason_att,
            Some(RecommendReason {
                kind: RecommendReasonKind::Top50Attack,
                detail: "Top-50 컷라인 돌파 추천 타깃".to_string(),
            })
        );

        // Case 3: 세션 모멘텀 상승
        let reason_climb =
            derive_recommend_reason(0.0, 0.0, 4.0, None, Some(SessionTrend::Climbing));
        assert_eq!(
            reason_climb,
            Some(RecommendReason {
                kind: RecommendReasonKind::Climbing,
                detail: "세션 상승 모멘텀 상위 난이도 도전".to_string(),
            })
        );

        // Case 4: 방치 재도전
        let reason_retry = derive_recommend_reason(4.5, 0.0, 0.0, None, None);
        assert_eq!(
            reason_retry,
            Some(RecommendReason {
                kind: RecommendReasonKind::Retry,
                detail: "방치된 기록 경신 재도전 추천".to_string(),
            })
        );

        // Case 5: 점수 미달 (< 1.5) -> 사유 없음(None, 뱃지 생략)
        let reason_none = derive_recommend_reason(0.5, 0.0, 1.0, None, None);
        assert_eq!(reason_none, None);
    }

    #[test]
    fn test_smart_recommend_false_sorts_by_classic_rate_and_omits_reason() {
        let vdb = Arc::new(VArchiveDB::new());
        let temp_dir =
            std::env::temp_dir().join(format!("rec_test_classic_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let record_db = Arc::new(crate::store::record_db::RecordDB::new(
            temp_dir.join("rec.db"),
            None,
        ));
        let rdb = Arc::new(RecordManager::new(record_db));
        let recommender = LocalFloorRecommender::new(vdb, rdb);

        let ctx_classic = RecommendContext {
            song_id: 1,
            button_mode: Mode::B4,
            difficulty: Difficulty::NM,
            floor_range: 0.0,
            max_results: 6,
            same_mode_only: true,
            v_id: None,
            smart_recommend: false,
        };

        let footer = recommender.floor_summary(&ctx_classic);
        assert_eq!(footer.recommended_level, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
