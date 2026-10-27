//! `upload_history` 테이블: 플레이어용 업로드 기록 창의 영속 저장소.
//! 사용자(steam_id)별로 최근 `UPLOAD_HISTORY_LIMIT`건만 유지한다. 기존 테이블은 건드리지 않는다.
use super::*;

/// 업로드 기록 한 건 (창 표시용 값).
#[derive(Clone, Debug, PartialEq)]
pub struct UploadHistoryRecord {
    pub uploaded_at: i64,
    pub song_name: String,
    pub pattern: String,
    pub rate: f64,
    /// 이전 V-Archive Rate 대비 상승폭 (이전 기록이 없었다면 None).
    pub rate_delta: Option<f64>,
    /// 업로드 후 V-Archive Top-50 안의 순위 (순위 밖이면 None).
    pub top50_rank: Option<usize>,
    pub is_max_combo: bool,
    /// 업로드 직전 V-Archive 기록의 MAX COMBO 여부. 이전 기록이 없었거나, 이 컬럼이 생기기 전에
    /// 저장된 기록이면 None.
    pub prev_max_combo: Option<bool>,
    pub success: bool,
    pub message: String,
}

/// 사용자당 보관하는 최대 기록 수.
pub const UPLOAD_HISTORY_LIMIT: usize = 200;

/// 기존 DB에 뒤늦게 추가된 컬럼 (이름, ADD COLUMN 구문). 새 DB는 CREATE TABLE에 이미 포함된다.
const UPLOAD_HISTORY_ADDED_COLUMNS: [(&str, &str); 3] = [
    (
        "rate_delta",
        "ALTER TABLE upload_history ADD COLUMN rate_delta REAL",
    ),
    (
        "top50_rank",
        "ALTER TABLE upload_history ADD COLUMN top50_rank INTEGER",
    ),
    (
        "prev_max_combo",
        "ALTER TABLE upload_history ADD COLUMN prev_max_combo INTEGER",
    ),
];

