use crate::store::record_db::{RecentRecordEntry, RecordDB, VArchiveTop50Summary};
use overmax_core::{Difficulty, Mode, RecordKey, RecordValue, VerifiedPlayEvent};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

pub trait RecordSource {
    fn is_ready(&self) -> bool;
    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue>;
}

pub struct RecordManager {
    record_db: Arc<RecordDB>,
    varchive_cache: Mutex<HashMap<RecordKey, RecordValue>>,
    data_revision: AtomicU64,
    dirty_record_keys: Mutex<HashSet<RecordKey>>,
    full_dirty: AtomicBool,
}

impl RecordManager {
    pub fn new(record_db: Arc<RecordDB>) -> Self {
        Self {
            record_db,
            varchive_cache: Mutex::new(HashMap::new()),
            data_revision: AtomicU64::new(0),
            dirty_record_keys: Mutex::new(HashSet::new()),
            full_dirty: AtomicBool::new(true),
        }
    }

    /// VerifiedPlayEvent를 수신하여 기록을 갱신하고, 신규/개선 여부를 반환합니다.
    pub fn handle_verified_play(&self, event: &VerifiedPlayEvent) -> bool {
        let key = event.record_key();
        if event.is_result_screen {
            let now_unix = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            self.record_db.insert_play_event(event, now_unix);
        }

        if !event.is_result_screen && event.rate <= 0.0 {
            // 선곡 화면에서 미플레이(0.0%)로 확인된 경우: 과거 오기록이 있다면 삭제하여 미플레이로 교정
            self.delete(key.0, key.1, key.2)
        } else {
            self.upsert(
                key.0,
                key.1,
                key.2,
                event.rate,
                event.is_max_combo,
                event.is_result_screen,
            )
        }
    }

