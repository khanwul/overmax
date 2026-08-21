use overmax_core::{Difficulty, Mode, RecordKey, RecordValue, VerifiedPlayEvent};
use rusqlite::{params, Connection, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Debug, Clone)]
pub struct RawSyncCandidateRow {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub local_rate: f64,
    pub local_mc: bool,
    pub varchive_rate: Option<f64>,
    pub varchive_mc: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct VArchiveTop50Summary {
    /// 50위 곡의 레이팅 (50개 미만인 경우 가장 낮은 곡의 레이팅 또는 0.0)
    pub cutoff_rating: f64,
    /// 1위 ~ 50위 곡들의 순위 맵 (RecordKey -> 1-based rank)
    pub rank_map: std::collections::HashMap<RecordKey, usize>,
    /// 모드 내 등록된 유효 레이팅(rating > 0) 곡 수
    pub total_recorded_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RecentRecordEntry {
    pub song_id: i32,
    pub button_mode: Mode,
    pub difficulty: Difficulty,
    pub rate: f64,
    pub is_max_combo: bool,
    pub updated_at: i64,
}

pub struct RecordDB {
    db_path: PathBuf,
    steam_id: Mutex<String>,
    pub is_ready: bool,
}

impl RecordDB {
    const UNKNOWN_STEAM_ID: &'static str = "__unknown__";

    pub fn new(db_path: impl AsRef<Path>, steam_id: Option<&str>) -> Self {
        Self {
            db_path: db_path.as_ref().to_path_buf(),
            steam_id: Mutex::new(Self::normalize_steam_id(steam_id)),
            is_ready: false,
        }
    }

    fn normalize_steam_id(steam_id: Option<&str>) -> String {
        match steam_id {
            Some(s) if !s.trim().is_empty() => s.trim().to_string(),
            _ => Self::UNKNOWN_STEAM_ID.to_string(),
        }
    }

    pub fn masked_steam_id(&self) -> String {
        self.mask_id(&self.get_steam_id())
    }

    fn mask_id(&self, steam_id: &str) -> String {
        if steam_id == Self::UNKNOWN_STEAM_ID {
            return steam_id.to_string();
        }
        if steam_id.len() <= 8 {
            return "***".to_string();
        }
        format!("{}...{}", &steam_id[..4], &steam_id[steam_id.len() - 4..])
    }

    pub fn initialize(&mut self) -> bool {
        if let Some(parent) = self.db_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let conn_result = self.open_conn();
        if let Ok(mut conn) = conn_result {
            if self.create_records_table(&conn).is_ok()
                && self.create_varchive_records_table(&conn).is_ok()
                && self.create_play_events_table(&conn).is_ok()
            {
                self.ensure_schema(&mut conn);
                self.is_ready = true;
                return true;
            }
        }
        false
    }

    /// Internal connection factory ensuring WAL mode, busy timeout, and synchronous settings.
    pub fn open_conn(&self) -> Result<Connection> {
        let conn = Connection::open(&self.db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;",
        )?;
        Ok(conn)
    }

    /// Executes an operation with exponential backoff retry for transient SQLITE_BUSY errors.
    pub fn with_retry<T, F>(&self, mut op: F) -> Result<T>
    where
        F: FnMut(&Connection) -> Result<T>,
    {
        let mut backoff = std::time::Duration::from_millis(10);
        for attempt in 0..3 {
            let conn = self.open_conn()?;
            match op(&conn) {
                Ok(val) => return Ok(val),
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if (err.extended_code == 5 /* SQLITE_BUSY */ || err.extended_code == 261 /* SQLITE_BUSY_RECOVERY */ || err.extended_code == 6/* SQLITE_LOCKED */)
                        && attempt < 2 =>
                {
                    std::thread::sleep(backoff);
                    backoff *= 2;
                }
                Err(e) => return Err(e),
            }
        }
        let conn = self.open_conn()?;
        op(&conn)
    }

    fn create_records_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS records (
                steam_id      TEXT NOT NULL,
                song_id       TEXT NOT NULL,
                button_mode   TEXT NOT NULL,
                difficulty    TEXT NOT NULL,
                rate          REAL NOT NULL,
                is_max_combo  INTEGER NOT NULL DEFAULT 0,
                updated_at    INTEGER NOT NULL DEFAULT (strftime('%s', 'now')),
                PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_recent ON records (steam_id, button_mode, updated_at DESC)",
            [],
        )?;
        Ok(())
    }

    fn create_varchive_records_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS varchive_records (
                steam_id      TEXT NOT NULL,
                song_id       TEXT NOT NULL,
                button_mode   TEXT NOT NULL,
                difficulty    TEXT NOT NULL,
                raw_data      TEXT NOT NULL,
                score         REAL GENERATED ALWAYS AS (json_extract(raw_data, '$.score')) STORED,
                max_combo     INTEGER GENERATED ALWAYS AS (json_extract(raw_data, '$.maxCombo')) STORED,
                updated_at    TEXT GENERATED ALWAYS AS (json_extract(raw_data, '$.updatedAt')) STORED,
                rating        REAL GENERATED ALWAYS AS (json_extract(raw_data, '$.rating')) STORED,
                PRIMARY KEY (steam_id, song_id, button_mode, difficulty)
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_varchive_updated_at ON varchive_records (steam_id, button_mode, updated_at DESC)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_varchive_rating ON varchive_records (rating)",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_varchive_top50 ON varchive_records (steam_id, button_mode, rating DESC)",
            [],
        )?;
        Ok(())
    }

