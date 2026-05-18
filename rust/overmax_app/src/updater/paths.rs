//! `cache/update/` paths (same layout as Python `app_updater.py`).

use std::path::{Path, PathBuf};

pub fn update_root(app_dir: &Path) -> PathBuf {
    app_dir.join("cache").join("update")
}

pub fn result_path(app_dir: &Path) -> PathBuf {
    update_root(app_dir).join("update_result.txt")
}

pub fn applied_tag_path(app_dir: &Path) -> PathBuf {
    update_root(app_dir).join("app_update_applied_tag.txt")
}
