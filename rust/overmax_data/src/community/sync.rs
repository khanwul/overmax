//! Local record vs V-Archive cached API records — mirrors `data/sync_manager.py`.

use crate::community::client::VArchiveDB;
use crate::store::record_db::RecordDB;
use overmax_core::{RecordKey, RecordValue};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct SyncCandidate {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub dlc: String,
    pub button_mode: String,
    pub difficulty: String,
    pub overmax_rate: f64,
    pub overmax_mc: bool,
    pub varchive_rate: Option<f64>,
    pub varchive_mc: Option<bool>,
    pub upload_status: String,
    pub upload_message: String,
}

impl SyncCandidate {
    pub fn key(&self) -> RecordKey {
        (
            self.song_id,
            self.button_mode.clone(),
            self.difficulty.clone(),
        )
    }

    pub fn key_ref(&self) -> (i32, &str, &str) {
        (self.song_id, &self.button_mode, &self.difficulty)
    }

    pub fn matches_key(&self, key: &RecordKey) -> bool {
        self.song_id == key.0 && self.button_mode == key.1 && self.difficulty == key.2
    }

    pub fn reason_label(&self) -> String {
        let mut parts = Vec::new();
        if self.varchive_rate.is_none() {
            parts.push("미등록".to_string());
        } else if self.overmax_rate > self.varchive_rate.unwrap_or(0.0) {
            parts.push(format!(
                "+{:.2}%",
                self.overmax_rate - self.varchive_rate.unwrap_or(0.0)
            ));
        }
        if self.overmax_rate >= 100.0 {
            parts.push("P".to_string());
        } else if self.overmax_mc && !self.varchive_mc.unwrap_or(false) {
            parts.push("M".to_string());
        }
        parts.join(" · ")
    }
}

/// Loads `cache/varchive/{steam_id}/{4|5|6|8}.json` into a lookup map.
pub fn load_varchive_record_cache(
    cache_root: &Path,
    steam_id: &str,
) -> HashMap<RecordKey, RecordValue> {
    let mut cache = HashMap::new();
    if steam_id.is_empty() || steam_id == "__unknown__" {
        return cache;
    }
    let user_dir = cache_root.join(steam_id);
    for button in [4i32, 5, 6, 8] {
        let button_mode = format!("{button}B");
        let path = user_dir.join(format!("{button}.json"));
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        let Ok(root) = serde_json::from_str::<Value>(&text) else {
            continue;
        };
        let Some(records) = root.get("records").and_then(|v| v.as_array()) else {
            continue;
        };
        merge_record_entries(&mut cache, records, &button_mode);
    }
    cache
}

fn merge_record_entries(
    cache: &mut HashMap<RecordKey, RecordValue>,
    records: &[Value],
    button_mode: &str,
) {
    for rec in records {
        let Some(title) = rec.get("title") else {
            continue;
        };
        let Some(song_id) = parse_song_id(title) else {
            continue;
        };
        let Some(diff) = rec.get("pattern").and_then(|v| v.as_str()) else {
            continue;
        };
        let rate_f64 = rec
            .get("score")
            .and_then(|v| v.as_f64())
            .or_else(|| rec.get("score").and_then(|v| v.as_i64()).map(|i| i as f64))
            .unwrap_or(0.0);
        let rate = rate_f64 as f32;
        let is_max_combo = rec
            .get("maxCombo")
            .and_then(|v| v.as_bool())
            .or_else(|| rec.get("maxCombo").and_then(|v| v.as_u64()).map(|n| n != 0))
            .unwrap_or(false);
        cache.insert(
            (song_id, button_mode.to_string(), diff.to_string()),
            (rate, is_max_combo),
        );
    }
}

