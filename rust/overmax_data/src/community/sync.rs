//! Local record vs V-Archive cached API records — mirrors `data/sync_manager.py`.

use crate::community::client::VArchiveDB;
use crate::store::record_db::RecordDB;
use overmax_core::RecordKey;

#[derive(Debug, Clone)]
pub struct SyncCandidate {
    pub song_id: i32,
    pub song_name: String,
    pub composer: String,
    pub dlc: String,
    pub button_mode: String,
    pub difficulty: String,
    pub pattern_level: Option<u32>,
    pub overmax_rate: f64,
    pub overmax_mc: bool,
    pub varchive_rate: Option<f64>,
    pub varchive_mc: Option<bool>,
    pub upload_status: String,
    pub upload_message: String,
}

pub const LEVEL_LABELS: [&str; 30] = [
    "1", "2", "3", "4", "5", "6", "7", "8", "9", "10", "11", "12", "13", "14", "15", "SC1", "SC2",
    "SC3", "SC4", "SC5", "SC6", "SC7", "SC8", "SC9", "SC10", "SC11", "SC12", "SC13", "SC14",
    "SC15",
];

pub fn pattern_level_index(difficulty: &str, level: Option<u32>) -> Option<usize> {
    let lvl = level?;
    if lvl == 0 || lvl > 15 {
        return None;
    }
    if difficulty == "SC" {
        Some(15 + (lvl as usize - 1))
    } else {
        Some(lvl as usize - 1)
    }
}

pub fn matches_filter(
    c: &SyncCandidate,
    filter: &crate::config::settings::SyncFilterSettings,
) -> bool {
    let mode_ok = match c.button_mode.as_str() {
        "4B" => filter.mode_4b,
        "5B" => filter.mode_5b,
        "6B" => filter.mode_6b,
        "8B" => filter.mode_8b,
        _ => true,
    };
    if !mode_ok {
        return false;
    }

    let diff_ok = match c.difficulty.as_str() {
        "NM" => filter.diff_nm,
        "HD" => filter.diff_hd,
        "MX" => filter.diff_mx,
        "SC" => filter.diff_sc,
        _ => true,
    };
    if !diff_ok {
        return false;
    }

    if let Some(lvl_idx) = pattern_level_index(&c.difficulty, c.pattern_level) {
        if lvl_idx < filter.min_level_idx || lvl_idx > filter.max_level_idx {
            return false;
        }
    }

    if c.overmax_rate < filter.min_rate || c.overmax_rate > filter.max_rate {
        return false;
    }

    if filter.require_mc_not_on_varchive {
        if !c.overmax_mc || c.varchive_mc == Some(true) {
            return false;
        }
    }

    if filter.exclude_unuploaded {
        if c.varchive_rate.is_none() {
            return false;
        }
    }

    true
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
) -> Vec<SyncCandidate> {
    let raw_rows = record_db.query_sync_candidates(steam_id);
    if raw_rows.is_empty() {
        return Vec::new();
    }
    let mut candidates = Vec::with_capacity(raw_rows.len());

    for row in raw_rows {
        let (song_name, composer, dlc, pattern_level) = match varchive_db.search_by_id(row.song_id)
        {
            Some(s) => (
                s.name.clone(),
                s.composer.to_string(),
                s.dlc_code.to_string(),
                s.get_pattern(&row.button_mode, &row.difficulty)
                    .and_then(|p| p.level),
            ),
            None => (row.song_id.to_string(), String::new(), String::new(), None),
        };

        candidates.push(SyncCandidate {
            song_id: row.song_id,
            song_name,
            composer,
            dlc,
            button_mode: row.button_mode,
            difficulty: row.difficulty,
            pattern_level,
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        let candidates = build_candidates(&vdb, &rdb, "76561198000000000");

        // Candidate 1 (synced) should be FILTERED OUT!
        // Candidate 2 (improved score) and Candidate 3 (unregistered) should be PRESENT!
        assert_eq!(candidates.len(), 2);
        let ids: Vec<i32> = candidates.iter().map(|c| c.song_id).collect();
        assert!(ids.contains(&2));
        assert!(ids.contains(&3));
        assert!(!ids.contains(&1));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_sync_filter_matching() {
        use crate::config::settings::SyncFilterSettings;

        let cand1 = SyncCandidate {
            song_id: 1,
            song_name: "Test 1".to_string(),
            composer: "Comp".to_string(),
            dlc: "R".to_string(),
            button_mode: "4B".to_string(),
            difficulty: "SC".to_string(),
            pattern_level: Some(5), // SC5 -> level index 19
            overmax_rate: 99.5,
            overmax_mc: true,
            varchive_rate: Some(98.0),
            varchive_mc: Some(false), // Local MC true, V-Archive MC false
            upload_status: String::new(),
            upload_message: String::new(),
        };

        let cand2 = SyncCandidate {
            song_id: 2,
            song_name: "Test 2".to_string(),
            composer: "Comp".to_string(),
            dlc: "R".to_string(),
            button_mode: "8B".to_string(),
            difficulty: "MX".to_string(),
            pattern_level: Some(14), // MX 14 -> level index 13
            overmax_rate: 97.0,
            overmax_mc: false,
            varchive_rate: None, // Unregistered
            varchive_mc: None,
            upload_status: String::new(),
            upload_message: String::new(),
        };

        let mut filter = SyncFilterSettings::default();
        assert!(matches_filter(&cand1, &filter));
        assert!(matches_filter(&cand2, &filter));

        // Test Mode Filter
        filter.mode_4b = false;
        assert!(!matches_filter(&cand1, &filter));
        assert!(matches_filter(&cand2, &filter));
        filter.mode_4b = true;

        // Test Diff Filter
        filter.diff_mx = false;
        assert!(matches_filter(&cand1, &filter));
        assert!(!matches_filter(&cand2, &filter));
        filter.diff_mx = true;

        // Test Level Filter
        filter.min_level_idx = 15; // SC1 ~ SC15
        filter.max_level_idx = 29;
        assert!(matches_filter(&cand1, &filter)); // SC5 is index 19
        assert!(!matches_filter(&cand2, &filter)); // MX14 is index 13
        filter = SyncFilterSettings::default();

        // Test Rate Filter
        filter.min_rate = 98.0;
        assert!(matches_filter(&cand1, &filter));
        assert!(!matches_filter(&cand2, &filter));
        filter = SyncFilterSettings::default();

        // Test MC filter
        filter.require_mc_not_on_varchive = true;
        assert!(matches_filter(&cand1, &filter)); // cand1 has local MC true, varchive MC false
        assert!(!matches_filter(&cand2, &filter)); // cand2 local MC is false
        filter = SyncFilterSettings::default();

        // Test Exclude Unuploaded filter
        filter.exclude_unuploaded = true;
        assert!(matches_filter(&cand1, &filter)); // cand1 varchive_rate is Some(98.0)
        assert!(!matches_filter(&cand2, &filter)); // cand2 varchive_rate is None
    }
}