    fn create_play_events_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS play_events (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                steam_id      TEXT NOT NULL,
                song_id       TEXT NOT NULL,
                button_mode   TEXT NOT NULL,
                difficulty    TEXT NOT NULL,
                rate          REAL NOT NULL,
                is_max_combo  INTEGER NOT NULL DEFAULT 0,
                played_at     INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
            )",
            [],
        )?;
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_play_events_recent ON play_events (steam_id, button_mode, played_at DESC)",
            [],
        )?;
        Ok(())
    }

    fn table_has_column(
        &self,
        conn: &Connection,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool> {
        let mut stmt = conn.prepare(&format!("PRAGMA table_info({})", table_name))?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let name: String = row.get(1)?;
            if name == column_name {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn ensure_schema(&self, conn: &mut Connection) {
        if let Ok(has_col) = self.table_has_column(conn, "records", "is_max_combo") {
            if !has_col {
                let _ = conn.execute("DROP TABLE records", []);
                let _ = self.create_records_table(conn);
            }
        }
        let _ = self.create_play_events_table(conn);
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_records_recent ON records (steam_id, button_mode, updated_at DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_varchive_top50 ON varchive_records (steam_id, button_mode, rating DESC)",
            [],
        );
        let _ = conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_play_events_recent ON play_events (steam_id, button_mode, played_at DESC)",
            [],
        );
    }

    pub fn set_steam_id(&self, steam_id: Option<&str>) -> (bool, String, String) {
        let new_sid = Self::normalize_steam_id(steam_id);
        let mut guard = overmax_core::lock_or_recover(&self.steam_id);
        let old_sid = guard.clone();
        let changed = old_sid != new_sid;
        *guard = new_sid.clone();
        (changed, self.mask_id(&old_sid), self.mask_id(&new_sid))
    }

    pub fn get_steam_id(&self) -> String {
        overmax_core::lock_or_recover(&self.steam_id).clone()
    }

    pub fn upsert(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
        rate: f64,
        is_max_combo: bool,
        only_if_improved: bool,
    ) -> bool {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return false;
        }

        let sid = song_id.to_string();
        let steam_id = self.get_steam_id();
        let is_max_combo_int = if is_max_combo { 1 } else { 0 };

        let res = self.with_retry(|conn| {
            let mut existing_rate: Option<f64> = None;
            let mut existing_max_combo: Option<i32> = None;

            let query_res = conn.query_row(
                "SELECT rate, is_max_combo FROM records 
                 WHERE steam_id = ?1 AND song_id = ?2 AND button_mode = ?3 AND difficulty = ?4",
                params![steam_id, sid, button_mode, difficulty],
                |row| {
                    let r: Option<f64> = row.get(0).ok();
                    let mc: Option<i32> = row.get(1).ok();
                    Ok((r, mc))
                },
            );

            if let Ok((r, mc)) = query_res {
                existing_rate = r;
                existing_max_combo = mc;
            }

            let mut final_rate = rate;
            let mut final_max_combo = is_max_combo_int;

            if only_if_improved {
                let should_update_rate = existing_rate.is_none_or(|ext_r| rate > ext_r);
                let should_update_combo =
                    existing_max_combo.is_none_or(|ext_mc| is_max_combo_int > ext_mc);

                if !should_update_rate && !should_update_combo {
                    return Ok(false);
                }

                final_rate = existing_rate.map_or(rate, |ext_r| rate.max(ext_r));
                final_max_combo = existing_max_combo
                    .map_or(is_max_combo_int, |ext_mc| is_max_combo_int.max(ext_mc));
            } else {
                // 선곡창(only_if_improved = false) 탐색 시:
                // 기존 기록과 rate 및 is_max_combo가 완전히 동일하다면 updated_at을 오염시키지 않고 즉시 반환
                if let (Some(ext_r), Some(ext_mc)) = (existing_rate, existing_max_combo) {
                    if (ext_r - rate).abs() < 0.001 && ext_mc == is_max_combo_int {
                        return Ok(false);
                    }
                }
            }

            conn.execute(
                "INSERT INTO records (
                    steam_id, song_id, button_mode, difficulty, rate, is_max_combo
                )
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(steam_id, song_id, button_mode, difficulty) DO UPDATE SET
                    rate          = excluded.rate,
                    is_max_combo  = excluded.is_max_combo,
                    updated_at    = CAST(strftime('%s', 'now') AS INTEGER)",
                params![
                    steam_id,
                    sid,
                    button_mode,
                    difficulty,
                    final_rate,
                    final_max_combo
                ],
            )?;
            Ok(true)
        });
        res.unwrap_or(false)
    }

    pub fn delete(&self, song_id: i32, button_mode: Mode, difficulty: Difficulty) -> bool {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return false;
        }

        let sid = song_id.to_string();
        let steam_id = self.get_steam_id();

        let res = self.with_retry(|conn| {
            let rows = conn.execute(
                "DELETE FROM records WHERE steam_id=?1 AND song_id=?2 AND button_mode=?3 AND difficulty=?4",
                params![steam_id, sid, button_mode, difficulty],
            )?;
            Ok(rows > 0)
        });
        res.unwrap_or(false)
    }

    pub fn get(
        &self,
        song_id: i32,
        button_mode: Mode,
        difficulty: Difficulty,
    ) -> Option<RecordValue> {
        let button_mode = button_mode.as_str();
        let difficulty = difficulty.as_str();
        if !self.is_ready {
            return None;
        }

        let steam_id = self.get_steam_id();
        if let Ok(conn) = self.open_conn() {
            let mut stmt = conn
                .prepare(
                    "SELECT rate, is_max_combo FROM records
                 WHERE steam_id=?1 AND song_id=?2 AND button_mode=?3 AND difficulty=?4",
                )
                .ok()?;
            let result: Result<(f64, i32)> = stmt.query_row(
                params![steam_id, song_id.to_string(), button_mode, difficulty],
                |row| Ok((row.get(0)?, row.get(1)?)),
            );
            if let Ok((rate, is_max_combo)) = result {
                return Some((rate as f32, is_max_combo != 0));
            }
        }
        None
    }

    fn with_rate_map_connection<T>(&self, f: impl FnOnce(&Connection) -> T) -> Option<T> {
        thread_local! {
            static RATE_MAP_CONN: std::cell::RefCell<Option<(PathBuf, Connection)>> =
                const { std::cell::RefCell::new(None) };
        }

        RATE_MAP_CONN.with(|cell| {
            let mut slot = cell.borrow_mut();
            let need_new = match slot.as_ref() {
                Some((path, _)) => path != &self.db_path,
                None => true,
            };
            if need_new {
                let conn = self.open_conn().ok()?;
                *slot = Some((self.db_path.clone(), conn));
            }
            slot.as_ref().map(|(_, conn)| f(conn))
        })
    }

    pub fn get_rate_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, RecordValue> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, rate, is_max_combo 
             FROM records 
             WHERE steam_id=?1 AND song_id IN ({})",
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
                        if let (
                            Ok(song_id_str),
                            Ok(button_mode),
                            Ok(difficulty),
                            Ok(rate),
                            Ok(is_max_combo_int),
                        ) = (
                            row.get::<_, String>(0),
                            row.get::<_, String>(1),
                            row.get::<_, String>(2),
                            row.get::<_, f64>(3),
                            row.get::<_, i32>(4),
                        ) {
                            if let Ok(sid) = song_id_str.parse::<i32>() {
                                if let (Some(m), Some(d)) = (
                                    Mode::from_str(&button_mode),
                                    Difficulty::from_str(&difficulty),
                                ) {
                                    map.insert((sid, m, d), (rate as f32, is_max_combo_int != 0));
                                }
                            }
                        }
                    }
                }
            }
        });
        map
    }

    pub fn get_updated_at_map(
        &self,
        song_ids: &[i32],
    ) -> std::collections::HashMap<RecordKey, i64> {
        if !self.is_ready || song_ids.is_empty() {
            return std::collections::HashMap::new();
        }

        let steam_id = self.get_steam_id();
        let placeholders = vec!["?"; song_ids.len()].join(",");
        let query = format!(
            "SELECT song_id, button_mode, difficulty, updated_at
             FROM records
             WHERE steam_id=?1 AND song_id IN ({})",
            placeholders
        );

        let mut map = std::collections::HashMap::new();
        let _ =
            self.with_rate_map_connection(|conn| {
                if let Ok(mut stmt) = conn.prepare(&query) {
                    let mut p = Vec::new();
                    p.push(&steam_id as &dyn rusqlite::ToSql);
                    let song_ids_str: Vec<String> =
                        song_ids.iter().map(|s| s.to_string()).collect();
                    for id_str in &song_ids_str {
                        p.push(id_str as &dyn rusqlite::ToSql);
                    }
                    if let Ok(mut rows) = stmt.query(&*p) {
                        while let Ok(Some(row)) = rows.next() {
                            if let (
                                Ok(song_id_str),
                                Ok(button_mode),
                                Ok(difficulty),
                                Ok(updated_at),
                            ) = (
                                row.get::<_, String>(0),
                                row.get::<_, String>(1),
                                row.get::<_, String>(2),
                                row.get::<_, i64>(3),
                            ) {
                                if let Ok(sid) = song_id_str.parse::<i32>() {
                                    if let (Some(m), Some(d)) = (
                                        Mode::from_str(&button_mode),
                                        Difficulty::from_str(&difficulty),
                                    ) {
                                        map.insert((sid, m, d), updated_at);
                                    }
                                }
                            }
                        }
                    }
                }
            });
        map
    }

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

    /// 결과창에서 발생한 플레이 이벤트를 45초 디바운스 가드 하에 play_events 테이블에 누적 기록한다.
    pub fn insert_play_event(&self, event: &VerifiedPlayEvent, now_unix: i64) -> bool {
        if !self.is_ready || !event.is_result_screen || event.rate < 0.01 {
            return false;
        }

        let steam_id = self.get_steam_id();
        if steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return false;
        }

        let song_id_str = event.song_id.to_string();
        let button_mode = event.mode.as_str();
        let diff_str = event.diff.as_str();
        let debounce_window = (now_unix - 45).max(0);

        let result = self.with_retry(|conn| {
            // 45초 이내 동일 패턴 디바운스 가드 (레벨업/언락 팝업 재진입 방지)
            // 단, 연출 지연으로 MAX COMBO가 뒤늦게 감지되었거나 Rate가 개선된 경우 기존 행을 즉시 UPDATE한다.
            let mut check_stmt = conn.prepare_cached(
                "SELECT id, rate, is_max_combo FROM play_events
                 WHERE steam_id = ?1 AND song_id = ?2 AND button_mode = ?3 AND difficulty = ?4
                   AND played_at >= ?5
                 ORDER BY played_at DESC
                 LIMIT 1",
            )?;
            let mut rows = check_stmt.query(rusqlite::params![
                steam_id,
                song_id_str,
                button_mode,
                diff_str,
                debounce_window
            ])?;

            if let Ok(Some(row)) = rows.next() {
                let existing_id: i64 = row.get(0)?;
                let existing_rate: f64 = row.get(1)?;
                let existing_mc: i32 = row.get(2)?;
                let new_mc = if event.is_max_combo { 1 } else { 0 };

                let event_rate_f64 = event.rate as f64;
                let mc_improved = existing_mc == 0 && new_mc == 1;
                let rate_improved = event_rate_f64 > existing_rate + 0.001;

                if mc_improved || rate_improved {
                    let final_rate = existing_rate.max(event_rate_f64);
                    let final_mc = existing_mc.max(new_mc);
                    let mut update_stmt = conn.prepare_cached(
                        "UPDATE play_events
                         SET rate = ?1, is_max_combo = ?2
                         WHERE id = ?3",
                    )?;
                    update_stmt.execute(rusqlite::params![final_rate, final_mc, existing_id])?;
                    return Ok(true);
                }

                return Ok(false);
            }

            let mut insert_stmt = conn.prepare_cached(
                "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            )?;
            insert_stmt.execute(rusqlite::params![
                steam_id,
                song_id_str,
                button_mode,
                diff_str,
                event.rate,
                if event.is_max_combo { 1 } else { 0 },
                now_unix,
            ])?;
            Ok(true)
        });

        result.unwrap_or(false)
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

    pub fn merge_varchive_fetched_records(
        &self,
        steam_id: &str,
        button: i32,
        data: &serde_json::Value,
        clear_first: bool,
    ) -> Result<(), String> {
        if !self.is_ready {
            return Err("DB is not ready".to_string());
        }
        let mut conn = self.open_conn().map_err(|e| e.to_string())?;
        let tx = conn.transaction().map_err(|e| e.to_string())?;

        let mode = match button {
            4 => Mode::B4,
            5 => Mode::B5,
            6 => Mode::B6,
            8 => Mode::B8,
            _ => return Err(format!("invalid button: {button}")),
        };
        let button_mode = mode.as_str();

        if clear_first {
            tx.execute(
                "DELETE FROM varchive_records WHERE steam_id = ?1 AND button_mode = ?2",
                params![steam_id, button_mode],
            )
            .map_err(|e| e.to_string())?;
        }

        let new_records = data
            .get("records")
            .and_then(|r| r.as_array())
            .ok_or_else(|| "records field missing or not an array".to_string())?;

        for rec in new_records {
            let Some(obj) = rec.as_object() else {
                continue;
            };
            let song_id = obj
                .get("title")
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s.clone()),
                    serde_json::Value::Number(n) => Some(n.to_string()),
                    _ => None,
                })
                .ok_or_else(|| "missing title (song_id)".to_string())?;

            let difficulty = obj
                .get("pattern")
                .and_then(|v| v.as_str())
                .ok_or_else(|| "missing pattern (difficulty)".to_string())?;

            let raw_data_str = serde_json::to_string(rec).map_err(|e| e.to_string())?;

            tx.execute(
                "INSERT OR REPLACE INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![steam_id, song_id, button_mode, difficulty, raw_data_str],
            )
            .map_err(|e| e.to_string())?;
        }

        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
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

    pub fn migrate_json_cache_to_db(&self, cache_root: &Path) -> Result<(), String> {
        if !self.is_ready {
            return Err("DB is not ready".to_string());
        }
        let steam_id = self.get_steam_id();
        let user_dir = cache_root.join(&steam_id);
        if !user_dir.exists() {
            return Ok(());
        }

        for button in &[4, 5, 6, 8] {
            let path = user_dir.join(format!("{button}.json"));
            if path.exists() {
                let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
                if let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) {
                    if let Err(e) =
                        self.merge_varchive_fetched_records(&steam_id, *button, &data, true)
                    {
                        return Err(format!("Failed to migrate {button}.json: {e}"));
                    }
                }
                let backup_path = user_dir.join(format!("{button}.json.bak"));
                let _ = fs::rename(&path, &backup_path);
            }
        }
        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn test_concurrent_record_db_writes() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("concurrent_record.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());
        let db = Arc::new(db);

        let mut handles = Vec::new();
        // 8 concurrent writer threads
        for t_idx in 0..8 {
            let db_clone = db.clone();
            handles.push(thread::spawn(move || {
                for i in 0..20 {
                    let song_id = t_idx * 100 + i;
                    let success = db_clone.upsert(
                        song_id,
                        Mode::B4,
                        Difficulty::MX,
                        99.50 + (i as f64 * 0.01),
                        true,
                        false,
                    );
                    assert!(
                        success,
                        "Thread {} failed to upsert song {}",
                        t_idx, song_id
                    );
                }
            }));
        }

        // 4 concurrent reader threads
        for _ in 0..4 {
            let db_clone = db.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..20 {
                    let map = db_clone.get_rate_map(&[1, 2, 100, 200, 300]);
                    let _ = map.len();
                    thread::sleep(std::time::Duration::from_millis(1));
                }
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        let all_rows = db.all_records_for_steam("76561198000000001");
        assert_eq!(all_rows.len(), 8 * 20);
    }

    #[test]
    fn test_get_updated_at_map() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_updated_at.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let before = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        assert!(db.upsert(101, Mode::B4, Difficulty::NM, 98.5, false, false));
        assert!(db.upsert(102, Mode::B6, Difficulty::HD, 99.0, true, false));

        let map = db.get_updated_at_map(&[101, 102, 999]);
        assert_eq!(map.len(), 2);
        let updated_101 = map.get(&(101, Mode::B4, Difficulty::NM)).copied().unwrap();
        let updated_102 = map.get(&(102, Mode::B6, Difficulty::HD)).copied().unwrap();
        assert!(updated_101 >= before);
        assert!(updated_102 >= before);
    }

    #[test]
    fn test_get_varchive_top50_summary_and_rating_map() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_varchive_top50.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let conn = db.open_conn().unwrap();
        for i in 1..=55 {
            let json = serde_json::json!({
                "score": 99.0 + (i as f64 * 0.01),
                "maxCombo": true,
                "updatedAt": "2026-08-21T00:00:00.000Z",
                "rating": 100.0 + (i as f64 * 1.0),
            });
            conn.execute(
                "INSERT INTO varchive_records (steam_id, song_id, button_mode, difficulty, raw_data)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    "76561198000000001",
                    i.to_string(),
                    "4B",
                    "SC",
                    json.to_string(),
                ],
            ).unwrap();
        }

        let summary = db.get_varchive_top50_summary("76561198000000001", Mode::B4);
        assert_eq!(summary.total_recorded_count, 50);
        assert_eq!(summary.rank_map.len(), 50);
        assert_eq!(
            summary.rank_map.get(&(55, Mode::B4, Difficulty::SC)),
            Some(&1)
        );
        assert_eq!(
            summary.rank_map.get(&(6, Mode::B4, Difficulty::SC)),
            Some(&50)
        );
        assert_eq!(summary.cutoff_rating, 106.0);
        assert_eq!(summary.rank_map.get(&(5, Mode::B4, Difficulty::SC)), None);

        let rating_map = db.get_varchive_rating_map(&[55, 6, 5, 999]);
        assert_eq!(rating_map.len(), 3);
        assert_eq!(
            rating_map.get(&(55, Mode::B4, Difficulty::SC)),
            Some(&155.0)
        );
        assert_eq!(rating_map.get(&(6, Mode::B4, Difficulty::SC)), Some(&106.0));
        assert_eq!(rating_map.get(&(5, Mode::B4, Difficulty::SC)), Some(&105.0));
    }

    #[test]
    fn test_get_recent_records() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_recent.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let conn = db.open_conn().unwrap();
        for i in 1..=5 {
            conn.execute(
                "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "76561198000000001",
                    i.to_string(),
                    "4B",
                    "NM",
                    98.0 + (i as f64 * 0.2),
                    1,
                    1000 + i * 10,
                ],
            ).unwrap();
        }

        let recent = db.get_recent_records("76561198000000001", Mode::B4, 3);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].song_id, 5);
        assert_eq!(recent[0].updated_at, 1050);
        assert_eq!(recent[1].song_id, 4);
        assert_eq!(recent[2].song_id, 3);
    }

    #[test]
    fn test_play_events_logging_and_deduplication() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_play_events.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let event1 = VerifiedPlayEvent {
            song_id: 100,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate: 98.5,
            is_max_combo: true,
            is_result_screen: true,
        };

        // 1. 정상 결과창 이벤트 로깅 성공
        assert!(db.insert_play_event(&event1, 1000));

        // 2. 45초 이내 동일 패턴 재진입 (레벨업 팝업 가림 후 복귀) -> 디바운스 차단 (false)
        let dup_event = VerifiedPlayEvent {
            song_id: 100,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate: 98.5,
            is_max_combo: true,
            is_result_screen: true,
        };
        assert!(!db.insert_play_event(&dup_event, 1010)); // 10초 후 재진입 시도

        // 3. 45초 초과 후 다음 판 플레이 -> 삽입 성공
        let next_event = VerifiedPlayEvent {
            song_id: 100,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate: 99.0,
            is_max_combo: true,
            is_result_screen: true,
        };
        assert!(db.insert_play_event(&next_event, 1060)); // 60초 후 플레이

        // 4. 선곡 화면 이벤트는 play_events 삽입 거부
        let song_select_event = VerifiedPlayEvent {
            song_id: 101,
            mode: Mode::B4,
            diff: Difficulty::NM,
            rate: 99.9,
            is_max_combo: true,
            is_result_screen: false,
        };
        assert!(!db.insert_play_event(&song_select_event, 1100));

        // 5. MAX COMBO 연출 지연으로 45초 내에 뒤늦게 is_max_combo = true가 들어온 경우: In-place UPDATE 성공
        // 처음 1200초에 MC false로 들어갔다고 가정할 때, 1201초에 MC true로 오면 update 성공
        let pre_mc_event = VerifiedPlayEvent {
            song_id: 200,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate: 98.0,
            is_max_combo: false,
            is_result_screen: true,
        };
        assert!(db.insert_play_event(&pre_mc_event, 1200));
        let post_mc_event = VerifiedPlayEvent {
            song_id: 200,
            mode: Mode::B4,
            diff: Difficulty::SC,
            rate: 98.0,
            is_max_combo: true,
            is_result_screen: true,
        };
        assert!(db.insert_play_event(&post_mc_event, 1201)); // In-place UPDATE로 true 반환

        // 6. get_recent_records가 play_events에서 최신 상태를 조회하는지 검증 (MC가 1로 갱신됨)
        let recent = db.get_recent_records("76561198000000001", Mode::B4, 10);
        assert_eq!(recent.len(), 3);
        assert_eq!(recent[0].song_id, 200);
        assert!(recent[0].is_max_combo); // MC가 true로 갱신되었음
        assert_eq!(recent[1].song_id, 100);
        assert_eq!(recent[1].updated_at, 1060);
        assert_eq!(recent[2].song_id, 100);
        assert_eq!(recent[2].updated_at, 1000);
    }

    /// play_events가 limit 미만일 때 records로 보충하되 중복 제거하는 fallback 검증.
    ///
    /// 테스트 시나리오:
    /// play_events(실제 완주 로그)만을 100% 조회하며,
    /// 선곡창 휠 스크롤로 오염될 수 있는 records 테이블의 허위 레코드는 끌어오지 않음을 검증한다.
    #[test]
    fn test_recent_records_exclusive_play_events_without_records_pollution() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_fallback.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let conn = db.open_conn().unwrap();

        // records 테이블: 5건 (갱신 시각 기준 정렬)
        // 곡 100(SC)은 play_events에도 존재 → 중복 제거 대상
        for &(sid, diff, rate, ts) in &[
            (100, "SC", 97.5, 900), // play_events와 중복 (스킵 대상)
            (200, "NM", 96.0, 800),
            (201, "NM", 95.5, 700),
            (202, "NM", 95.0, 600),
            (203, "NM", 94.0, 500),
        ] {
            conn.execute(
                "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "76561198000000001",
                    sid.to_string(),
                    "4B",
                    diff,
                    rate,
                    0,
                    ts,
                ],
            )
            .unwrap();
        }

        // play_events 테이블: 2건 (실제 플레이 이력)
        for (sid, diff, rate, ts) in [(100, "SC", 98.5, 2000), (101, "SC", 97.0, 1900)] {
            conn.execute(
                "INSERT INTO play_events (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, played_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                rusqlite::params![
                    "76561198000000001",
                    sid.to_string(),
                    "4B",
                    diff,
                    rate,
                    0,
                    ts,
                ],
            )
            .unwrap();
        }
        drop(conn);

        // play_events 테이블의 실제 완주 기록만 정확하게 반환함을 검증
        // records 테이블(선곡창 휠 스크롤 오염)은 세션 이력으로 끌어오지 않음
        let recent = db.get_recent_records("76561198000000001", Mode::B4, 5);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].song_id, 100);
        assert_eq!(recent[0].updated_at, 2000); // play_events의 played_at
        assert_eq!(recent[1].song_id, 101);
        assert_eq!(recent[1].updated_at, 1900);

        // limit=1일 때 최신 1건만 반환
        let recent_small = db.get_recent_records("76561198000000001", Mode::B4, 1);
        assert_eq!(recent_small.len(), 1);
        assert_eq!(recent_small[0].song_id, 100);

        // play_events가 없는 모드는 0건 반환 (records 테이블의 허위 레코드를 끌어오지 않음)
        let recent_6b = db.get_recent_records("76561198000000001", Mode::B6, 5);
        assert_eq!(recent_6b.len(), 0);
    }

    #[test]
    fn test_song_select_browsing_preserves_records_updated_at() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("record_updated_at.db");
        let mut db = RecordDB::new(&db_path, Some("76561198000000001"));
        assert!(db.initialize());

        let conn = db.open_conn().unwrap();
        let old_timestamp = 1700000000i64; // 과거 시각

        // 1. 과거 기록 삽입 (rate: 98.5%, is_max_combo: 0, updated_at: 1700000000)
        conn.execute(
            "INSERT INTO records (steam_id, song_id, button_mode, difficulty, rate, is_max_combo, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            rusqlite::params![
                "76561198000000001",
                "100",
                "4B",
                "SC",
                98.5,
                0,
                old_timestamp,
            ],
        )
        .unwrap();
        drop(conn);

        // 2. 선곡창(only_if_improved = false)에서 동일한 값으로 upsert 호출
        let updated = db.upsert(100, Mode::B4, Difficulty::SC, 98.5, false, false);
        assert!(!updated, "동일한 기록 재방문 시 쓰기를 스킵해야 함");

        // 3. updated_at이 과거 시각 그대로 보존되었는지 검증
        let conn = db.open_conn().unwrap();
        let current_ts: i64 = conn
            .query_row(
                "SELECT updated_at FROM records WHERE steam_id = '76561198000000001' AND song_id = '100'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(
            current_ts, old_timestamp,
            "선곡창 탐색으로 기존 updated_at이 오염되지 않아야 함"
        );

        // 4. 점수가 개선된 경우(98.5 -> 99.0)에는 updated_at이 현재로 갱신됨을 검증
        let improved = db.upsert(100, Mode::B4, Difficulty::SC, 99.0, false, false);
        assert!(improved);

        let new_ts: i64 = conn
            .query_row(
                "SELECT updated_at FROM records WHERE steam_id = '76561198000000001' AND song_id = '100'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(
            new_ts > old_timestamp,
            "실제 점수 개선 시에는 updated_at이 갱신되어야 함"
        );
    }
}
