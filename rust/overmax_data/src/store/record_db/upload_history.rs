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
    pub is_max_combo: bool,
    pub success: bool,
    pub message: String,
}

/// 사용자당 보관하는 최대 기록 수.
pub const UPLOAD_HISTORY_LIMIT: usize = 200;

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
                is_max_combo  INTEGER NOT NULL DEFAULT 0,
                success       INTEGER NOT NULL DEFAULT 0,
                message       TEXT NOT NULL DEFAULT ''
            )",
            [],
        )?;
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
                    (steam_id, uploaded_at, song_name, pattern, rate, is_max_combo, success, message)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    steam_id,
                    record.uploaded_at,
                    record.song_name,
                    record.pattern,
                    record.rate,
                    record.is_max_combo as i32,
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
                "SELECT uploaded_at, song_name, pattern, rate, is_max_combo, success, message
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
                    is_max_combo: row.get::<_, i32>(4)? != 0,
                    success: row.get::<_, i32>(5)? != 0,
                    message: row.get(6)?,
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
            is_max_combo: i.is_multiple_of(2),
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
