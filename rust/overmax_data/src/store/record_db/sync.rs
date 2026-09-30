//! `RecordDB`의 V-Archive 동기화 병합 책임.
use super::*;

impl RecordDB {
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

        // 순서 주의: 증분 동기화(clear_first=true)는 기존 행을 전부 지운 뒤 새
        // 목록으로 대체한다. 서버가 일시적으로 빈 목록을 주면 공식 Top-50
        // 랭크/레이팅이 통째로 소실되므로, 목록 추출을 먼저 수행하고 빈 배열이면
        // 기존 캐시를 그대로 보존한다.
        let new_records = data
            .get("records")
            .and_then(|r| r.as_array())
            .ok_or_else(|| "records field missing or not an array".to_string())?;

        if new_records.is_empty() {
            return Ok(());
        }

        if clear_first {
            tx.execute(
                "DELETE FROM varchive_records WHERE steam_id = ?1 AND button_mode = ?2",
                params![steam_id, button_mode],
            )
            .map_err(|e| e.to_string())?;
        }

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
}