    pub fn refresh(&self) {
        let steam_id = self.record_db.get_steam_id();
        let cache = self
            .record_db
            .load_varchive_records(&steam_id)
            .unwrap_or_default();
        if let Ok(mut guard) = self.varchive_cache.lock() {
            *guard = cache;
        }
        self.full_dirty.store(true, Ordering::SeqCst);
        if let Ok(mut guard) = self.dirty_record_keys.lock() {
            guard.clear();
        }
        self.data_revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn upsert_varchive_record(&self, key: RecordKey, value: RecordValue) {
        if let Ok(mut guard) = self.varchive_cache.lock() {
            guard.insert(key, value);
        }
        if let Ok(mut guard) = self.dirty_record_keys.lock() {
            guard.insert(key);
        }
        self.data_revision.fetch_add(1, Ordering::SeqCst);
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let result = self.record_db.set_steam_id(steam_id);
        if result.0 {
            self.refresh();
        }
        result
    }

    pub fn upsert(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
        rate: f32,
        is_max_combo: bool,
        only_if_improved: bool,
    ) -> bool {
        if self.record_db.upsert(
            song_id,
            button_mode,
            difficulty,
            rate as f64,
            is_max_combo,
            only_if_improved,
        ) {
            if let Ok(mut guard) = self.dirty_record_keys.lock() {
                guard.insert((song_id, button_mode, difficulty));
            }
            self.data_revision.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn delete(&self, song_id: i32, button_mode: Mode, difficulty: Difficulty) -> bool {
        if self.record_db.delete(song_id, button_mode, difficulty) {
            let key = (song_id, button_mode, difficulty);
            if let Ok(mut guard) = self.varchive_cache.lock() {
                guard.remove(&key);
            }
            if let Ok(mut guard) = self.dirty_record_keys.lock() {
                guard.insert(key);
            }
            self.data_revision.fetch_add(1, Ordering::SeqCst);
            return true;
        }
        false
    }

    pub fn data_revision(&self) -> u64 {
        self.data_revision.load(Ordering::SeqCst)
    }

    pub fn consume_dirty_info(&self) -> (bool, HashSet<RecordKey>) {
        let full_dirty = self.full_dirty.swap(false, Ordering::SeqCst);
        let mut keys = HashSet::new();
        if let Ok(mut guard) = self.dirty_record_keys.lock() {
            std::mem::swap(&mut *guard, &mut keys);
        }
        (full_dirty, keys)
    }

    pub fn get_local_record(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
    ) -> Option<RecordValue> {
        self.record_db.get(song_id, button_mode, difficulty)
    }

    pub fn get_local_updated_at_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, i64> {
        self.record_db.get_updated_at_map(song_ids)
    }

    pub fn get_varchive_top50_summary(&self, mode: Mode) -> VArchiveTop50Summary {
        let steam_id = self.record_db.get_steam_id();
        self.record_db.get_varchive_top50_summary(&steam_id, mode)
    }

    /// V-Archive 공식 Top 50 기록이 존재하면 우선 반환하고,
    /// 비어있는 경우 로컬 `records` 테이블의 기록과 자체 Performance Rating을 결합하여 Top 50 요약을 산출한다.
    pub fn get_top50_summary_with_fallback<F>(
        &self,
        mode: Mode,
        find_floor: F,
    ) -> VArchiveTop50Summary
    where
        F: Fn(i32, Mode, Difficulty) -> f64,
    {
        let steam_id = self.record_db.get_steam_id();
        let summary = self.record_db.get_varchive_top50_summary(&steam_id, mode);
        if summary.total_recorded_count > 0 {
            return summary;
        }

        // V-Archive 공식 기록이 없는 경우: 로컬 기록(`records`) 기반으로 자체 Performance Rating 산출하여 Top 50 구성
        let local_records = self.record_db.get_local_records_by_mode(&steam_id, mode);
        if local_records.is_empty() {
            return VArchiveTop50Summary::default();
        }

        let mut rated_records: Vec<(i32, Difficulty, f64)> = local_records
            .into_iter()
            .filter_map(|r| {
                let floor = find_floor(r.song_id, mode, r.difficulty);
                if floor > 0.0 {
                    let rating = crate::service::recommend::scoring::calculate_performance_rating(
                        floor, r.rate,
                    );
                    Some((r.song_id, r.difficulty, rating))
                } else {
                    None
                }
            })
            .collect();

        // Rating 내림차순 정렬
        rated_records.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

        let top50_slice = &rated_records[..rated_records.len().min(50)];
        let mut rank_map = std::collections::HashMap::new();
        let mut cutoff_rating = 0.0f64;

        for (idx, &(song_id, diff, rating)) in top50_slice.iter().enumerate() {
            let rank = idx + 1;
            rank_map.insert((song_id, mode, diff), rank);
            cutoff_rating = rating;
        }

        VArchiveTop50Summary {
            cutoff_rating,
            rank_map,
            total_recorded_count: top50_slice.len(),
        }
    }

    pub fn get_varchive_rating_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, f64> {
        self.record_db.get_varchive_rating_map(song_ids)
    }

    pub fn get_recent_records(&self, mode: Mode, limit: usize) -> Vec<RecentRecordEntry> {
        let steam_id = self.record_db.get_steam_id();
        self.record_db.get_recent_records(&steam_id, mode, limit)
    }

    pub fn get_all_recorded_song_ids(&self) -> std::collections::HashSet<i32> {
        let steam_id = self.record_db.get_steam_id();
        self.record_db.get_all_recorded_song_ids(&steam_id)
    }

    pub fn get_varchive_cache_record(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
    ) -> Option<RecordValue> {
        let guard = overmax_core::lock_or_recover(&self.varchive_cache);
        guard.get(&(song_id, button_mode, difficulty)).copied()
    }

    fn merge_varchive_cache(&self, result: &mut HashMap<RecordKey, RecordValue>, song_ids: &[i32]) {
        let cache = overmax_core::lock_or_recover(&self.varchive_cache);
        let song_ids_set: HashSet<i32> = song_ids.iter().copied().collect();
        for (key, &(v_rate, v_mc)) in cache.iter() {
            if !song_ids_set.contains(&key.0) {
                continue;
            }
            result
                .entry(*key)
                .and_modify(|entry| {
                    entry.0 = entry.0.max(v_rate);
                    entry.1 |= v_mc;
                })
                .or_insert((v_rate, v_mc));
        }
    }
}

impl RecordSource for RecordDB {
    fn is_ready(&self) -> bool {
        self.is_ready
    }

    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue> {
        RecordDB::get_rate_map(self, song_ids)
    }
}

impl RecordSource for RecordManager {
    fn is_ready(&self) -> bool {
        self.record_db.is_ready
    }

    fn get_rate_map(&self, song_ids: &[i32]) -> HashMap<RecordKey, RecordValue> {
        let mut result = self.record_db.get_rate_map(song_ids);
        self.merge_varchive_cache(&mut result, song_ids);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn rate_map_merges_local_and_varchive_cache_by_best_rate() {
        let dir = test_dir("record-manager-merge");
        let db_path = dir.join("record.db");
        let cache_root = dir.join("varchive");
        let steam_id = "765611";
        std::fs::create_dir_all(cache_root.join(steam_id)).unwrap();

        let mut db = RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());
        assert!(db.upsert(
            42,
            overmax_core::Mode::B4,
            overmax_core::Difficulty::MX,
            98.0,
            false,
            false
        ));
        write_cache(&cache_root, steam_id);
        db.migrate_json_cache_to_db(&cache_root).unwrap();

        let db = Arc::new(db);
        let manager = RecordManager::new(db);
        manager.refresh();

        let map = manager.get_rate_map(&[42, 99]);

        assert_eq!(
            map.get(&(42, overmax_core::Mode::B4, overmax_core::Difficulty::MX)),
            Some(&(99.5, true))
        );
        assert_eq!(
            map.get(&(99, overmax_core::Mode::B4, overmax_core::Difficulty::SC)),
            Some(&(97.0, false))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    fn write_cache(cache_root: &Path, steam_id: &str) {
        let payload = json!({
            "records": [
                {"title": "42", "pattern": "MX", "score": 99.5, "maxCombo": true},
                {"title": "99", "pattern": "SC", "score": 97.0, "maxCombo": false}
            ]
        });
        std::fs::write(
            cache_root.join(steam_id).join("4.json"),
            payload.to_string(),
        )
        .unwrap();
    }

    fn test_dir(prefix: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn test_recommendation_caching_and_stats() {
        use crate::community::client::VArchiveDB;
        use crate::service::recommend::Recommender;

        let mut vdb = VArchiveDB::new();
        let song1_json = serde_json::json!({
            "name": "Song A",
            "title": "1",
            "composer": "Artist A",
            "dlcCode": "pack",
            "patterns": {
                "4B": {
                    "MX": {
                        "level": 15,
                        "floorName": "15.0"
                    }
                }
            }
        });
        let song2_json = serde_json::json!({
            "name": "Song B",
            "title": "2",
            "composer": "Artist B",
            "dlcCode": "pack",
            "patterns": {
                "4B": {
                    "MX": {
                        "level": 15,
                        "floorName": "15.0"
                    }
                }
            }
        });
        vdb.songs = vec![
            serde_json::from_value(song1_json).unwrap(),
            serde_json::from_value(song2_json).unwrap(),
        ];

        let dir = test_dir("recommend-stats-cache");
        let db_path = dir.join("record.db");
        let mut db = RecordDB::new(&db_path, None);
        assert!(db.initialize());

        assert!(db.upsert(
            1,
            overmax_core::Mode::B4,
            overmax_core::Difficulty::MX,
            99.0,
            false,
            false
        ));
        assert!(db.upsert(
            2,
            overmax_core::Mode::B4,
            overmax_core::Difficulty::MX,
            97.0,
            false,
            false
        ));

        let record_db = Arc::new(db);
        let record_manager = Arc::new(RecordManager::new(record_db));
        record_manager.refresh();

        let recommender = Recommender::new(Arc::new(vdb), record_manager);

        let result = recommender.recommend(
            1,
            overmax_core::Mode::B4,
            overmax_core::Difficulty::MX,
            0.1,
            10,
            true,
        );

        assert_eq!(result.entries.len(), 1);
        assert_eq!(result.entries[0].song_id, 2);

        assert_eq!(result.total_count, 2);
        assert_eq!(result.has_record_count, 2);
        assert_eq!(result.avg_rate, 98.0);

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_record_manager_helpers() {
        let dir = test_dir("record-manager-helpers");
        let db_path = dir.join("record.db");
        let cache_root = dir.join("varchive");
        let steam_id = "765611";
        std::fs::create_dir_all(cache_root.join(steam_id)).unwrap();

        let mut db = RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());
        assert!(db.upsert(
            123,
            overmax_core::Mode::B5,
            overmax_core::Difficulty::SC,
            99.80,
            true,
            false
        ));
        write_cache(&cache_root, steam_id); // Writes MX/SC cache: MX=99.5, SC=97.0 for song 42/99
        db.migrate_json_cache_to_db(&cache_root).unwrap();

        let db = Arc::new(db);
        let manager = RecordManager::new(db);
        manager.refresh();

        // 1. Verify get_local_record
        assert_eq!(
            manager.get_local_record(123, overmax_core::Mode::B5, overmax_core::Difficulty::SC),
            Some((99.80, true))
        );
        assert_eq!(
            manager.get_local_record(999, overmax_core::Mode::B4, overmax_core::Difficulty::NM),
            None
        );

        // 2. Verify get_varchive_cache_record
        // Write cache has MX 99.5 for song 42
        assert_eq!(
            manager.get_varchive_cache_record(
                42,
                overmax_core::Mode::B4,
                overmax_core::Difficulty::MX
            ),
            Some((99.5, true))
        );
        assert_eq!(
            manager.get_varchive_cache_record(
                42,
                overmax_core::Mode::B4,
                overmax_core::Difficulty::NM
            ),
            None
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_upsert_varchive_record_incremental_update() {
        let dir = test_dir("record-manager-incremental");
        let db_path = dir.join("record.db");
        let mut db = RecordDB::new(&db_path, Some("765611"));
        assert!(db.initialize());

        let db = Arc::new(db);
        let manager = RecordManager::new(db);

        assert_eq!(
            manager.get_varchive_cache_record(
                50,
                overmax_core::Mode::B6,
                overmax_core::Difficulty::MX
            ),
            None
        );

        // Perform O(1) incremental update
        manager.upsert_varchive_record(
            (50, overmax_core::Mode::B6, overmax_core::Difficulty::MX),
            (99.85, true),
        );

        assert_eq!(
            manager.get_varchive_cache_record(
                50,
                overmax_core::Mode::B6,
                overmax_core::Difficulty::MX
            ),
            Some((99.85, true))
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_handle_verified_play_deletes_record_when_unplayed_on_song_select() {
        let dir = test_dir("record-manager-handle-unplayed");
        let db_path = dir.join("record.db");
        let mut db = RecordDB::new(&db_path, Some("765611"));
        assert!(db.initialize());

        let db = Arc::new(db);
        let manager = RecordManager::new(db);

        // 1. 과거 오인식으로 잘못 들어간 레코드 (90.0%)
        assert!(manager.upsert(
            77,
            overmax_core::Mode::B4,
            overmax_core::Difficulty::SC,
            90.0,
            false,
            false
        ));
        assert_eq!(
            manager.get_local_record(77, overmax_core::Mode::B4, overmax_core::Difficulty::SC),
            Some((90.0, false))
        );

        // 2. 선곡 화면에서 미플레이(0.00%)로 인식되어 이벤트 발생 -> 삭제되어야 함!
        let event = VerifiedPlayEvent {
            song_id: 77,
            mode: overmax_core::Mode::B4,
            diff: overmax_core::Difficulty::SC,
            rate: 0.0,
            is_max_combo: false,
            is_result_screen: false,
        };
        assert!(manager.handle_verified_play(&event));

        // 3. 로컬 DB에서 삭제 확인
        assert_eq!(
            manager.get_local_record(77, overmax_core::Mode::B4, overmax_core::Difficulty::SC),
            None
        );

        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn test_handle_verified_play_logs_to_play_events_on_result_screen() {
        let dir = test_dir("record-manager-play-events");
        let db_path = dir.join("record.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let db = Arc::new(db);
        let manager = RecordManager::new(db);

        let event = VerifiedPlayEvent {
            song_id: 88,
            mode: overmax_core::Mode::B6,
            diff: overmax_core::Difficulty::HD,
            rate: 98.76,
            is_max_combo: true,
            is_result_screen: true,
        };

        // 결과창 이벤트 처리 시 records UPSERT + play_events INSERT 동시 진행
        assert!(manager.handle_verified_play(&event));

        // get_recent_records를 통해 play_events에서 정상 조회되는지 확인
        let recent = manager.get_recent_records(overmax_core::Mode::B6, 5);
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].song_id, 88);
        assert!((recent[0].rate - 98.76).abs() < 0.001);
        assert!(recent[0].is_max_combo);

        let _ = std::fs::remove_dir_all(dir);
    }
}