fn parse_song_id(title: &Value) -> Option<i32> {
    match title {
        Value::Number(n) => n.as_i64().and_then(|v| i32::try_from(v).ok()),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn sort_key(c: &SyncCandidate) -> (i8, f64) {
    match c.varchive_rate {
        None => (1, -c.overmax_rate),
        Some(vr) => {
            let diff = c.overmax_rate - vr;
            if diff > 0.0 {
                (0, -diff)
            } else {
                (2, 0.0)
            }
        }
    }
}

/// Builds sync candidates for one Steam id using SQL LEFT JOIN on local `record.db`.
pub fn build_candidates(
    varchive_db: &VArchiveDB,
    record_db: &RecordDB,
    steam_id: &str,
    _varchive_cache_root: &Path,
) -> Vec<SyncCandidate> {
    let raw_rows = record_db.query_sync_candidates(steam_id);
    if raw_rows.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::with_capacity(raw_rows.len());

    for row in raw_rows {
        let (song_name, composer, dlc) = match varchive_db.search_by_id(row.song_id) {
            Some(s) => (
                s.name.clone(),
                s.composer.to_string(),
                s.dlc_code.to_string(),
            ),
            None => (row.song_id.to_string(), String::new(), String::new()),
        };

        candidates.push(SyncCandidate {
            song_id: row.song_id,
            song_name,
            composer,
            dlc,
            button_mode: row.button_mode,
            difficulty: row.difficulty,
            overmax_rate: row.local_rate,
            overmax_mc: row.local_mc,
            varchive_rate: row.varchive_rate,
            varchive_mc: row.varchive_mc,
            upload_status: String::new(),
            upload_message: String::new(),
        });
    }

    candidates.sort_by(|a, b| {
        sort_key(a)
            .partial_cmp(&sort_key(b))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    candidates
}

/// Merges one score into `cache/varchive/{steam_id}/{button}.json` (same shape as Python client).
pub fn upsert_varchive_cache_record(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
    song_id: i32,
    difficulty: &str,
    score: f64,
    is_max_combo: bool,
) -> Result<(), String> {
    let user_dir = cache_root.join(steam_id);
    fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join(format!("{button}.json"));

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "records": [] }))
    } else {
        json!({ "records": [] })
    };

    let records = root
        .as_object_mut()
        .and_then(|m| {
            m.entry("records".to_string())
                .or_insert_with(|| json!([]))
                .as_array_mut()
        })
        .ok_or_else(|| "invalid cache shape".to_string())?;

    let title = song_id.to_string();
    let mut updated = false;
    for rec in records.iter_mut() {
        let Some(obj) = rec.as_object_mut() else {
            continue;
        };
        let title_match = match obj.get("title") {
            Some(Value::String(s)) => s == &title,
            Some(Value::Number(n)) => n.as_i64() == Some(song_id as i64),
            _ => false,
        };
        let pat_match = obj.get("pattern").and_then(|v| v.as_str()) == Some(difficulty);
        if title_match && pat_match {
            obj.insert("score".into(), json!(score));
            obj.insert("maxCombo".into(), json!(is_max_combo));
            updated = true;
            break;
        }
    }
    if !updated {
        records.push(json!({
            "title": title,
            "pattern": difficulty,
            "score": score,
            "maxCombo": is_max_combo,
        }));
    }

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn save_fetched_records_to_cache(
    cache_root: &Path,
    steam_id: &str,
    v_id: &str,
    button: i32,
    data: &Value,
) -> Result<(), String> {
    let user_dir = cache_root.join(steam_id);
    fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join(format!("{button}.json"));

    let records = data.get("records").cloned().unwrap_or_else(|| json!([]));
    let updated_at = data
        .get("user")
        .and_then(|u| u.get("updated_at"))
        .cloned()
        .unwrap_or(json!(null));

    let cache_data = json!({
        "v_id": v_id,
        "button": button,
        "records": records,
        "updated_at": updated_at,
    });

    let text = serde_json::to_string_pretty(&cache_data).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

pub fn delete_varchive_cache_record(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
    song_id: i32,
    difficulty: &str,
) -> Result<(), String> {
    let path = cache_root.join(steam_id).join(format!("{button}.json"));
    if !path.exists() {
        return Ok(());
    }

    let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let mut root = serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?;

    if let Some(obj) = root.as_object_mut() {
        if let Some(records) = obj.get_mut("records").and_then(|r| r.as_array_mut()) {
            let title = song_id.to_string();
            records.retain(|rec| {
                if let Some(rec_obj) = rec.as_object() {
                    let title_match = match rec_obj.get("title") {
                        Some(Value::String(s)) => s == &title,
                        Some(Value::Number(n)) => n.as_i64() == Some(song_id as i64),
                        _ => false,
                    };
                    let pat_match =
                        rec_obj.get("pattern").and_then(|v| v.as_str()) == Some(difficulty);
                    !(title_match && pat_match)
                } else {
                    true
                }
            });
        }
    }

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

/// Merges fetched records (e.g., from a single song API fetch or full fetch) into the local cache.
/// For each record in `data["records"]`, it upserts it into the local cache file.
pub fn merge_fetched_records_to_cache(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
    data: &Value,
) -> Result<(), String> {
    let user_dir = cache_root.join(steam_id);
    fs::create_dir_all(&user_dir).map_err(|e| e.to_string())?;
    let path = user_dir.join(format!("{button}.json"));

    let mut root = if path.exists() {
        let text = fs::read_to_string(&path).map_err(|e| e.to_string())?;
        serde_json::from_str::<Value>(&text).unwrap_or_else(|_| json!({ "records": [] }))
    } else {
        json!({ "records": [] })
    };

    let existing_records = root
        .as_object_mut()
        .and_then(|m| {
            m.entry("records".to_string())
                .or_insert_with(|| json!([]))
                .as_array_mut()
        })
        .ok_or_else(|| "invalid cache shape".to_string())?;

    let new_records = data
        .get("records")
        .and_then(|r| r.as_array())
        .ok_or_else(|| "invalid input data: records field missing or not an array".to_string())?;

    for new_rec in new_records {
        let Some(new_obj) = new_rec.as_object() else {
            continue;
        };

        let new_title = new_obj.get("title").and_then(|v| match v {
            Value::String(s) => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
        let new_pattern = new_obj.get("pattern").and_then(|v| v.as_str());

        if let (Some(t), Some(p)) = (new_title, new_pattern) {
            let mut updated = false;
            for rec in existing_records.iter_mut() {
                let Some(obj) = rec.as_object_mut() else {
                    continue;
                };
                let title_match = match obj.get("title") {
                    Some(Value::String(s)) => s == &t,
                    Some(Value::Number(n)) => n.to_string() == t,
                    _ => false,
                };
                let pat_match = obj.get("pattern").and_then(|v| v.as_str()) == Some(p);
                if title_match && pat_match {
                    // Replace the whole record object with the new one
                    *rec = new_rec.clone();
                    updated = true;
                    break;
                }
            }
            if !updated {
                existing_records.push(new_rec.clone());
            }
        }
    }

    // Update user.updated_at if present in the data
    if let Some(new_updated_at) = data.get("user").and_then(|u| u.get("updated_at")) {
        if let Some(obj) = root.as_object_mut() {
            obj.insert("updated_at".to_string(), new_updated_at.clone());
        }
    } else if let Some(new_updated_at) = data.get("updated_at") {
        if let Some(obj) = root.as_object_mut() {
            obj.insert("updated_at".to_string(), new_updated_at.clone());
        }
    }

    let text = serde_json::to_string_pretty(&root).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())
}

/// Scans the local JSON cache file and returns the latest updatedAt ISO timestamp string.
pub fn get_latest_updated_at_from_cache(
    cache_root: &Path,
    steam_id: &str,
    button: i32,
) -> Option<String> {
    let path = cache_root.join(steam_id).join(format!("{button}.json"));
    if !path.exists() {
        return None;
    }
    let text = fs::read_to_string(&path).ok()?;
    let root: Value = serde_json::from_str(&text).ok()?;
    let records = root.get("records")?.as_array()?;

    let mut latest: Option<String> = None;
    for rec in records {
        if let Some(updated_at) = rec.get("updatedAt").and_then(|v| v.as_str()) {
            if !updated_at.is_empty() {
                match &latest {
                    Some(curr) => {
                        if updated_at > curr {
                            latest = Some(updated_at.to_string());
                        }
                    }
                    None => {
                        latest = Some(updated_at.to_string());
                    }
                }
            }
        }
    }
    latest
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_merge_fetched_records_to_cache() {
        let dir = std::env::temp_dir().join(format!("varch-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();

        let initial_payload = json!({
            "user": {
                "v_id": "test_user",
                "updated_at": "2026-07-16T12:00:00Z"
            },
            "records": [
                {"title": "100", "pattern": "MX", "score": 98.5, "maxCombo": false}
            ]
        });

        // 1. Initial merge (creates file)
        merge_fetched_records_to_cache(&dir, "765611", 6, &initial_payload).unwrap();

        // 2. Merge another song
        let next_payload = json!({
            "user": {
                "v_id": "test_user",
                "updated_at": "2026-07-16T13:00:00Z"
            },
            "records": [
                {"title": "101", "pattern": "NM", "score": 99.9, "maxCombo": true}
            ]
        });
        merge_fetched_records_to_cache(&dir, "765611", 6, &next_payload).unwrap();

        // 3. Upsert existing song and change its score
        let update_payload = json!({
            "user": {
                "v_id": "test_user",
                "updated_at": "2026-07-16T14:00:00Z"
            },
            "records": [
                {"title": "100", "pattern": "MX", "score": 100.0, "maxCombo": true}
            ]
        });
        merge_fetched_records_to_cache(&dir, "765611", 6, &update_payload).unwrap();

        // Load and assert
        let m = load_varchive_record_cache(&dir, "765611");
        // song 100 MX should be updated to 100.0, true
        assert_eq!(
            m.get(&(100, "6B".into(), "MX".into())),
            Some(&(100.0, true))
        );
        // song 101 NM should be preserved as 99.9, true
        assert_eq!(m.get(&(101, "6B".into(), "NM".into())), Some(&(99.9, true)));

        // Read raw JSON to check updated_at
        let path = dir.join("765611").join("6.json");
        let text = std::fs::read_to_string(path).unwrap();
        let val: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(
            val.get("updated_at").and_then(|v| v.as_str()),
            Some("2026-07-16T14:00:00Z")
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_get_latest_updated_at_from_cache() {
        let dir = std::env::temp_dir().join(format!("varch-latest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();

        // 1. If file does not exist, should return None
        assert_eq!(get_latest_updated_at_from_cache(&dir, "765611", 6), None);

        // 2. Normal case with various updatedAt dates
        let payload = json!({
            "records": [
                {"title": "1", "pattern": "NM", "score": 99.0, "maxCombo": true, "updatedAt": "2023-10-08T12:00:00.000Z"},
                {"title": "2", "pattern": "HD", "score": 98.0, "maxCombo": false, "updatedAt": "2023-10-09T15:30:00.000Z"},
                {"title": "3", "pattern": "MX", "score": 97.0, "maxCombo": true, "updatedAt": "2023-10-05T09:00:00.000Z"},
                {"title": "4", "pattern": "SC", "score": 96.0, "maxCombo": false, "updatedAt": ""} // Empty updatedAt
            ]
        });
        std::fs::write(dir.join("765611").join("6.json"), payload.to_string()).unwrap();

        // Should return the latest one: 2023-10-09T15:30:00.000Z
        assert_eq!(
            get_latest_updated_at_from_cache(&dir, "765611", 6),
            Some("2023-10-09T15:30:00.000Z".to_string())
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn parses_cache_file_like_python_client() {
        let dir = std::env::temp_dir().join(format!("varch-cache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();
        let payload = json!({
            "records": [
                {"title": "42", "pattern": "MX", "score": 99.5, "maxCombo": true}
            ]
        });
        std::fs::write(dir.join("765611").join("4.json"), payload.to_string()).unwrap();

        let m = load_varchive_record_cache(&dir, "765611");
        assert_eq!(m.get(&(42, "4B".into(), "MX".into())), Some(&(99.5, true)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn keeps_song_id_zero_from_varchive_cache() {
        let dir = std::env::temp_dir().join(format!("varch-cache-zero-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("765611")).unwrap();
        let payload = json!({
            "records": [
                {"title": "0", "pattern": "NM", "score": 88.0, "maxCombo": false}
            ]
        });
        std::fs::write(dir.join("765611").join("4.json"), payload.to_string()).unwrap();

        let m = load_varchive_record_cache(&dir, "765611");
        assert_eq!(m.get(&(0, "4B".into(), "NM".into())), Some(&(88.0, false)));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_build_candidates_filters_already_synced_records_from_sqlite_db() {
        let dir = std::env::temp_dir().join(format!("varch-build-cand-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let db_path = dir.join("record.db");
        let mut rdb = RecordDB::new(&db_path, Some("76561198000000000"));
        assert!(rdb.initialize());

        // 1. Insert local records into record.db
        rdb.upsert(1, "4B", "MX", 99.5, true, false); // Already synced identical score
        rdb.upsert(2, "4B", "MX", 98.0, false, false); // Local is 98.0%, V-Archive has 95.0% (Improved!)
        rdb.upsert(3, "4B", "MX", 99.0, false, false); // Unregistered on V-Archive

        // 2. Insert V-Archive records into SQLite table
        let v_payload = json!({
            "records": [
                {"title": "1", "pattern": "MX", "score": 99.5, "maxCombo": true},
                {"title": "2", "pattern": "MX", "score": 95.0, "maxCombo": false}
            ]
        });
        rdb.merge_varchive_fetched_records("76561198000000000", 4, &v_payload, false)
            .unwrap();

        let vdb = VArchiveDB::new();
        let candidates = build_candidates(&vdb, &rdb, "76561198000000000", &dir);

        // Candidate 1 (synced) should be FILTERED OUT!
        // Candidate 2 (improved score) and Candidate 3 (unregistered) should be PRESENT!
        assert_eq!(candidates.len(), 2);
        let ids: Vec<i32> = candidates.iter().map(|c| c.song_id).collect();
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&1));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