impl RecordDB {
    pub(super) fn create_upload_history_table(&self, conn: &Connection) -> Result<()> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS upload_history (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                steam_id      TEXT NOT NULL,
                uploaded_at   INTEGER NOT NULL,
                song_name     TEXT NOT NULL,
                pattern       TEXT NOT NULL,
                rate          REAL NOT NULL,
                rate_delta    REAL,
                top50_rank    INTEGER,
                is_max_combo  INTEGER NOT NULL DEFAULT 0,
                prev_max_combo INTEGER,
                success       INTEGER NOT NULL DEFAULT 0,
                message       TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;

        let existing_columns: Vec<String> = {
            let mut stmt = conn.prepare("PRAGMA table_info(upload_history)")?;
            let names = stmt
                .query_map([], |row| row.get::<_, String>(1))?
                .collect::<Result<Vec<_>>>()?;
            names
        };
        for (name, ddl) in UPLOAD_HISTORY_ADDED_COLUMNS {
            if !existing_columns.iter().any(|c| c == name) {
                conn.execute(ddl, [])?;
            }
        }

        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_upload_history_recent ON upload_history (steam_id, id DESC)",
            [],
        )?;
        Ok(())
    }

    /// 업로드 결과 한 건을 저장하고, 같은 사용자의 오래된 기록은 `UPLOAD_HISTORY_LIMIT`건만 남기고 정리한다.
    pub fn insert_upload_history(&self, record: &UploadHistoryRecord) -> bool {
        if !self.is_ready {
            return false;
        }
        let steam_id = self.get_steam_id();
        if steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return false;
        }

        self.with_retry(|conn| {
            let tx = conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO upload_history
                    (steam_id, uploaded_at, song_name, pattern, rate, rate_delta, top50_rank,
                     is_max_combo, prev_max_combo, success, message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    steam_id,
                    record.uploaded_at,
                    record.song_name,
                    record.pattern,
                    record.rate,
                    record.rate_delta,
                    record.top50_rank.map(|r| r as i64),
                    record.is_max_combo as i32,
                    record.prev_max_combo.map(|mc| mc as i32),
                    record.success as i32,
                    record.message
                ],
            )?;
            tx.execute(
                "DELETE FROM upload_history
                 WHERE steam_id = ?1
                   AND id NOT IN (
                       SELECT id FROM upload_history
                       WHERE steam_id = ?1
                       ORDER BY id DESC
                       LIMIT ?2
                   )",
                params![steam_id, UPLOAD_HISTORY_LIMIT as i64],
            )?;
            tx.commit()
        })
        .is_ok()
    }

    /// 현재 사용자의 업로드 기록을 최신순으로 최대 `UPLOAD_HISTORY_LIMIT`건 반환한다.
    pub fn load_upload_history(&self) -> Vec<UploadHistoryRecord> {
        let steam_id = self.get_steam_id();
        if !self.is_ready || steam_id.is_empty() || steam_id == Self::UNKNOWN_STEAM_ID {
            return Vec::new();
        }

        self.with_retry(|conn| {
            let mut stmt = conn.prepare_cached(
                "SELECT uploaded_at, song_name, pattern, rate, rate_delta, top50_rank,
                        is_max_combo, prev_max_combo, success, message
                 FROM upload_history
                 WHERE steam_id = ?1
                 ORDER BY id DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(params![steam_id, UPLOAD_HISTORY_LIMIT as i64], |row| {
                Ok(UploadHistoryRecord {
                    uploaded_at: row.get(0)?,
                    song_name: row.get(1)?,
                    pattern: row.get(2)?,
                    rate: row.get(3)?,
                    rate_delta: row.get(4)?,
                    top50_rank: row.get::<_, Option<i64>>(5)?.map(|r| r as usize),
                    is_max_combo: row.get::<_, i32>(6)? != 0,
                    prev_max_combo: row.get::<_, Option<i32>>(7)?.map(|mc| mc != 0),
                    success: row.get::<_, i32>(8)? != 0,
                    message: row.get(9)?,
                })
            })?;
            rows.collect::<Result<Vec<_>>>()
        })
        .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(i: usize) -> UploadHistoryRecord {
        UploadHistoryRecord {
            uploaded_at: 1_000 + i as i64,
            song_name: format!("song {i}"),
            pattern: "4B NM".to_string(),
            rate: 99.0,
            rate_delta: if i.is_multiple_of(3) { Some(0.5) } else { None },
            top50_rank: if i.is_multiple_of(5) { Some(7) } else { None },
            is_max_combo: i.is_multiple_of(2),
            prev_max_combo: if i.is_multiple_of(4) { Some(false) } else { None },
            success: true,
            message: format!("msg {i}"),
        }
    }

    #[test]
    fn upload_history_keeps_latest_first_and_prunes_to_limit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let mut db = RecordDB::new(
            temp_dir.path().join("upload_history.db"),
            Some("76561190000000000"),
        );
        assert!(db.initialize());

        for i in 0..UPLOAD_HISTORY_LIMIT + 5 {
            assert!(db.insert_upload_history(&sample(i)));
        }

        let loaded = db.load_upload_history();
        assert_eq!(loaded.len(), UPLOAD_HISTORY_LIMIT);
        assert_eq!(loaded[0], sample(UPLOAD_HISTORY_LIMIT + 4));
        assert_eq!(loaded[UPLOAD_HISTORY_LIMIT - 1], sample(5));
    }

    #[test]
    fn upload_history_migrates_table_without_prev_max_combo() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("upload_history_migrate.db");
        let steam_id = "76561190000000003";
        {
            // prev_max_combo 컬럼이 없던 이전 스키마로 한 건을 미리 넣어 둔다.
            let conn = Connection::open(&db_path).unwrap();
            conn.execute(
                "CREATE TABLE upload_history (
                    id            INTEGER PRIMARY KEY AUTOINCREMENT,
                    steam_id      TEXT NOT NULL,
                    uploaded_at   INTEGER NOT NULL,
                    song_name     TEXT NOT NULL,
                    pattern       TEXT NOT NULL,
                    rate          REAL NOT NULL,
                    rate_delta    REAL,
                    top50_rank    INTEGER,
                    is_max_combo  INTEGER NOT NULL DEFAULT 0,
                    success       INTEGER NOT NULL DEFAULT 0,
                    message       TEXT NOT NULL DEFAULT ''
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO upload_history (steam_id, uploaded_at, song_name, pattern, rate,
                     is_max_combo, success, message)
                 VALUES (?1, 1, 'old', '4B NM', 99.0, 1, 1, 'ok')",
                params![steam_id],
            )
            .unwrap();
        }

        let mut db = RecordDB::new(&db_path, Some(steam_id));
        assert!(db.initialize());
        let loaded = db.load_upload_history();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].prev_max_combo, None);
        assert!(loaded[0].is_max_combo);

        let mut new_record = sample(1);
        new_record.prev_max_combo = Some(true);
        assert!(db.insert_upload_history(&new_record));
        assert_eq!(db.load_upload_history()[0], new_record);
    }

    #[test]
    fn upload_history_is_scoped_per_steam_id() {
        let temp_dir = tempfile::tempdir().unwrap();
        let db_path = temp_dir.path().join("upload_history_scope.db");
        let mut db = RecordDB::new(&db_path, Some("76561190000000001"));
        assert!(db.initialize());
        assert!(db.insert_upload_history(&sample(1)));

        db.set_steam_id(Some("76561190000000002"));
        assert!(db.load_upload_history().is_empty());
        assert!(db.insert_upload_history(&sample(2)));
        assert_eq!(db.load_upload_history(), vec![sample(2)]);

        db.set_steam_id(Some("76561190000000001"));
        assert_eq!(db.load_upload_history(), vec![sample(1)]);
    }
}
