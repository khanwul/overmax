use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};
use std::sync::{Arc, Mutex};

use overmax_core::{Difficulty, Mode, RecordKey};

use crate::community::client::VArchiveDB;
use crate::service::record_manager::{RecordManager, RecordSource};

use super::scoring::is_base_bundle_dlc;
use super::types::{
    CandidateSearchParams, FloorCacheKey, FloorRateSummary, LocalRecommendFooter, RawCandidate,
    RecommendBundle, RecommendContext, RecommendationSource, SourceStatus, StrategyFooterParams,
    StrategySortParams,
};

pub struct LocalFloorRecommender {
    pub(crate) vdb: Arc<VArchiveDB>,
    pub(crate) rdb: Arc<RecordManager>,
    floor_rate_cache: Mutex<HashMap<FloorCacheKey, FloorRateSummary>>,
    floor_rate_dirty: Mutex<HashMap<FloorCacheKey, bool>>,
    floor_patterns: Mutex<HashMap<FloorCacheKey, Vec<RecordKey>>>,
    record_to_floor_key: Mutex<HashMap<RecordKey, FloorCacheKey>>,
    cache_index_ready: AtomicBool,
}

impl LocalFloorRecommender {
    pub(crate) fn now_unix() -> i64 {
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

    pub(crate) fn parse_floor_value(floor_name: Option<&Arc<str>>) -> Option<f64> {
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

    /// 기본 5개 번들 팩 및 사용자의 플레이 이력에 존재하는 DLC 코드 집합을 추출한다.
    pub fn get_owned_dlc_set(&self) -> std::collections::HashSet<String> {
        let mut owned = std::collections::HashSet::new();
        // 기본 5개 번들 팩은 무조건 보유 인정
        owned.insert("r".to_string());
        owned.insert("rv".to_string());
        owned.insert("p1".to_string());
        owned.insert("p2".to_string());
        owned.insert("gg".to_string());

        if self.rdb.is_ready() {
            let recorded_ids = self.rdb.get_all_recorded_song_ids();
            for song in &self.vdb.songs {
                if let Ok(sid) = song.title.parse::<i32>() {
                    if recorded_ids.contains(&sid) {
                        let dlc_norm = song
                            .dlc_code
                            .to_lowercase()
                            .replace(char::is_whitespace, "");
                        if !dlc_norm.is_empty() {
                            owned.insert(dlc_norm);
                        }
                    }
                }
            }
        }

        owned
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

    pub fn recommend(&self, ctx: &RecommendContext) -> RecommendBundle {
        <Self as RecommendationSource>::recommend(self, ctx)
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

        // 보유 DLC 팩이 아닌 미플레이 곡은 추천 풀에서 제외 (기록이 있는 곡은 100% 보존)
        let owned_dlcs = self.get_owned_dlc_set();
        candidates.retain(|c| {
            if c.is_played() {
                return true;
            }
            let dlc_norm = c
                .song
                .dlc_code
                .to_lowercase()
                .replace(char::is_whitespace, "");
            is_base_bundle_dlc(&dlc_norm) || owned_dlcs.contains(&dlc_norm)
        });

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
