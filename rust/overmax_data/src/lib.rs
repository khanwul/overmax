pub mod community;
pub mod config;
pub mod service;
pub mod store;

pub use community::client::VArchiveDB;
pub use community::sheet_meta::{PatternSheetMeta, PatternSheetMetaItem};
pub use community::sync::{
    build_candidates, delete_varchive_cache_record, load_varchive_record_cache,
    save_fetched_records_to_cache, upsert_varchive_cache_record, SyncCandidate,
};
pub use config::compatibility::DataCompatibility;
pub use config::scene_config::{GlobalRoiConfig, RoiRect, SceneRoiConfig};
pub use config::settings::{
    diff_settings, load_base_settings, load_merged_settings, merge_settings_layers,
    normalize_settings, save_user_settings, AppUpdateSettings, DebugWindowSettings,
    JacketMatcherSettings, OverlayPosition, OverlaySettings, ScreenCaptureSettings, Settings,
    SettingsPaths, VArchiveSettings, VArchiveUserMap, WindowTrackerSettings,
};
pub use overmax_core::{RecordKey, RecordValue};
pub use service::jacket_matcher::{JacketMatcher, JacketMatcherConfig};
pub use service::recommend::{RecommendEntry, RecommendResult, Recommender};
pub use service::record_manager::{RecordManager, RecordSource};
pub use store::image_index::{ImageEntry, ImageIndexDb, ImageMatch};
pub use store::record_db::RecordDB;
