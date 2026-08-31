use overmax_core::{Difficulty, Mode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::community::client::Song;
use crate::service::record_manager::RecordManager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
    /// 미플레이 첫 클리어 도전 추천
    Unplayed,
}

impl RecommendReasonKind {
    pub fn badge_label(&self) -> &'static str {
        match self {
            Self::Top50Attack => "TOP",
            Self::Top50Defend => "DEF",
            Self::Climbing => "UP",
            Self::Recovery => "REST",
            Self::Retry => "TRY",
            Self::Unplayed => "CLR",
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
    /// 와이어 포맷은 공통 어휘(`mode`)를 사용 — `overmax-recommend/1` 및
    /// `overmax-ipc/1` 이벤트와 동일한 키 (docs/guides/recommend-provider-protocol.md §4)
    #[serde(rename = "mode")]
    pub button_mode: Mode,
    /// 와이어 포맷은 공통 어휘(`diff`)를 사용
    #[serde(rename = "diff")]
    pub difficulty: Difficulty,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    pub target_rate: f64,
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
    pub button_mode: Mode,
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

pub(crate) struct CandidateSearchParams {
    pub target_song_id: i32,
    pub target_mode: Mode,
    pub target_diff: Difficulty,
    pub ref_floor: f64,
    pub use_official: bool,
    pub ref_diff_grp: &'static str,
    pub floor_range: f64,
    pub same_mode_only: bool,
}

pub(crate) struct RawCandidate<'a> {
    pub song_id: i32,
    pub song: &'a Song,
    pub mode: Mode,
    pub diff: Difficulty,
    pub level: Option<u32>,
    pub floor: f64,
    pub floor_name: Option<Arc<str>>,
    pub rate: Option<f64>,
    pub is_max_combo: bool,
    pub updated_at: Option<i64>,
    pub varchive_rating: Option<f64>,
    pub reason: Option<RecommendReason>,
}

impl<'a> RawCandidate<'a> {
    pub fn is_played(&self) -> bool {
        self.rate.is_some()
    }

    pub fn into_entry(self) -> RecommendEntry {
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

pub(crate) struct StrategySortParams<'a, 'b, F>
where
    F: Fn(i32, Mode, Difficulty) -> f64,
{
    pub candidates: &'b mut Vec<RawCandidate<'a>>,
    pub rdb: &'b RecordManager,
    pub find_floor: F,
    pub button_mode: Mode,
    pub ref_floor: f64,
    pub max_results: usize,
    pub target_rate: f64,
    pub now_unix: i64,
}

pub(crate) struct StrategyFooterParams<'a, F>
where
    F: Fn(i32, Mode, Difficulty) -> f64,
{
    pub rdb: &'a RecordManager,
    pub find_floor: F,
    pub button_mode: Mode,
    pub current_diff: Difficulty,
    pub target_rate: f64,
    pub now_unix: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 공통 어휘 계약 고정: `RecommendEntry` 직렬화는 `overmax-recommend/1` 및
    /// `overmax-ipc/1`과 동일한 `song_id`/`mode`/`diff` 키를 사용해야 한다.
    #[test]
    fn recommend_entry_serializes_shared_wire_vocabulary() {
        let entry = RecommendEntry {
            song_id: 123,
            song_name: "Test Song".into(),
            composer: "Composer".into(),
            button_mode: Mode::B5,
            difficulty: Difficulty::SC,
            level: None,
            floor: None,
            floor_name: None,
            rate: None,
            is_max_combo: false,
            score: Some(0.87),
            reason: None,
        };

        let v = serde_json::to_value(&entry).expect("serialize");
        assert_eq!(v["song_id"], 123);
        assert_eq!(v["mode"], "5B");
        assert_eq!(v["diff"], "SC");
        assert!(v.get("button_mode").is_none(), "internal field name leaked");
        assert!(v.get("difficulty").is_none(), "internal field name leaked");
    }

    #[test]
    fn reason_kind_serializes_snake_case() {
        let reason = RecommendReason {
            kind: RecommendReasonKind::Top50Attack,
            detail: "cut-line".into(),
            rank: None,
        };
        let v = serde_json::to_value(&reason).expect("serialize");
        assert_eq!(v["kind"], "top50_attack");
    }
}
