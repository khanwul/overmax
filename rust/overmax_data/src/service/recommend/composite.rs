use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use overmax_core::{Difficulty, Mode};
use serde::Deserialize;

use crate::community::client::VArchiveDB;
use crate::service::recommend_provider_fetch::RECOMMEND_PROTOCOL_ID;
use crate::service::record_manager::{RecordManager, RecordSource};

use super::local::LocalFloorRecommender;
use super::types::{
    RecommendBundle, RecommendContext, RecommendEntry, RecommendPanel, RecommendResult,
    RecommendStrategy, RecommendationSource, SourceStatus, VaryDim,
};

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

    pub fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle {
        <Self as RecommendationSource>::recommend(self, ctx)
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

        if payload.protocol != RECOMMEND_PROTOCOL_ID || payload.entries.is_empty() {
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
            target_rate: 99.0,
        };
        self.recommend_panel(&ctx).as_legacy_result()
    }
}

pub type Recommender = CompositeRecommender;
