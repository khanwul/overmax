pub mod config;
pub mod store;
pub mod community;
pub mod service;

pub use config::compatibility::DataCompatibility;
pub use store::image_index::{ImageIndexDb, ImageMatch, ImageEntry};
pub use service::jacket_matcher::{JacketMatcher, JacketMatcherConfig};
pub use service::recommend::{RecommendEntry, RecommendResult, Recommender};
pub use store::record_db::RecordDB;
pub use service::record_manager::{RecordManager, RecordSource};
pub use config::scene_config::{GlobalRoiConfig, RoiRect, SceneRoiConfig};
pub use config::settings::{
    diff_settings, load_base_settings, load_merged_settings, merge_settings_layers,
    normalize_settings, save_user_settings, SettingsPaths, Settings, WindowTrackerSettings,
    ScreenCaptureSettings, DebugWindowSettings, OverlaySettings, JacketMatcherSettings,
    AppUpdateSettings, VArchiveSettings, OverlayPosition, VArchiveUserMap,
};
pub use community::sheet_meta::{PatternSheetMeta, PatternSheetMetaItem};
pub use community::sync::{
    build_candidates, load_varchive_record_cache, upsert_varchive_cache_record, save_fetched_records_to_cache, delete_varchive_cache_record, SyncCandidate,
};
pub use community::client::VArchiveDB;
