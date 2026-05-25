use overmax_core::SceneType;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoiRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneRoiConfig {
    pub rois: HashMap<String, RoiRect>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalRoiConfig {
    pub scenes: HashMap<SceneType, SceneRoiConfig>,
}

impl Default for GlobalRoiConfig {
    fn default() -> Self {
        let mut scenes = HashMap::new();
        
        // Freestyle ROI
        let mut freestyle_rois = HashMap::new();
        freestyle_rois.insert("jacket".to_string(), RoiRect { x: 710, y: 534, width: 58, height: 58 });
        freestyle_rois.insert("rate".to_string(), RoiRect { x: 176, y: 583, width: 94, height: 22 });
        freestyle_rois.insert("btn_mode".to_string(), RoiRect { x: 80, y: 130, width: 5, height: 5 });
        freestyle_rois.insert("max_combo_badge".to_string(), RoiRect { x: 409, y: 587, width: 36, height: 33 });
        freestyle_rois.insert("diff_panel".to_string(), RoiRect { x: 98, y: 488, width: 110, height: 28 });
        scenes.insert(SceneType::Freestyle, SceneRoiConfig { rois: freestyle_rois });

        // Online ROI
        let mut online_rois = HashMap::new();
        online_rois.insert("jacket".to_string(), RoiRect { x: 664, y: 534, width: 60, height: 58 });
        online_rois.insert("rate".to_string(), RoiRect { x: 191, y: 554, width: 94, height: 27 });
        online_rois.insert("btn_mode".to_string(), RoiRect { x: 60, y: 130, width: 5, height: 5 });
        online_rois.insert("max_combo_badge".to_string(), RoiRect { x: 397, y: 601, width: 36, height: 36 });
        online_rois.insert("diff_panel".to_string(), RoiRect { x: 82, y: 467, width: 116, height: 31 });
        scenes.insert(SceneType::Online, SceneRoiConfig { rois: online_rois });
        
        Self { scenes }
    }
}
