//! `RecordDB`의 읽기 전용 리포트/조회 책임.
//! Top-50 요약·랭크, 최근 기록, 동기화 후보 조회 등을 담당한다.
use super::*;

impl RecordDB {
    pub fn get_varchive_rating_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, f64> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        if steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return std::collections::HashMap::new();
        }

        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, rating
             FROM varchive_records
             WHERE steam_id=?1 AND song_id IN ({}) AND rating > 0",
            placeholders
        );

        let mut map = std::collections::HashMap::new();
        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(&query) {
                let mut p = Vec::new();
                p.push(&steam_id as &dyn rusqlite::ToSql);
                let song_ids_str: Vec<String> = song_ids.iter().map(|s| s.to_string()).collect();
                for id_str in &song_ids_str {
                    p.push(id_str as &dyn rusqlite::ToSql);
                }
                if let Ok(mut rows) = stmt.query(&*p) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (Ok(song_id_str), Ok(button_mode), Ok(difficulty), Ok(rating)) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, f64>(3),
                        ) {
                            if let Ok(sid) = song_id_str.parse::<i32>() {
                                if let (Some(m), Some(d)) = (
                                    Mode::from_str(&button_mode),
                                    Difficulty::from_str(&difficulty),
                                ) {
                                    map.insert((sid, m, d), rating);
                                }
                            }
                        }
                    }
                }
            }
        });
        map
    }

    /// 사용자의 모든 플레이/동기화 이력(records, varchive_records, play_events)에 등장하는 모든 song_id 집합을 반환한다.
    /// 보유 DLC 자동 추론에 사용된다.
    pub fn get_all_recorded_song_ids(&self, steam_id: &str) -> std::collections::HashSet<i32> {
        let mut set = std::collections::HashSet::new();
        if !self.is_ready {
            return set;
        }

        let _ = self.with_rate_map_connection(|conn| {
            // 1. records
            if let Ok(mut stmt) =
                conn.prepare("SELECT DISTINCT song_id FROM records WHERE steam_id = ?1")
            {
                if let Ok(mut rows) = stmt.query([steam_id]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(id_str) = row.get::<_, String>(0) {
                            if let Ok(id) = id_str.parse::<i32>() {
                                set.insert(id);
                            }
                        }
                    }
                }
            }
            // 2. varchive_records
            if let Ok(mut stmt) =
                conn.prepare("SELECT DISTINCT song_id FROM varchive_records WHERE steam_id = ?1")
            {
                if let Ok(mut rows) = stmt.query([steam_id]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(id_str) = row.get::<_, String>(0) {
                            if let Ok(id) = id_str.parse::<i32>() {
                                set.insert(id);
                            }
                        }
                    }
                }
            }
            // 3. play_events
            if let Ok(mut stmt) =
                conn.prepare("SELECT DISTINCT song_id FROM play_events WHERE steam_id = ?1")
            {
                if let Ok(mut rows) = stmt.query([steam_id]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let Ok(id_str) = row.get::<_, String>(0) {
                            if let Ok(id) = id_str.parse::<i32>() {
                                set.insert(id);
                            }
                        }
                    }
                }
            }
        });

        set
    }

    pub fn get_recent_records(
        &self,
        steam_id: &str,
        mode: Mode,
        limit: usize,
    ) -> Vec<RecentRecordEntry> {
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID || limit == 0
        {
            return Vec::new();
        }

        let button_mode = mode.as_str();
        let mut results = Vec::new();

        // play_events 테이블(결과창 실제 완주 이력)에서만 100% 조회
        // 선곡창 휠 스크롤/탐색으로 오염될 수 있는 records.updated_at을 완전히 배제하여
        // 플레이어가 실제로 플레이한 곡들만 세션 이력으로 수집한다.
        let query_events = "SELECT song_id, difficulty, rate, is_max_combo, played_at
                            FROM play_events
                            WHERE steam_id = ?1 AND button_mode = ?2
                            ORDER BY played_at DESC
                            LIMIT ?3";

        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(query_events) {
                if let Ok(mut rows) =
                    stmt.query(rusqlite::params![steam_id, button_mode, limit as i64])
                {
                    while let Ok(Some(row)) = rows.next() {
                        if let (
                            Ok(song_id_str),
                            Ok(diff_str),
                            Ok(rate),
                            Ok(mc_int),
                            Ok(played_at),
                        ) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, f64>(2),
                            row.get::<_, i32>(3),
                            row.get::<_, i64>(4),
                        ) {
                            if let (Ok(sid), Some(diff)) =
                                (song_id_str.parse::<i32>(), Difficulty::from_str(&diff_str))
                            {
                                results.push(RecentRecordEntry {
                                    song_id: sid,
                                    button_mode: mode,
                                    difficulty: diff,
                                    rate,
                                    is_max_combo: mc_int != 0,
                                    updated_at: played_at,
                                });
                            }
                        }
                    }
                }
            }
        });

        results
    }

    /// 특정 버튼 모드의 모든 유효 로컬 기록(`records` 테이블, rate > 0)을 조회한다.
    /// V-Archive 연동이 없을 때 로컬 Top-50 fallback을 산출하기 위해 사용된다.
    pub fn get_local_records_by_mode(&self, steam_id: &str, mode: Mode) -> Vec<RecentRecordEntry> {
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return Vec::new();
        }

        let button_mode = mode.as_str();
        let mut results = Vec::new();

        let query = "SELECT song_id, difficulty, rate, is_max_combo, updated_at
                     FROM records
                     WHERE steam_id = ?1 AND button_mode = ?2 AND rate > 0";

        let _ = self.with_rate_map_connection(|conn| {
            if let Ok(mut stmt) = conn.prepare(query) {
                if let Ok(mut rows) = stmt.query(rusqlite::params![steam_id, button_mode]) {
                    while let Ok(Some(row)) = rows.next() {
                        if let (
                            Ok(song_id_str),
                            Ok(diff_str),
                            Ok(rate),
                            Ok(mc_int),
                            Ok(updated_at),
                        ) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, f64>(2),
                            row.get::<_, i32>(3),
                            row.get::<_, i64>(4),
                        ) {
                            if let (Ok(sid), Some(diff)) =
                                (song_id_str.parse::<i32>(), Difficulty::from_str(&diff_str))
                            {
                                results.push(RecentRecordEntry {
                                    song_id: sid,
                                    button_mode: mode,
                                    difficulty: diff,
                                    rate,
                                    is_max_combo: mc_int != 0,
                                    updated_at,
                                });
                            }
                        }
                    }
                }
            }
        });

        results
    }

    /// All local rows for a Steam id (for sync). Ignores internal `steam_id` mutex.
    pub fn all_records_for_steam(
        &self,
        steam_id: &str,
    ) -> std::collections::HashMap<RecordKey, (f64, bool)> {
        let mut map = std::collections::HashMap::new();
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return map;
        }
        let Ok(conn) = self.open_conn() else {
            return map;
        };
        let mut stmt = match conn.prepare(
            "SELECT song_id, button_mode, difficulty, rate, is_max_combo
             FROM records
             WHERE steam_id = ?1 AND rate > 0",
        ) {
            Ok(s) => s,
            Err(_) => return map,
        };
        let mut rows = match stmt.query(params![steam_id]) {
            Ok(r) => r,
            Err(_) => return map,
        };
        while let Ok(Some(row)) = rows.next() {
            let song_id_str: String = match row.get(0) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Ok(sid) = song_id_str.parse::<i32>() else {
                continue;
            };
            let button_mode: String = row.get(1).unwrap_or_default();
            let difficulty: String = row.get(2).unwrap_or_default();
            let rate: f64 = row.get(3).unwrap_or(0.0);
            let is_max_combo: i32 = row.get(4).unwrap_or(0);
            if let (Some(m), Some(d)) = (
                Mode::from_str(&button_mode),
                Difficulty::from_str(&difficulty),
            ) {
                map.insert((sid, m, d), (rate, is_max_combo != 0));
            }
        }
        map
    }

    pub fn load_varchive_records(
        &self,
        steam_id: &str,
    ) -> Result<std::collections::HashMap<RecordKey, RecordValue>> {
        let mut map = std::collections::HashMap::new();
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return Ok(map);
        }
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            "SELECT song_id, button_mode, difficulty, score, max_combo 
             FROM varchive_records WHERE steam_id = ?1",
        )?;
        let mut rows = stmt.query(params![steam_id])?;
        while let Some(row) = rows.next()? {
            let song_id_str: String = row.get(0)?;
            let song_id: i32 = song_id_str.parse().unwrap_or(0);
            let button_mode: String = row.get(1)?;
            let difficulty: String = row.get(2)?;
            let score: f64 = row.get(3)?;
            let max_combo_int: i32 = row.get(4)?;
            let max_combo = max_combo_int != 0;

            if let (Some(m), Some(d)) = (
                Mode::from_str(&button_mode),
                Difficulty::from_str(&difficulty),
            ) {
                map.insert((song_id, m, d), (score as f32, max_combo));
            }
        }
        Ok(map)
    }

    /// Direct SQL LEFT JOIN query to fetch sync candidates directly from SQLite DB.
    pub fn query_sync_candidates(&self, steam_id: &str) -> Vec<RawSyncCandidateRow> {
        let mut list = Vec::new();
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return list;
        }
        let Ok(conn) = self.open_conn() else {
            return list;
        };
        let mut stmt = match conn.prepare(
            "SELECT 
                r.song_id,
                r.button_mode,
                r.difficulty,
                r.rate,
                r.is_max_combo,
                v.score,
                v.max_combo
             FROM records r
             LEFT JOIN varchive_records v 
                ON r.steam_id = v.steam_id 
               AND r.song_id = v.song_id 
               AND r.button_mode = v.button_mode 
               AND r.difficulty = v.difficulty
             WHERE r.steam_id = ?1
               AND r.rate > 0
               AND (
                   v.song_id IS NULL 
                   OR (r.rate - v.score) >= 0.01 
                   OR (r.is_max_combo = 1 AND COALESCE(v.max_combo, 0) = 0)
               )",
        ) {
            Ok(s) => s,
            Err(_) => return list,
        };

        let mut rows = match stmt.query(params![steam_id]) {
            Ok(r) => r,
            Err(_) => return list,
        };

        while let Ok(Some(row)) = rows.next() {
            let song_id_str: String = match row.get(0) {
                Ok(v) => v,
                Err(_) => continue,
            };
            let Ok(song_id) = song_id_str.parse::<i32>() else {
                continue;
            };
            let bm_str: String = row.get(1).unwrap_or_default();
            let diff_str: String = row.get(2).unwrap_or_default();
            let local_rate: f64 = row.get(3).unwrap_or(0.0);
            let local_mc: i32 = row.get(4).unwrap_or(0);
            let v_score: Option<f64> = row.get(5).ok();
            let v_mc_int: Option<i32> = row.get(6).ok();

            if let (Some(bm), Some(d)) = (Mode::from_str(&bm_str), Difficulty::from_str(&diff_str))
            {
                list.push(RawSyncCandidateRow {
                    song_id,
                    button_mode: bm,
                    difficulty: d,
                    local_rate,
                    local_mc: local_mc != 0,
                    varchive_rate: v_score,
                    varchive_mc: v_mc_int.map(|m| m != 0),
                });
            }
        }

        list
    }

    pub fn get_latest_updated_at_from_db(&self, steam_id: &str, button: i32) -> Option<String> {
        if !self.is_ready {
            return None;
        }
        let mode = match button {
            4 => Mode::B4,
            5 => Mode::B5,
            6 => Mode::B6,
            8 => Mode::B8,
            _ => return None,
        };
        let button_mode = mode.as_str();
        let conn = self.open_conn().ok()?;
        let mut stmt = conn
            .prepare(
                "SELECT updated_at 
             FROM varchive_records 
             WHERE steam_id = ?1 AND button_mode = ?2 
             ORDER BY updated_at DESC LIMIT 1",
            )
            .ok()?;
        let mut rows = stmt.query(params![steam_id, button_mode]).ok()?;
        if let Some(row) = rows.next().ok()? {
            let val: Option<String> = row.get(0).ok();
            return val;
        }
        None
    }

    pub fn get_varchive_top50_summary(&self, steam_id: &str, mode: Mode) -> VArchiveTop50Summary {
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return VArchiveTop50Summary::default();
        }

        let Ok(conn) = self.open_conn() else {
            return VArchiveTop50Summary::default();
        };

        let button_mode = mode.as_str();
        let query = "SELECT song_id, difficulty, rating
                     FROM varchive_records
                     WHERE steam_id = ?1 AND button_mode = ?2 AND rating > 0
                     ORDER BY rating DESC
                     LIMIT 50";

        let mut rank_map = std::collections::HashMap::new();
        let mut rating_map = std::collections::HashMap::new();
        let mut cutoff_rating = 0.0f64;
        let mut total_count = 0usize;

        if let Ok(mut stmt) = conn.prepare(query) {
            if let Ok(mut rows) = stmt.query(rusqlite::params![steam_id, button_mode]) {
                let mut rank = 1;
                while let Ok(Some(row)) = rows.next() {
                    if let (Ok(song_id_str), Ok(diff_str), Ok(rating)) = (
                        row.get::<_, String>(0),
                        row.get::<_, String>(1),
                        row.get::<_, f64>(2),
                    ) {
                        if let (Ok(sid), Some(diff)) =
                            (song_id_str.parse::<i32>(), Difficulty::from_str(&diff_str))
                        {
                            rank_map.insert((sid, mode, diff), rank);
                            rating_map.insert((sid, mode, diff), rating);
                            cutoff_rating = rating;
                            total_count += 1;
                            rank += 1;
                        }
                    }
                }
            }
        }

        VArchiveTop50Summary {
            cutoff_rating,
            rank_map,
            rating_map,
            total_recorded_count: total_count,
        }
    }

    pub fn get_varchive_top50_rank(
        &self,
        steam_id: &str,
        mode: Mode,
        song_id: &str,
        difficulty: Difficulty,
    ) -> Result<Option<usize>, String> {
        let button_mode = mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return Err("DB is not ready".to_string());
        }
        let conn = self.open_conn().map_err(|e| e.to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT song_id, difficulty 
                 FROM varchive_records 
                 WHERE steam_id = ?1 AND button_mode = ?2 AND rating > 0
                 ORDER BY rating DESC LIMIT 50",
            )
            .map_err(|e| e.to_string())?;

        let mut rows = stmt
            .query(params![steam_id, button_mode])
            .map_err(|e| e.to_string())?;
        let mut rank = 1;
        while let Some(row) = rows.next().map_err(|e| e.to_string())? {
            let s_id: String = row.get(0).map_err(|e| e.to_string())?;
            let diff: String = row.get(1).map_err(|e| e.to_string())?;
            if s_id == song_id && diff == difficulty {
                return Ok(Some(rank));
            }
            rank += 1;
        }
        Ok(None)
    }
}
