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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<usize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum RecommendStrategy {
    #[default]
    Smart,
    Classic,
}

impl RecommendStrategy {
    pub fn from_smart_flag(smart: bool) -> Self {
        if smart {
            Self::Smart
        } else {
            Self::Classic
        }
    }

    pub fn is_smart(&self) -> bool {
        matches!(self, Self::Smart)
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
    pub strategy: RecommendStrategy,
}

impl RecommendContext {
    pub fn smart_recommend(&self) -> bool {
        self.strategy.is_smart()
    }
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
    pub rate: f64,
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
    /// 손풀기(Warm-up) 곡은 세션 평균을 왜곡하지 않도록 분리 집계한다.
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

        // 세션 내 최고 레이팅(Peak Rating)
        let peak_rating = valid_plays
            .iter()
            .map(|r| r.rating)
            .fold(0.0f64, |acc, r| acc.max(r));

        // 손풀기 곡 임계값: 최고 레이팅 대비 30.0점 미만 (약 2.5~3 Floor 차이)
        const WARMUP_RATING_GAP: f64 = 30.0;
        let is_warmup = |r: &SessionPlayInfo| peak_rating - r.rating > WARMUP_RATING_GAP;

        // 주력 플레이들이 1개 이상 존재하면 손풀기 곡은 세션 평균 레이팅 계산에서 제외
        let core_plays: Vec<&SessionPlayInfo> = valid_plays
            .iter()
            .copied()
            .filter(|r| !is_warmup(r))
            .collect();

        let target_plays = if !core_plays.is_empty() {
            &core_plays
        } else {
            &valid_plays
        };

        let avg_rating =
            target_plays.iter().map(|r| r.rating).sum::<f64>() / target_plays.len() as f64;

