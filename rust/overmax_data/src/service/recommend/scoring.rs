use super::types::{RecommendReason, RecommendReasonKind};
use overmax_core::Difficulty;

/// 기본 번들 팩(RESPECT 본편, Portable 1/2, Guilty Gear)인지 확인한다.
pub fn is_base_bundle_dlc(dlc_code: &str) -> bool {
    let dlc = dlc_code.to_lowercase().replace(char::is_whitespace, "");
    matches!(
        dlc.as_str(),
        "r" | "rv"
            | "respect"
            | "respect/v"
            | "p1"
            | "portable1"
            | "p2"
            | "portable2"
            | "gg"
            | "guiltygear"
            | "guilty gear"
    )
}

/// 미플레이 신규 도전 추천 기본 최대 보너스
pub(crate) const UNPLAYED_MAX_BONUS: f64 = 5.5;

/// 미플레이 곡의 난이도 적합도와 세션 모멘텀을 기반으로 추천 점수를 산출한다.
pub(crate) fn unplayed_challenge_score(
    cand_floor: f64,
    ref_floor: f64,
    trend_state: Option<&SessionTrendState>,
) -> f64 {
    let delta = cand_floor - ref_floor;

    let Some(state) = trend_state else {
        // 세션 데이터가 없을 때: 현재 층과 가까울수록 높은 기본 점수 (최대 5.0)
        let dist = delta.abs();
        if dist <= 0.3 {
            return (1.0 - (dist / 0.3)) * (UNPLAYED_MAX_BONUS - 0.5);
        }
        return 0.0;
    };

    match state.trend {
        SessionTrend::Climbing => {
            // 컨디션 쾌조: 동급(0.0) ~ 상위 난이도(+0.25) 도전에 최고 점수 (5.5점)
            if (0.0..=0.35).contains(&delta) {
                let center = 0.15;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.20)).max(0.0) * UNPLAYED_MAX_BONUS
            } else {
                0.0
            }
        }
        SessionTrend::Steady => {
            // 안정 순항: 현재 선곡 난이도와 일치하는(±0.15) 미플레이 곡에 5.0점
            if delta.abs() <= 0.15 {
                (1.0 - (delta.abs() / 0.15)) * (UNPLAYED_MAX_BONUS - 0.5)
            } else {
                0.0
            }
        }
        SessionTrend::Recovery => {
            // 컨디션 저조: 살짝 낮은 층(-0.2)의 부담 없는 미플레이 곡에 4.5점
            if (-0.4..=0.0).contains(&delta) {
                let center = -0.2;
                let dist = (delta - center).abs();
                (1.0 - (dist / 0.2)).max(0.0) * (UNPLAYED_MAX_BONUS - 1.0)
            } else {
                0.0
            }
        }
    }
}

/// 재도전 목표 정확도. 100.0(퍼펙트) 대신 현실적인 재도전 유인 지점으로 설정.
pub(crate) const RETRY_TARGET_RATE: f64 = 99.5;
/// 이 시간 이내에 플레이한 곡은 재도전 우선순위에서 사실상 제외(당일 반복 추천 억제).
pub(crate) const RETRY_RECENCY_GRACE_HOURS: f64 = 12.0;
/// 이 일수가 지나면 격차(rate_gap)를 100% 반영(가중치 1.0로 포화).
pub(crate) const RETRY_RECENCY_RAMP_DAYS: f64 = 14.0;

