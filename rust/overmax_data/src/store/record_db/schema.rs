//! `RecordDB`의 스키마 생성·마이그레이션 책임.
//! 테이블 생성, 컬럼 존재 검사, JSON 캐시 이관을 담당한다.
use super::*;

impl RecordDB {
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
}