        let trend = if valid_plays.len() == 1 {
            Self::Steady
        } else if is_warmup(last_play) {
            // 직전 곡이 손풀기 곡일 때: 절대 rating 편차가 아닌 손풀기 곡에서의 정확도(Rate)로 판정
            if last_play.rate >= 99.7 || last_play.is_max_combo {
                Self::Steady // 손풀기 성과 양호 -> 컨디션 유지
            } else if last_play.rate < 97.0 {
                Self::Recovery // 손풀기에서 삐끗함 -> 회복 필요
            } else {
                Self::Steady
            }
        } else {
            let delta = last_play.rating - avg_rating;
            if delta >= MOMENTUM_CLIMB_RATING_DELTA || (last_play.is_max_combo && delta >= 0.0) {
                Self::Climbing
            } else if delta >= MOMENTUM_RECOVERY_RATING_DELTA {
                Self::Steady
            } else {
                Self::Recovery
            }
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
/// 손풀기 곡으로 인해 권장 레벨이 급락하지 않도록 세션 내 최고/주력 난이도를 앵커로 방어한다.
fn derive_recommended_level(
    trend_state: Option<&SessionTrendState>,
    session_plays: &[SessionPlayInfo],
    current_diff: Difficulty,
    current_floor: f64,
) -> Option<String> {
    let is_sc = current_diff == Difficulty::SC;

    // 1. 현재 탭 난이도 계열(SC vs 일반)과 일치하는 세션 플레이들
    let same_diff_plays: Vec<&SessionPlayInfo> = session_plays
        .iter()
        .filter(|p| (p.diff == Difficulty::SC) == is_sc && p.floor > 0.0)
        .collect();

    // 2. 세션 내 최고 난이도(Peak Floor)
    let peak_floor = same_diff_plays
        .iter()
        .map(|p| p.floor)
        .fold(0.0f64, |acc, f| acc.max(f));

    // 3. 기준 난이도(Base Floor) 결정
    let base_floor = if same_diff_plays.is_empty() {
        current_floor
    } else {
        let core_floor = peak_floor.max(current_floor);
        let last_play = same_diff_plays.first().unwrap();

        const WARMUP_FLOOR_GAP: f64 = 2.5;
        if core_floor - last_play.floor >= WARMUP_FLOOR_GAP {
            // 직전 곡이 주력층 대비 2.5레벨 이상 낮은 손풀기 곡이면 주력층(core_floor)을 유지!
            core_floor
        } else {
            // 주력 곡이면 직전 플레이 난이도와 현재 선곡창 난이도 중 유효한 난이도 반영
            last_play.floor.max(current_floor.min(core_floor))
        }
    };

    // 4. 컨디션 모멘텀 오프셋 적용
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
                    rank: Some(rank),
                });
            }
        }
        return Some(RecommendReason {
            kind: RecommendReasonKind::Top50Attack,
            detail: "Top-50 컷라인 돌파 추천 타깃".to_string(),
            rank: None,
        });
    }

    if (flow_score - max_score).abs() < 0.001 && flow_score >= REASON_THRESHOLD {
        match trend {
            Some(SessionTrend::Climbing) => {
                return Some(RecommendReason {
                    kind: RecommendReasonKind::Climbing,
                    detail: "세션 상승 모멘텀 상위 난이도 도전".to_string(),
                    rank: None,
                });
            }
            Some(SessionTrend::Recovery) => {
                return Some(RecommendReason {
                    kind: RecommendReasonKind::Recovery,
                    detail: "세션 회복/손풀기 적정 난이도".to_string(),
                    rank: None,
                });
            }
            _ => {}
        }
    }

    if (retry_score - max_score).abs() < 0.001 && retry_score >= REASON_THRESHOLD {
        return Some(RecommendReason {
            kind: RecommendReasonKind::Retry,
            detail: "방치된 기록 경신 재도전 추천".to_string(),
            rank: None,
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

struct StrategySortParams<'a, 'b, F>
where
    F: Fn(i32, Mode, Difficulty) -> f64,
{
    candidates: &'b mut Vec<RawCandidate<'a>>,
    rdb: &'b RecordManager,
    find_floor: F,
    button_mode: Mode,
    ref_floor: f64,
    max_results: usize,
    now_unix: i64,
}

struct StrategyFooterParams<'a, F>
where
    F: Fn(i32, Mode, Difficulty) -> f64,
{
    rdb: &'a RecordManager,
    find_floor: F,
    button_mode: Mode,
    current_diff: Difficulty,
    ref_floor: f64,
    now_unix: i64,
}

impl RecommendStrategy {
    fn sort_and_annotate<'a, 'b, F>(&self, params: StrategySortParams<'a, 'b, F>)
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

                params
                    .candidates
                    .sort_by(|a, b| match (a.is_played(), b.is_played()) {
                        (true, false) => Ordering::Less,
                        (false, true) => Ordering::Greater,
                        (true, true) => {
                            let a_rank = top50.rank_map.get(&(a.song_id, a.mode, a.diff)).copied();
                            let b_rank = top50.rank_map.get(&(b.song_id, b.mode, b.diff)).copied();
                            let pa = retry_priority(
                                a.rate.unwrap_or(0.0),
                                a.updated_at,
                                params.now_unix,
                            ) + top50_boundary_score(
                                a.varchive_rating,
                                a_rank,
                                top50.cutoff_rating,
                                top50.total_recorded_count,
                            ) + session_flow_score(
                                a.floor,
                                a.rate,
                                params.ref_floor,
                                session_trend_state.as_ref(),
                            );
                            let pb = retry_priority(
                                b.rate.unwrap_or(0.0),
                                b.updated_at,
                                params.now_unix,
                            ) + top50_boundary_score(
                                b.varchive_rating,
                                b_rank,
                                top50.cutoff_rating,
                                top50.total_recorded_count,
                            ) + session_flow_score(
                                b.floor,
                                b.rate,
                                params.ref_floor,
                                session_trend_state.as_ref(),
                            );
                            pb.partial_cmp(&pa)
                                .unwrap_or(Ordering::Equal)
                                .then_with(|| {
                                    a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal)
                                })
                        }
                        (false, false) => a.floor.partial_cmp(&b.floor).unwrap_or(Ordering::Equal),
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
                    }
                }
            }
        }
    }

    fn derive_footer_level<F>(&self, params: StrategyFooterParams<'_, F>) -> Option<String>
    where
        F: Fn(i32, Mode, Difficulty) -> f64,
    {
        match self {
            Self::Classic => None,
            Self::Smart => {
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
                derive_recommended_level(
                    session_trend_state.as_ref(),
                    &session_play_infos,
                    params.current_diff,
                    params.ref_floor,
                )
            }
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

        let recommended_level = ctx.strategy.derive_footer_level(StrategyFooterParams {
            rdb: &self.rdb,
            find_floor: |sid, m, d| self.find_pattern_floor(sid, m, d, use_official),
            button_mode: ctx.button_mode,
            current_diff: ctx.difficulty,
            ref_floor: final_ref_floor,
            now_unix: Self::now_unix(),
        });

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

        let now_unix = Self::now_unix();
        ctx.strategy.sort_and_annotate(StrategySortParams {
            candidates: &mut candidates,
            rdb: &self.rdb,
            find_floor: |sid, m, d| self.find_pattern_floor(sid, m, d, use_official),
            button_mode: ctx.button_mode,
            ref_floor: final_ref_floor,
            max_results: ctx.max_results,
            now_unix,
        });

        let final_entries = candidates.into_iter().map(|c| c.into_entry()).collect();

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
            strategy: RecommendStrategy::Smart,
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
            strategy: RecommendStrategy::Smart,
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
            rate: 99.0,
            diff: Difficulty::SC,
            is_max_combo: false,
            updated_at: now - 5 * 3600,
        }];
        assert_eq!(SessionTrend::analyze_session(&expired_play, now), None);

        // 3. 세션 첫 판 (1개 플레이 -> Steady)
        let first_play = vec![SessionPlayInfo {
            rating: 150.0,
            floor: 12.0,
            rate: 99.0,
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
                rate: 97.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 135.0,
                floor: 13.0,
                rate: 98.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 135.0,
                floor: 13.0,
                rate: 98.0,
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

        // Case B: 8층 99.8% 달성 (손풀기 양호) -> 세션 평균을 왜곡하지 않고 Steady 유지!
        let warmup_play = vec![
            SessionPlayInfo {
                rating: 105.6, // 8층 99.8% 손풀기
                floor: 8.0,
                rate: 99.8,
                diff: Difficulty::SC,
                is_max_combo: true,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 150.0,
                floor: 13.0,
                rate: 98.5,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 160.0,
                floor: 13.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 900,
            },
        ];
        let warmup_state = SessionTrend::analyze_session(&warmup_play, now);
        assert_eq!(
            warmup_state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Steady)
        );

        // Case C: 8층에서 95.0%로 삐끗 (손풀기 저조) -> Recovery 판정
        let warmup_miss = vec![
            SessionPlayInfo {
                rating: 70.0,
                floor: 8.0,
                rate: 95.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 150.0,
                floor: 13.0,
                rate: 98.5,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
        ];
        let miss_state = SessionTrend::analyze_session(&warmup_miss, now);
        assert_eq!(
            miss_state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Recovery)
        );
    }

    #[test]
    fn test_derive_recommended_level() {
        let session_plays = vec![
            SessionPlayInfo {
                rating: 160.0,
                floor: 13.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: 1000,
            },
            SessionPlayInfo {
                rating: 160.0,
                floor: 13.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: 900,
            },
        ];

        let state_climb = SessionTrendState {
            trend: SessionTrend::Climbing,
            avg_rating: 170.0,
            last_floor: 12.8,
            last_diff: Difficulty::SC,
        };
        // 13.0 + 0.3 = 13.3 -> round 13 -> "SC 13"
        assert_eq!(
            derive_recommended_level(Some(&state_climb), &session_plays, Difficulty::SC, 13.0),
            Some("SC 13".to_string())
        );

        // 손풀기 곡(SC 6.0)을 직전에 쳤더라도 주력층(SC 13.0)을 앵커로 유지하여 SC 6으로 급락하지 않음 검증!
        let warmup_session_plays = vec![
            SessionPlayInfo {
                rating: 80.0,
                floor: 6.0,
                rate: 99.9,
                diff: Difficulty::SC,
                is_max_combo: true,
                updated_at: 1200,
            },
            SessionPlayInfo {
                rating: 160.0,
                floor: 13.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: 1000,
            },
        ];
        let state_warmup = SessionTrendState {
            trend: SessionTrend::Steady,
            avg_rating: 160.0,
            last_floor: 6.0,
            last_diff: Difficulty::SC,
        };
        assert_eq!(
            derive_recommended_level(
                Some(&state_warmup),
                &warmup_session_plays,
                Difficulty::SC,
                13.0
            ),
            Some("SC 13".to_string())
        );

        let state_recovery = SessionTrendState {
            trend: SessionTrend::Recovery,
            avg_rating: 170.0,
            last_floor: 13.2,
            last_diff: Difficulty::SC,
        };
        // 13.0 - 0.3 = 12.7 -> round 13 -> "SC 13"
        assert_eq!(
            derive_recommended_level(Some(&state_recovery), &session_plays, Difficulty::SC, 13.0),
            Some("SC 13".to_string())
        );

        let state_normal = SessionTrendState {
            trend: SessionTrend::Steady,
            avg_rating: 140.0,
            last_floor: 11.0,
            last_diff: Difficulty::MX,
        };
        assert_eq!(
            derive_recommended_level(Some(&state_normal), &[], Difficulty::MX, 11.0),
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
                rank: Some(45),
            })
        );

        // Case 2: Top-50 돌파 (컷라인 직하)
        let reason_att = derive_recommend_reason(2.0, 5.8, 1.0, None, None);
        assert_eq!(
            reason_att,
            Some(RecommendReason {
                kind: RecommendReasonKind::Top50Attack,
                detail: "Top-50 컷라인 돌파 추천 타깃".to_string(),
                rank: None,
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
                rank: None,
            })
        );

        // Case 4: 방치 재도전
        let reason_retry = derive_recommend_reason(4.5, 0.0, 0.0, None, None);
        assert_eq!(
            reason_retry,
            Some(RecommendReason {
                kind: RecommendReasonKind::Retry,
                detail: "방치된 기록 경신 재도전 추천".to_string(),
                rank: None,
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
            strategy: RecommendStrategy::Classic,
        };

        let footer = recommender.floor_summary(&ctx_classic);
        assert_eq!(footer.recommended_level, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    // ============================================================
    //  Score Scale Analysis Tests
    //
    //  아래 테스트들은 각 추천 score의 범위와 최종 ranking에서의
    //  상대적 영향력을 검증한다.
    //
    //  Score 범위 요약 (의도된 설계):
    //  - retry_priority:       0.0 ~ 9.5  (99.5 - rate, rate=90.0일 때 최대)
    //  - top50_boundary_score: 0.0 ~ 6.0  (attack max) / 3.0 ~ 5.0 (defend)
    //  - session_flow_score:   0.0 ~ 4.0  (MOMENTUM_MAX_BONUS)
    //
    //  합산 최대: 9.5 + 6.0 + 4.0 = 19.5
    //
    //  의도적 설계 근거:
    //  - retry가 가장 강한 영향력 (rate 90%대 방치곡은 경신 욕구가 가장 강함)
    //  - top50은 중간 영향력 (컷라인 공략은 동기부여가 크지만 모든 곡에 해당하지 않음)
    //  - session_flow는 보조 영향력 (동급 난이도 내 미세 정렬 역할)
    // ============================================================

    #[test]
    fn test_score_scale_ranges_and_dominance() {
        let now = 1_000_000i64;

        // 1. retry_priority 범위 검증
        // rate=90.0, 2주 전 → 최대치에 근접
        let retry_max = retry_priority(90.0, Some(now - 15 * 86400), now);
        assert!(
            retry_max > 9.0 && retry_max <= 9.5,
            "retry_priority max expected 9.0~9.5, got {}",
            retry_max
        );

        // rate=99.0, 2주 전 → 격차 0.5
        let retry_small = retry_priority(99.0, Some(now - 15 * 86400), now);
        assert!(
            retry_small > 0.4 && retry_small <= 0.5,
            "retry_priority near-max rate expected 0.4~0.5, got {}",
            retry_small
        );

        // rate=95.0, 1시간 전(유예 이내) → ~0
        let retry_recent = retry_priority(95.0, Some(now - 3600), now);
        assert!(
            retry_recent < 0.1,
            "retry_priority within grace period should be ~0, got {}",
            retry_recent
        );

        // 2. top50_boundary_score 범위 검증
        let cutoff = 172.96;
        let top50_defend_max = top50_boundary_score(Some(172.96), Some(50), cutoff, 50);
        assert!(
            (top50_defend_max - 5.0).abs() < 0.01,
            "top50 defend max expected 5.0, got {}",
            top50_defend_max
        );

        let top50_attack_max = top50_boundary_score(Some(cutoff - 0.01), None, cutoff, 50);
        assert!(
            top50_attack_max > 5.9 && top50_attack_max <= 6.0,
            "top50 attack max expected ~6.0, got {}",
            top50_attack_max
        );

        // 3. session_flow_score 범위 검증
        let trend_climb = SessionTrendState {
            trend: SessionTrend::Climbing,
            avg_rating: 150.0,
            last_floor: 13.0,
            last_diff: Difficulty::SC,
        };
        // Climbing 최적 지점 (floor+0.25)
        let flow_max = session_flow_score(13.25, None, 13.0, Some(&trend_climb));
        assert!(
            (flow_max - 4.0).abs() < 0.01,
            "session_flow climbing max expected 4.0, got {}",
            flow_max
        );

        // 4. 지배력(Dominance) 분석:
        //    단일 score가 ranking을 결정할 수 있는 시나리오 검증
        //
        //    Scenario A: retry=9.5 vs top50=6.0+flow=4.0=10.0
        //    → top50+flow가 retry를 이길 수 있음 (의도: 복합 신호가 단일 신호를 넘을 수 있어야 함)
        let combo_signal = 6.0 + 4.0; // top50_max + flow_max
        assert!(
            combo_signal > retry_max,
            "combo signals should be able to outweigh retry: {} vs {}",
            combo_signal,
            retry_max
        );

        //    Scenario B: retry=5.0(적당한 갭) vs top50_defend=5.0
        //    → 동등한 영향력 (의도: 중간 갭의 방치곡과 Top50 수성은 비슷한 우선순위)
        let retry_mid = retry_priority(94.5, Some(now - 15 * 86400), now);
        assert!(
            (retry_mid - 5.0).abs() < 0.1,
            "retry mid-gap expected ~5.0, got {}",
            retry_mid
        );

        //    Scenario C: flow 단독 최대(4.0)는 retry 중간(5.0)을 넘지 못함
        //    → 의도: session flow는 같은 층 내 미세 정렬이므로 단독으로 ranking을 역전하면 안 됨
        assert!(
            flow_max < retry_mid,
            "flow alone should not dominate over mid-gap retry: {} vs {}",
            flow_max,
            retry_mid
        );
    }

    /// 서로 다른 추천 신호가 동일 곡을 가리키는 경우와 충돌하는 경우의
    /// reason 도출 검증.
    ///
    /// Extrapolation 패턴:
    /// - 실제 분포에서 retry 대상곡은 대개 rate 90~98%, floor 11~14
    /// - Top50 경계곡은 rating 170~175 근처 (cutoff ±2)
    /// - session flow 대상은 현재 floor ±0.4 범위
    #[test]
    fn test_recommend_reason_multi_signal_convergence() {
        // Case 1: 모든 신호가 동시에 높은 경우 → 가장 높은 score의 reason이 선택됨
        // retry=5.0, top50=5.5, flow=3.5 → top50이 지배 → Top50Attack
        let reason = derive_recommend_reason(5.0, 5.5, 3.5, None, Some(SessionTrend::Climbing));
        assert_eq!(reason.unwrap().kind, RecommendReasonKind::Top50Attack);

        // Case 2: retry가 지배 (방치된 고난도 곡)
        // retry=8.0, top50=3.0, flow=2.0 → retry 지배 → Retry
        let reason = derive_recommend_reason(8.0, 3.0, 2.0, None, Some(SessionTrend::Steady));
        assert_eq!(reason.unwrap().kind, RecommendReasonKind::Retry);

        // Case 3: flow가 지배 (세션 상승 중 적정 난이도 곡)
        // retry=0.5, top50=0.0, flow=3.8 → flow 지배 → Climbing
        let reason = derive_recommend_reason(0.5, 0.0, 3.8, None, Some(SessionTrend::Climbing));
        assert_eq!(reason.unwrap().kind, RecommendReasonKind::Climbing);

        // Case 4: 모든 score가 임계치(1.5) 미만 → reason 없음 (뱃지 생략)
        let reason = derive_recommend_reason(1.0, 1.2, 0.8, None, None);
        assert_eq!(reason, None);

        // Case 5: Top50 방어 + retry 동시 해당 (41~50위이면서 rate<99.5)
        // → top50_defend 우선 (순위 보존이 경신보다 급함)
        let reason = derive_recommend_reason(4.0, 4.5, 0.0, Some(45), None);
        assert_eq!(reason.unwrap().kind, RecommendReasonKind::Top50Defend);
    }

    /// 실제 사용자 기록 분포를 기반으로 한 Performance Rating 범위 검증.
    ///
    /// 실제 데이터에서 관찰되는 분포:
    /// - Floor 8~9 (중급): rate 95~99%, rating 80~120
    /// - Floor 11~13 (상급): rate 90~98%, rating 110~170
    /// - Floor 14~15 (최상급): rate 85~97%, rating 120~195
    #[test]
    fn test_performance_rating_realistic_distribution() {
        // Floor 8, rate 99.8% → 고정확도 중급 = 약 106점
        let mid_high = calculate_performance_rating(8.0, 99.8);
        assert!(
            mid_high > 100.0 && mid_high < 110.0,
            "floor 8 rate 99.8% expected 100~110, got {}",
            mid_high
        );

        // Floor 13, rate 97.0% → 상급 표준 = 약 130점
        let high_standard = calculate_performance_rating(13.0, 97.0);
        assert!(
            high_standard > 120.0 && high_standard < 140.0,
            "floor 13 rate 97% expected 120~140, got {}",
            high_standard
        );

        // Floor 15, rate 100.0% → 최대 = 200점
        let max = calculate_performance_rating(15.0, 100.0);
        assert!((max - 200.0).abs() < 0.01);

        // Floor 13, rate 90.0% → 경계 (retry 대상) = 약 69점
        let retry_target = calculate_performance_rating(13.0, 90.0);
        assert!(
            retry_target > 60.0 && retry_target < 80.0,
            "floor 13 rate 90% expected 60~80, got {}",
            retry_target
        );

        // Floor 0 또는 rate 0 → 0점 (미플레이)
        assert_eq!(calculate_performance_rating(0.0, 99.0), 0.0);
        assert_eq!(calculate_performance_rating(13.0, 0.0), 0.0);
    }

    /// 세션 내 다양한 플레이 패턴에서의 trend 분석 검증.
    ///
    /// Extrapolation:
    /// - 실제 세션에서 3~5판 연속 플레이 후 상승/하락 판별
    /// - 손풀기(warmup)→주력곡→고난도 도전 순서가 일반적
    /// - 세션 간 4시간 이상 간격이면 세션 만료
    #[test]
    fn test_session_trend_various_patterns() {
        let now = 1_000_000i64;

        // Pattern 1: 점진적 상승 (warmup → 주력 → 고난도 성공)
        // 실제 패턴: 8층 손풀기 → 12층 안정 → 13층 도전 성공
        let gradual_climb = vec![
            SessionPlayInfo {
                rating: 170.0, // 13층 99.0%
                floor: 13.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 155.0, // 12층 99.0%
                floor: 12.0,
                rate: 99.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 105.0, // 8층 99.8% (warmup)
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
        // 세션 평균은 손풀기 제외: (170 + 155) / 2 = 162.5
        let avg = state.as_ref().unwrap().avg_rating;
        assert!(
            (avg - 162.5).abs() < 0.1,
            "avg_rating expected 162.5, got {}",
            avg
        );

        // Pattern 2: 하락세 (주력 → 실패 → 저조)
        // 실제 패턴: 13층 98% → 14층 88% → 세션 회복 필요
        let declining = vec![
            SessionPlayInfo {
                rating: 102.0, // 14층 88% (대실패)
                floor: 14.0,
                rate: 88.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 300,
            },
            SessionPlayInfo {
                rating: 160.0, // 13층 98%
                floor: 13.0,
                rate: 98.0,
                diff: Difficulty::SC,
                is_max_combo: false,
                updated_at: now - 600,
            },
            SessionPlayInfo {
                rating: 155.0, // 12층 99%
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

        // Pattern 3: 안정 순항 (동일 층 반복 플레이)
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

        // Pattern 4: 맥콤으로 인한 Climbing 인정
        let mc_climb = vec![
            SessionPlayInfo {
                rating: 162.0,
                floor: 13.0,
                rate: 98.5,
                diff: Difficulty::SC,
                is_max_combo: true, // 맥콤!
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
        // delta = 162 - 161 = 1.0 < 3.0 이지만, is_max_combo && delta >= 0.0 → Climbing
        assert_eq!(
            state.as_ref().map(|s| s.trend),
            Some(SessionTrend::Climbing)
        );
    }

    /// 실제 Candidate Ranking Pipeline 통합 테스트 (End-to-End Ranking Verification).
    ///
    /// 검증 목적:
    /// - 여러 신호(Retry, Top-50, Session Flow)가 실제 DB/V-Archive/PlayEvents와 결합되었을 때
    ///   의도된 우선순위 계층(Hierarchy)에 따라 후보들이 올바르게 정렬되는지 직접 검증한다.
    ///
    /// 검증 계층:
    /// 1. Top-50 돌파 + Session Flow 복합 신호 (Score ~10.4) → Strong Retry(8.5)를 역전하여 1위
    /// 2. 단독 Strong Retry (Score 8.5) → 단독으로도 강력한 우선순위로 2위
    /// 3. 중간 Retry (Score 4.5) → 단독 Session Flow(4.0)보다 높은 우선순위로 3위
    /// 4. 단독 Session Flow (Score 4.0) → 4위
    /// 5. 약한 신호 플레이곡 (Score 0.0) → 5위
    /// 6. 미플레이 곡 → 플레이 완료곡들 뒤에 정렬 (6위)
    #[test]
    fn test_candidate_ranking_integration_multi_signal_hierarchy() {
        let now = LocalFloorRecommender::now_unix();

        // 1. VArchiveDB 구성 (후보 곡 0~6 및 세션 플레이 곡 997~999)
        let mut vdb = VArchiveDB::new();
        let make_song = |id: i32, name: &str, floor_str: Option<&str>| {
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
                "dlcCode": "pack",
                "patterns": patterns
            })
        };

        vdb.songs = vec![
            serde_json::from_value(make_song(0, "Current Song", Some("13.0"))).unwrap(),
            serde_json::from_value(make_song(1, "Combo (Top50+Flow)", Some("13.25"))).unwrap(),
            serde_json::from_value(make_song(2, "Strong Retry", Some("13.0"))).unwrap(),
            serde_json::from_value(make_song(3, "Mid Retry", Some("13.0"))).unwrap(),
            serde_json::from_value(make_song(4, "Pure Flow", Some("13.25"))).unwrap(),
            serde_json::from_value(make_song(5, "Weak Signal", Some("13.0"))).unwrap(),
            serde_json::from_value(make_song(6, "Unplayed Song", Some("13.0"))).unwrap(),
            // 세션 기록 곡들 (후보 범위 12.5~13.5 밖으로 설정하여 추천 목록 오염 방지)
            serde_json::from_value(make_song(999, "Session Core 11", Some("11.0"))).unwrap(),
            serde_json::from_value(make_song(998, "Session Core 10", Some("10.0"))).unwrap(),
            serde_json::from_value(make_song(997, "Session Warmup 7", Some("7.0"))).unwrap(),
        ];

        // 2. DB 구성
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("rec_ranking_integration.db");
        let steam_id = "76561198000000001";
        let mut db = crate::store::record_db::RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());

        let conn = db.open_conn().unwrap();

        // 2-1. V-Archive Top50 (51개 데이터 추가, Cutoff = 173.0)
        // 2-1. V-Archive Top50 (49개 기준 곡 + Song 4 = 총 50개 Top50, Cutoff = 174.0)
        for i in 101..=149 {
            let rating = 174.0 + ((i - 101) as f64 * 0.4); // 174.0 ~ 193.2
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
            ).unwrap();
        }

        // Song 4: rating 188.0 (Top-50 상위 15위권 안정 순위 -> top50_score = 0.0)
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

        // Song 1: rating 173.8 (Cutoff 174.0 직하 delta=0.2 돌파 타깃 -> top50_attack = 5.4)
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

        // 2-2. play_events 구성 (Climbing 세션: 손풀기 8층 -> 12층 -> 13층 성공)
        let session_events = [
            (999, "SC", 99.5, now - 300), // 13층 99.5% (Rating ~168.0)
            (998, "SC", 98.0, now - 600), // 12층 98.0% (Rating ~135.0)
            (997, "SC", 99.8, now - 900), // 8층 99.8% (Warmup, Rating ~105.0)
        ];
        for (sid, diff, rate, ts) in session_events {
            conn.execute(
                "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![steam_id, sid.to_string(), "4B", diff, rate, 0, ts],
            ).unwrap();
        }

        // 2-3. records 테이블에 각 후보곡의 과거 기록 삽입
        let long_ago = now - 15 * 86400; // 15일 전 (recency_weight = 1.0)
        let older = now - 5 * 3600; // 5시간 전 (세션 윈도우 4시간 밖, retry는 99.4%라 0.1 이하)
        let records = [
            (1, 98.5, long_ago), // Song 1: retry = 1.0, top50 ~ 5.4, flow = 4.0 -> Total ~ 10.4
            (2, 91.0, long_ago), // Song 2: retry = 8.5, top50 = 0, flow = 0 -> Total = 8.5
            (3, 95.0, long_ago), // Song 3: retry = 4.5, top50 = 0, flow = 0 -> Total = 4.5
            (4, 99.5, long_ago), // Song 4: retry = 0.0, top50 = 0, flow = 4.0 -> Total = 4.0
            (5, 99.4, older),    // Song 5: retry ~ 0.0, top50 = 0, flow = 0.0 -> Total ~ 0.0
                                 // Song 6은 records 없음 (미플레이)
        ];
        for (sid, rate, ts) in records {
            conn.execute(
                "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![steam_id, sid.to_string(), "4B", "SC", rate, 0, ts],
            ).unwrap();
        }
        drop(conn);

        // 3. Recommender 실행
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

        // 4. Ranking 순위 및 사유(Reason) 검증
        assert_eq!(
            entries.len(),
            6,
            "Total 6 candidates (excluding current song 0)"
        );

        // 1위: Song 1 (Top50 + Flow 복합 신호가 Strong Retry를 역전)
        assert_eq!(entries[0].song_id, 1);
        assert_eq!(
            entries[0].reason.as_ref().map(|r| r.kind),
            Some(RecommendReasonKind::Top50Attack)
        );

        // 2위: Song 2 (강한 Retry 신호 단독으로 2위 차지)
        assert_eq!(entries[1].song_id, 2);
        assert_eq!(
            entries[1].reason.as_ref().map(|r| r.kind),
            Some(RecommendReasonKind::Retry)
        );

        // 3위: Song 3 (중간 Retry가 Pure Flow보다 우위)
        assert_eq!(entries[2].song_id, 3);
        assert_eq!(
            entries[2].reason.as_ref().map(|r| r.kind),
            Some(RecommendReasonKind::Retry)
        );

        // 4위: Song 4 (Pure Session Flow)
        assert_eq!(entries[3].song_id, 4);
        assert_eq!(
            entries[3].reason.as_ref().map(|r| r.kind),
            Some(RecommendReasonKind::Climbing)
        );

        // 5위: Song 5 (약한 신호 플레이곡 - score 0)
        assert_eq!(entries[4].song_id, 5);
        assert_eq!(entries[4].reason, None);

        // 6위: Song 6 (미플레이 곡 - 플레이 완료곡들 뒤에 배치)
        assert_eq!(entries[5].song_id, 6);
        assert_eq!(entries[5].rate, None);
        assert_eq!(entries[5].reason, None);
    }
}
