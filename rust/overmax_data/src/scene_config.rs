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
        // TODO: 기존 ROI 데이터를 여기에 마이그레이션하거나, 
        // 외부 JSON 설정 파일에서 로드하도록 확장 가능합니다.
        scenes.insert(SceneType::Freestyle, SceneRoiConfig { rois: HashMap::new() });
        scenes.insert(SceneType::Online, SceneRoiConfig { rois: HashMap::new() });
        Self { scenes }
    }
}