/// 재도전 우선순위 = (목표 정확도 - 현재 rate) * 최근성 가중치.
/// `updated_at`이 없으면(레거시 데이터 등) 최근성 가중 없이 격차만 반환한다.
pub(crate) fn retry_priority(rate: f64, updated_at: Option<i64>, now_unix: i64) -> f64 {
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
pub(crate) const TOP50_DEFENSE_RANK_START: usize = 41;
/// 컷라인 기준 돌파 후보 탐색 델타 (컷라인 - 2.0 이내)
pub(crate) const TOP50_ATTACK_RATING_DELTA: f64 = 2.0;
/// 41~50위 수성 보너스 최대치
pub(crate) const TOP50_DEFENSE_BONUS: f64 = 5.0;
/// 51위 이하 컷라인 최근접 돌파 보너스 최대치
pub(crate) const TOP50_ATTACK_MAX_BONUS: f64 = 6.0;

/// Top-50 경계 추천 점수 계산 순수 함수.
/// - rank가 Some(41..=50): 컷라인 방어 타깃 (순위가 50위에 가까울수록 높은 보너스 3.0 ~ 5.0)
/// - rank가 None이거나 51위 이상이고 rating이 cutoff_rating - 2.0 이상: 컷라인 돌파 타깃 (0.0 ~ 6.0)
/// - Top-50 데이터가 부족하거나(50개 미만) 무관한 구간: 0.0
pub(crate) fn top50_boundary_score(
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
        if rating <= cutoff_rating && rating >= (cutoff_rating - TOP50_ATTACK_RATING_DELTA) {
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
pub(crate) const MOMENTUM_CLIMB_RATING_DELTA: f64 = 3.0;
/// 상대 저조/회복 판별 레이팅 편차 기준치 (개인 세션 평균 대비 -6.0점 미만)
pub(crate) const MOMENTUM_RECOVERY_RATING_DELTA: f64 = -6.0;
/// 세션 모멘텀 추천 최대 보너스
pub(crate) const MOMENTUM_MAX_BONUS: f64 = 4.0;
/// 세션 만료 유효 윈도우 시간 (4시간)
pub(crate) const SESSION_IDLE_TIMEOUT_HOURS: f64 = 4.0;

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
pub(crate) fn session_flow_score(
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

/// Microsoft TrueSkill / TrueMatch 기반의 단일 난이도 계열 실력 정규분포 N(μ, σ²).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GaussianSkill {
    /// 실력 중심값 (평균 Floor, 예: 11.2)
    pub mu: f64,
    /// 실력 분산/표준편차 (Floor 스케일, 기본 0.8 ~ 2.5)
    pub sigma: f64,
    /// 표본 수
    pub sample_count: usize,
}

impl GaussianSkill {
    pub const DEFAULT_SC: Self = Self {
        mu: 5.0,
        sigma: 1.5,
        sample_count: 0,
    };

    pub const DEFAULT_PAD: Self = Self {
        mu: 8.0,
        sigma: 2.0,
        sample_count: 0,
    };

    pub fn new(mu: f64, sigma: f64, sample_count: usize) -> Self {
        Self {
            mu: if mu.is_finite() && mu > 0.0 { mu } else { 5.0 },
            sigma: if sigma.is_finite() {
                sigma.clamp(0.8, 2.5)
            } else {
                1.5
            },
            sample_count,
        }
    }

    /// 주어진 난이도(Floor)가 회복/손풀기 안전 구간에 해당하는지 확인 (Floor <= mu - 0.8 * sigma)
    pub fn is_recovery_zone(&self, floor: f64) -> bool {
        floor <= (self.mu - 0.8 * self.sigma)
    }

    /// 주어진 난이도(Floor)가 상위 도전 구간에 해당하는지 확인 (mu <= Floor <= mu + 1.2 * sigma)
    pub fn is_climbing_zone(&self, floor: f64) -> bool {
        floor >= self.mu && floor <= (self.mu + 1.2 * self.sigma)
    }

    /// 주어진 난이도(Floor)가 주력 재도전 적정 범위에 해당하는지 확인 (|Floor - mu| <= 1.0 * sigma)
    pub fn is_core_zone(&self, floor: f64) -> bool {
        (floor - self.mu).abs() <= 1.0 * self.sigma
    }
}

/// 버튼 모드별 SC 및 일반(Pad) 2-Track 실력 프로필
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SkillProfile {
    pub sc: GaussianSkill,
    pub pad: GaussianSkill,
}

impl SkillProfile {
    pub fn for_diff(&self, diff: Difficulty) -> &GaussianSkill {
        if diff.is_sc() {
            &self.sc
        } else {
            &self.pad
        }
    }
}

/// Top 50 요약 정보로부터 SC 및 일반(Pad) 2-Track 실력 프로필을 산출한다.
/// - 샘플이 부족할 경우 Cross-Track Fallback을 통해 상호 보완한다.
pub(crate) fn derive_skill_profile<F>(
    top50: &crate::store::record_db::VArchiveTop50Summary,
    find_floor: &F,
    button_mode: overmax_core::Mode,
) -> SkillProfile
where
    F: Fn(i32, overmax_core::Mode, Difficulty) -> f64,
{
    let mut sc_floors: Vec<f64> = Vec::new();
    let mut pad_floors: Vec<f64> = Vec::new();

    for &(sid, m, d) in top50.rank_map.keys() {
        if m == button_mode {
            let f = find_floor(sid, m, d);
            if f > 0.0 {
                if d.is_sc() {
                    sc_floors.push(f);
                } else {
                    pad_floors.push(f);
                }
            }
        }
    }

    let compute_gaussian = |floors: &[f64], default_skill: GaussianSkill| -> GaussianSkill {
        if floors.len() < 3 {
            if floors.is_empty() {
                return default_skill;
            }
            let mean = floors.iter().sum::<f64>() / floors.len() as f64;
            return GaussianSkill::new(mean, default_skill.sigma, floors.len());
        }

        let mean = floors.iter().sum::<f64>() / floors.len() as f64;
        let variance = floors
            .iter()
            .map(|&f| {
                let diff = f - mean;
                diff * diff
            })
            .sum::<f64>()
            / floors.len() as f64;
        let std_dev = variance.sqrt();
        GaussianSkill::new(mean, std_dev, floors.len())
    };

    let mut sc_skill = compute_gaussian(&sc_floors, GaussianSkill::DEFAULT_SC);
    let mut pad_skill = compute_gaussian(&pad_floors, GaussianSkill::DEFAULT_PAD);

    // Cross-Track Fallback
    if sc_skill.sample_count >= 3 && pad_skill.sample_count < 3 {
        let pad_mu = (sc_skill.mu + 1.0).clamp(1.0, 15.0);
        pad_skill = GaussianSkill::new(pad_mu, sc_skill.sigma, pad_floors.len());
    } else if pad_skill.sample_count >= 3 && sc_skill.sample_count < 3 {
        let sc_mu = (pad_skill.mu - 1.0).clamp(1.0, 15.0);
        sc_skill = GaussianSkill::new(sc_mu, pad_skill.sigma, sc_floors.len());
    }

    SkillProfile {
        sc: sc_skill,
        pad: pad_skill,
    }
}

/// V-Archive Top 50 곡들 중 현재 난이도 계열(SC vs 일반)에 해당하는 패턴들의 중앙값(Median Floor)을 기본 실력대 앵커로 산출한다.
/// 최상위 한계곡(Peak)에 편향되지 않고 플레이어의 주력 난이도 허리를 가장 정직하게 대변한다.
#[cfg(test)]
pub(crate) fn derive_top50_base_floor<F>(
    top50: &crate::store::record_db::VArchiveTop50Summary,
    find_floor: &F,
    button_mode: overmax_core::Mode,
    is_sc: bool,
) -> Option<f64>
where
    F: Fn(i32, overmax_core::Mode, Difficulty) -> f64,
{
    let mut floors: Vec<f64> = top50
        .rank_map
        .keys()
        .filter(|&(_, m, d)| *m == button_mode && (d.is_sc() == is_sc))
        .map(|&(sid, m, d)| find_floor(sid, m, d))
        .filter(|&f| f > 0.0)
        .collect();

    if floors.is_empty() {
        return None;
    }

    floors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let len = floors.len();
    let median = if len % 2 == 1 {
        floors[len / 2]
    } else {
        (floors[(len / 2) - 1] + floors[len / 2]) / 2.0
    };

    Some(median)
}

/// SkillProfile 기반 장기 실력대 및 최근 세션 모멘텀을 결합하여 인게임 공식 레벨 라벨(예: "SC 13", "12")을 도출한다.
/// - 선곡창 커서(현재 곡)에 반응하지 않고 플레이어의 실력/세션 상태만으로 결정된다.
/// - SC 및 일반(Pad) 2-Track 실력 프로필을 기반으로 하여, 패턴 변경 시에도 일관된 기준을 유지한다.
/// - 세션 주력곡 플레이가 누적됨에 따라 세션 데이터를 점진적으로(스근하게) 블렌딩한다.
pub(crate) fn derive_recommended_level(
    trend_state: Option<&SessionTrendState>,
    session_plays: &[SessionPlayInfo],
    current_diff: Difficulty,
    skill_profile: &SkillProfile,
) -> Option<String> {
    let is_sc = current_diff.is_sc();
    let skill = skill_profile.for_diff(current_diff);

    // 1. 현재 탭 난이도 계열(SC vs 일반)과 일치하는 유효 세션 플레이들
    let same_diff_plays: Vec<&SessionPlayInfo> = session_plays
        .iter()
        .filter(|p| p.diff.is_sc() == is_sc && p.floor > 0.0)
        .collect();

    // 2. 세션 내 최고 난이도 (Session Peak Floor)
    let session_peak = same_diff_plays
        .iter()
        .map(|p| p.floor)
        .fold(0.0f64, |acc, f| acc.max(f));

    // 3. 글로벌 앵커 난이도 (스킬 프로필의 표본이 있으면 mu 우선, 없으면 세션 최고 난이도)
    let anchor_floor = if skill.sample_count > 0 {
        skill.mu
    } else if session_peak > 0.0 {
        session_peak
    } else {
        return None;
    };

    // 4. 세션 플레이 분석 및 손풀기 가드 & 점진적 블렌딩
    const WARMUP_FLOOR_GAP: f64 = 2.5;

    // 앵커 대비 2.5 미만 차이의 주력 플레이들
    let core_session_plays: Vec<&&SessionPlayInfo> = same_diff_plays
        .iter()
        .filter(|p| anchor_floor - p.floor < WARMUP_FLOOR_GAP)
        .collect();

    let base_floor = if core_session_plays.is_empty() {
        // 세션에 주력 플레이가 아직 없는 경우 (손풀기만 쳤거나 0판):
        // 앵커를 그대로 유지하여 저난도로 곤두박질치는 것을 방어!
        anchor_floor
    } else {
        // 세션 주력곡들의 중앙값과 직전 주력곡의 조화로 세션 대표 난이도 산출
        let mut core_floors: Vec<f64> = core_session_plays.iter().map(|p| p.floor).collect();
        core_floors.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let session_median = core_floors[core_floors.len() / 2];
        let last_core_floor = core_session_plays.first().unwrap().floor;
        let session_floor = (session_median + last_core_floor) / 2.0;

        let core_count = core_session_plays.len();
        if skill.sample_count > 0 {
            // 주력 플레이 수에 따라 세션 데이터를 스근하게 블렌딩
            // 1판: 25% 세션, 2판: 50% 세션, 3판: 75% 세션, 4판 이상: 100% 세션
            let session_weight = (core_count as f64 / 4.0).min(1.0);
            (1.0 - session_weight) * anchor_floor + session_weight * session_floor
        } else {
            session_floor
        }
    };

    // 5. 컨디션 모멘텀 오프셋 적용
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
/// - 실력 분포(GaussianSkill)를 기반으로 도메인 가드를 적용하여 비정상적인 뱃지(고난도 REST 등) 부여를 원천 차단한다.
pub(crate) fn derive_recommend_reason(
    retry_score: f64,
    top50_score: f64,
    flow_score: f64,
    varchive_rank: Option<usize>,
    trend: Option<SessionTrend>,
    cand_floor: f64,
    skill: Option<&GaussianSkill>,
) -> Option<RecommendReason> {
    const REASON_THRESHOLD: f64 = 1.5;

    // 1. Top-50 수성 / 돌파 (레인 A) - 절대적 1순위
    if top50_score >= REASON_THRESHOLD && top50_score >= retry_score && top50_score >= flow_score {
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

    // 2. 세션 모멘텀 플로우 (레인 D) - 실력 분포 도메인 가드 적용
    if flow_score >= REASON_THRESHOLD && flow_score >= retry_score {
        match trend {
            Some(SessionTrend::Climbing) => {
                // Climbing zone (mu <= Floor <= mu + 1.2*sigma) 가드
                let is_valid = skill.map_or(true, |s| {
                    s.is_climbing_zone(cand_floor) || cand_floor >= s.mu
                });
                if is_valid {
                    return Some(RecommendReason {
                        kind: RecommendReasonKind::Climbing,
                        detail: "세션 상승 모멘텀 상위 난이도 도전".to_string(),
                        rank: None,
                    });
                }
            }
            Some(SessionTrend::Recovery) => {
                // Recovery zone (Floor <= mu - 0.8*sigma) 필수 가드!
                // 고난도(Floor > mu - 0.8*sigma)에는 절대로 REST 뱃지를 달지 않는다!
                let is_valid = skill.map_or(true, |s| s.is_recovery_zone(cand_floor));
                if is_valid {
                    return Some(RecommendReason {
                        kind: RecommendReasonKind::Recovery,
                        detail: "세션 회복/손풀기 적정 난이도".to_string(),
                        rank: None,
                    });
                }
                // 만약 저난도가 아니라면 Recovery 뱃지 대신 아래 Retry로 폴백 시도
            }
            _ => {}
        }
    }

    // 3. 방치된 기록 재도전 (레인 B)
    if retry_score >= REASON_THRESHOLD {
        return Some(RecommendReason {
            kind: RecommendReasonKind::Retry,
            detail: "방치된 기록 경신 재도전 추천".to_string(),
            rank: None,
        });
    }

    None
}
