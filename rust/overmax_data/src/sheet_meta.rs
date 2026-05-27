use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

use crate::varchive::{Difficulty, Mode};

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum GoldMeta {
    #[default]
    #[serde(rename = "")]
    None,
    #[serde(rename = "핲랜")]
    HalfRandom,
    #[serde(rename = "맥랜")]
    MaxRandom,
    #[serde(rename = "랜덤")]
    Random,
}

impl GoldMeta {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::HalfRandom => "핲랜",
            Self::MaxRandom => "맥랜",
            Self::Random => "랜덤",
        }
    }
}

#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistMeta {
    #[default]
    #[serde(rename = "")]
    None,
    #[serde(rename = "사용")]
    Used,
    #[serde(rename = "주의")]
    Caution,
    #[serde(rename = "미사용")]
    NotUsed,
}

impl AssistMeta {
    pub fn is_none(&self) -> bool {
        matches!(self, Self::None)
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "",
            Self::Used => "사용",
            Self::Caution => "주의",
            Self::NotUsed => "미사용",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatternSheetMetaItem {
    #[serde(default, skip_serializing_if = "GoldMeta::is_none")]
    pub gold: GoldMeta,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub note: String,
    #[serde(default, skip_serializing_if = "AssistMeta::is_none")]
    pub assist_key: AssistMeta,
    #[serde(default, skip_serializing_if = "is_false")]
    pub keypart: bool,
}

fn is_false(val: &bool) -> bool {
    !*val
}

/// JSON Array의 개별 요소. 파일 저장/로드에 사용됩니다.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PatternMetaEntry {
    pub song_id: String,
    pub mode: Mode,
    pub diff: Difficulty,
    #[serde(flatten)]
    pub meta: PatternSheetMetaItem,
}

type LookupKey = (String, Mode, Difficulty);

#[derive(Clone, Debug, Default)]
pub struct PatternSheetMeta {
    items: HashMap<LookupKey, PatternSheetMetaItem>,
}

impl PatternSheetMeta {
    pub fn load_cache(path: impl AsRef<Path>, varchive_db: &crate::varchive::VArchiveDB) -> Self {
        let path_ref = path.as_ref();
        let Ok(text) = std::fs::read_to_string(path_ref) else {
            return Self::default();
        };

        // 신 포맷: JSON Array
        if let Ok(entries) = serde_json::from_str::<Vec<PatternMetaEntry>>(&text) {
            let items = entries
                .into_iter()
                .map(|e| ((e.song_id, e.mode, e.diff), e.meta))
                .collect();
            return Self { items };
        }

        // 구 포맷 fallback: mode|title|diff 키를 쓰는 JSON Object (유저 로컬 파일)
        let raw_items: HashMap<String, PatternSheetMetaItem> =
            serde_json::from_str(&text).unwrap_or_default();
        let mut items: HashMap<LookupKey, PatternSheetMetaItem> = HashMap::new();

        for (key, item) in raw_items {
            let parts: Vec<&str> = key.split('|').collect();
            if parts.len() != 3 {
                continue;
            }
            let first = parts[0].to_lowercase();
            if first != "4b" && first != "5b" && first != "6b" && first != "8b" {
                // song_id|mode|diff 포맷은 개발용이었으므로 무시
                continue;
            }
            // mode|title|diff → song_id 매칭
            let mode_str = parts[0].to_uppercase();
            let title = parts[1];
            let diff_str = parts[2].to_uppercase();
            let Some(mode) = Mode::from_str(&mode_str) else { continue };
            let Some(diff) = Difficulty::from_str(&diff_str) else { continue };
            if let Some(song) =
                varchive_db.find_best_match(title, &mode_str, &diff_str, None, "", &item.note)
            {
                items.insert((song.title.to_string(), mode, diff), item);
            }
        }

        // 새 포맷으로 re-save (다음 로드부터 Array 경로)
        let meta = Self { items };
        meta.save(path_ref);
        meta
    }

    pub fn get(&self, song_id: &str, mode: Mode, diff: Difficulty) -> PatternSheetMetaItem {
        self.items
            .get(&(song_id.to_string(), mode, diff))
            .cloned()
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) {
        let entries: Vec<PatternMetaEntry> = self
            .items
            .iter()
            .map(|((song_id, mode, diff), meta)| PatternMetaEntry {
                song_id: song_id.clone(),
                mode: *mode,
                diff: *diff,
                meta: meta.clone(),
            })
            .collect();
        if let Ok(serialized) = serde_json::to_string_pretty(&entries) {
            let _ = std::fs::write(path, serialized);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_uses_tuple_key() {
        let mut items = HashMap::new();
        items.insert(
            ("123".to_string(), Mode::B5, Difficulty::SC),
            PatternSheetMetaItem {
                gold: GoldMeta::Random,
                note: "개인차".into(),
                assist_key: AssistMeta::Used,
                keypart: false,
            },
        );
        let meta = PatternSheetMeta { items };

        assert_eq!(
            meta.get("123", Mode::B5, Difficulty::SC),
            PatternSheetMetaItem {
                gold: GoldMeta::Random,
                note: "개인차".into(),
                assist_key: AssistMeta::Used,
                keypart: false,
            }
        );
    }
}
