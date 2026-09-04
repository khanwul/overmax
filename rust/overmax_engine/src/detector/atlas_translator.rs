use crate::capture::frame::CapturedFrame;
use crate::capture::frame_utils::{crop_roi, ImageView};
use crate::detector::roi::RoiRect;
use overmax_core::{Difficulty, SceneType};

/// 컴파일 타임에 100% 결정되는 제로 코스트 O(1) 아틀라스 트랜슬레이터 (Zero Runtime Overhead)
pub struct AtlasTranslator;

impl AtlasTranslator {
    /// 512x512 아틀라스 내부에서 지정된 씬과 이름에 해당하는 ROI 사각형을 반환합니다.
    ///
    /// 컴파일러가 완벽한 O(1) 점프 테이블로 인라인화하므로 런타임 룩업 비용이 0입니다.
    #[inline(always)]
    pub const fn get_roi_for_scene(name: &str, scene: SceneType) -> Option<RoiRect> {
        match (scene, name.as_bytes()) {
            // [ResultFreestyle]
            (SceneType::ResultFreestyle, b"score") => Some(RoiRect {
                x1: 0,
                y1: 0,
                x2: 407,
                y2: 94,
            }),
            (SceneType::ResultFreestyle, b"mode") => Some(RoiRect {
                x1: 0,
                y1: 94,
                x2: 340,
                y2: 169,
            }),
            (SceneType::ResultFreestyle, b"max_combo_badge") => Some(RoiRect {
                x1: 407,
                y1: 0,
                x2: 482,
                y2: 75,
            }),
            (SceneType::ResultFreestyle, b"rate") => Some(RoiRect {
                x1: 0,
                y1: 470,
                x2: 129,
                y2: 502,
            }),
            (SceneType::ResultFreestyle, b"jacket") => Some(RoiRect {
                x1: 75,
                y1: 395,
                x2: 135,
                y2: 455,
            }),
            (SceneType::ResultFreestyle, b"mode_digit") => Some(RoiRect {
                x1: 320,
                y1: 229,
                x2: 370,
                y2: 297,
            }),
            (SceneType::ResultFreestyle, b"diff_panel" | b"diff_panel_NM") => Some(RoiRect {
                x1: 422,
                y1: 315,
                x2: 512,
                y2: 333,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_HD") => Some(RoiRect {
                x1: 422,
                y1: 333,
                x2: 512,
                y2: 351,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_MX") => Some(RoiRect {
                x1: 422,
                y1: 351,
                x2: 512,
                y2: 369,
            }),
            (SceneType::ResultFreestyle, b"diff_panel_SC") => Some(RoiRect {
                x1: 418,
                y1: 369,
                x2: 508,
                y2: 387,
            }),
            (SceneType::ResultFreestyle, b"mode_colorbar") => Some(RoiRect {
                x1: 493,
                y1: 0,
                x2: 499,
                y2: 96,
            }),

            // [ResultOpen3]
            (SceneType::ResultOpen3, b"score") => Some(RoiRect {
                x1: 0,
                y1: 169,
                x2: 317,
                y2: 243,
            }),
            (SceneType::ResultOpen3, b"player_panel") => Some(RoiRect {
                x1: 0,
                y1: 315,
                x2: 316,
                y2: 355,
            }),
            (SceneType::ResultOpen3, b"max_combo_badge") => Some(RoiRect {
                x1: 407,
                y1: 75,
                x2: 482,
                y2: 150,
            }),
            (SceneType::ResultOpen3, b"jacket") => Some(RoiRect {
                x1: 400,
                y1: 150,
                x2: 460,
                y2: 210,
            }),
            (SceneType::ResultOpen3, b"rate") => Some(RoiRect {
                x1: 245,
                y1: 457,
                x2: 352,
                y2: 487,
            }),
            (SceneType::ResultOpen3, b"openmatch_diff") => Some(RoiRect {
                x1: 320,
                y1: 297,
                x2: 426,
                y2: 315,
            }),
            (SceneType::ResultOpen3, b"openmatch_mode") => Some(RoiRect {
                x1: 347,
                y1: 507,
                x2: 352,
                y2: 512,
            }),

            // [ResultOpen2]
            (SceneType::ResultOpen2, b"score") => Some(RoiRect {
                x1: 0,
                y1: 243,
                x2: 320,
                y2: 315,
            }),
            (SceneType::ResultOpen2, b"player_panel") => Some(RoiRect {
                x1: 0,
                y1: 355,
                x2: 316,
                y2: 395,
            }),
            (SceneType::ResultOpen2, b"max_combo_badge") => Some(RoiRect {
                x1: 0,
                y1: 395,
                x2: 75,
                y2: 470,
            }),
            (SceneType::ResultOpen2, b"jacket") => Some(RoiRect {
                x1: 135,
                y1: 395,
                x2: 195,
                y2: 455,
            }),
            (SceneType::ResultOpen2, b"rate") => Some(RoiRect {
                x1: 311,
                y1: 395,
                x2: 418,
                y2: 426,
            }),
            (SceneType::ResultOpen2, b"openmatch_diff") => Some(RoiRect {
                x1: 316,
                y1: 335,
                x2: 422,
                y2: 353,
            }),
            (SceneType::ResultOpen2, b"openmatch_mode") => Some(RoiRect {
                x1: 352,
                y1: 507,
                x2: 357,
                y2: 512,
            }),

            // [Freestyle]
            (SceneType::Freestyle, b"jacket") => Some(RoiRect {
                x1: 340,
                y1: 94,
                x2: 400,
                y2: 154,
            }),
            (SceneType::Freestyle, b"diff_panel" | b"diff_panel_NM") => Some(RoiRect {
                x1: 352,
                y1: 457,
                x2: 462,
                y2: 485,
            }),
            (SceneType::Freestyle, b"diff_panel_HD") => Some(RoiRect {
                x1: 361,
                y1: 426,
                x2: 471,
                y2: 454,
            }),
            (SceneType::Freestyle, b"diff_panel_MX") => Some(RoiRect {
                x1: 370,
                y1: 241,
                x2: 480,
                y2: 269,
            }),
            (SceneType::Freestyle, b"diff_panel_SC") => Some(RoiRect {
                x1: 370,
                y1: 269,
                x2: 480,
                y2: 297,
            }),
            (SceneType::Freestyle, b"score") => Some(RoiRect {
                x1: 129,
                y1: 487,
                x2: 233,
                y2: 511,
            }),
            (SceneType::Freestyle, b"rate") => Some(RoiRect {
                x1: 233,
                y1: 487,
                x2: 337,
                y2: 509,
            }),
            (SceneType::Freestyle, b"max_combo_badge") => Some(RoiRect {
                x1: 418,
                y1: 387,
                x2: 454,
                y2: 423,
            }),
            (SceneType::Freestyle, b"btn_mode") => Some(RoiRect {
                x1: 337,
                y1: 507,
                x2: 342,
                y2: 512,
            }),

            // [OpenMatch & LadderMatch (동일 레이아웃 공유)]
            (SceneType::OpenMatch | SceneType::LadderMatch, b"jacket") => Some(RoiRect {
                x1: 317,
                y1: 169,
                x2: 377,
                y2: 229,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel" | b"diff_panel_NM") => {
                Some(RoiRect {
                    x1: 377,
                    y1: 210,
                    x2: 493,
                    y2: 241,
                })
            }
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_HD") => Some(RoiRect {
                x1: 129,
                y1: 455,
                x2: 245,
                y2: 486,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_MX") => Some(RoiRect {
                x1: 195,
                y1: 395,
                x2: 311,
                y2: 426,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"diff_panel_SC") => Some(RoiRect {
                x1: 245,
                y1: 426,
                x2: 361,
                y2: 457,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"score") => Some(RoiRect {
                x1: 337,
                y1: 487,
                x2: 443,
                y2: 507,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"rate") => Some(RoiRect {
                x1: 316,
                y1: 315,
                x2: 419,
                y2: 335,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"max_combo_badge") => Some(RoiRect {
                x1: 454,
                y1: 387,
                x2: 490,
                y2: 423,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, b"btn_mode") => Some(RoiRect {
                x1: 342,
                y1: 507,
                x2: 347,
                y2: 512,
            }),

            _ => None,
        }
    }

    /// 지정된 씬과 난이도에 해당하는 아틀라스 내부의 diff_panel ROI를 반환합니다.
    #[inline(always)]
    pub const fn get_diff_panel_roi_for_scene(
        diff: Difficulty,
        scene: SceneType,
    ) -> Option<RoiRect> {
        match (scene, diff) {
            // Freestyle
            (SceneType::Freestyle, Difficulty::NM) => Some(RoiRect {
                x1: 352,
                y1: 457,
                x2: 462,
                y2: 485,
            }),
            (SceneType::Freestyle, Difficulty::HD) => Some(RoiRect {
                x1: 361,
                y1: 426,
                x2: 471,
                y2: 454,
            }),
            (SceneType::Freestyle, Difficulty::MX) => Some(RoiRect {
                x1: 370,
                y1: 241,
                x2: 480,
                y2: 269,
            }),
            (SceneType::Freestyle, Difficulty::SC) => Some(RoiRect {
                x1: 370,
                y1: 269,
                x2: 480,
                y2: 297,
            }),

            // OpenMatch & LadderMatch
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::NM) => Some(RoiRect {
                x1: 377,
                y1: 210,
                x2: 493,
                y2: 241,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::HD) => Some(RoiRect {
                x1: 129,
                y1: 455,
                x2: 245,
                y2: 486,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::MX) => Some(RoiRect {
                x1: 195,
                y1: 395,
                x2: 311,
                y2: 426,
            }),
            (SceneType::OpenMatch | SceneType::LadderMatch, Difficulty::SC) => Some(RoiRect {
                x1: 245,
                y1: 426,
                x2: 361,
                y2: 457,
            }),

            // ResultFreestyle
            (SceneType::ResultFreestyle, Difficulty::NM) => Some(RoiRect {
                x1: 422,
                y1: 315,
                x2: 512,
                y2: 333,
            }),
            (SceneType::ResultFreestyle, Difficulty::HD) => Some(RoiRect {
                x1: 422,
                y1: 333,
                x2: 512,
                y2: 351,
            }),
            (SceneType::ResultFreestyle, Difficulty::MX) => Some(RoiRect {
                x1: 422,
                y1: 351,
                x2: 512,
                y2: 369,
            }),
            (SceneType::ResultFreestyle, Difficulty::SC) => Some(RoiRect {
                x1: 418,
                y1: 369,
                x2: 508,
                y2: 387,
            }),

            _ => None,
        }
    }

    /// 아틀라스 프레임(512x512)에서 특정 씬의 ROI를 직접 크롭하여 ImageView로 반환합니다.
    #[inline]
    pub fn crop_roi<'a>(
        atlas_frame: &'a CapturedFrame,
        name: &str,
        scene: SceneType,
    ) -> Option<ImageView<'a>> {
        let roi = Self::get_roi_for_scene(name, scene)?;
        crop_roi(atlas_frame, roi)
    }

    /// 아틀라스 프레임에서 diff_panel ROI를 직접 크롭하여 ImageView로 반환합니다.
    #[inline]
    pub fn crop_diff_panel_roi<'a>(
        atlas_frame: &'a CapturedFrame,
        diff: Difficulty,
        scene: SceneType,
    ) -> Option<ImageView<'a>> {
        let roi = Self::get_diff_panel_roi_for_scene(diff, scene)?;
        crop_roi(atlas_frame, roi)
    }

    /// 기존 `RoiManager::and_then_roi`와 동일한 클로저 기반 호출 편의 인터페이스
    #[inline]
    pub fn and_then_roi<'a, T>(
        atlas_frame: &'a CapturedFrame,
        name: &str,
        scene: SceneType,
        f: impl FnOnce(&ImageView<'a>) -> Option<T>,
    ) -> Option<T> {
        Self::crop_roi(atlas_frame, name, scene)
            .as_ref()
            .and_then(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::atlas_layout::{ATLAS_HEIGHT, ATLAS_SLOTS, ATLAS_WIDTH};
    use crate::detector::roi::RoiManager;

    #[test]
    fn test_all_atlas_slots_resolvable_via_translator() {
        for slot in &ATLAS_SLOTS {
            let translated = AtlasTranslator::get_roi_for_scene(slot.name, slot.scene)
                .unwrap_or_else(|| {
                    panic!("Failed to resolve slot {} in {:?}", slot.name, slot.scene)
                });

            assert_eq!(
                translated.x1, slot.atlas_rect.x,
                "x1 mismatch for {} in {:?}",
                slot.name, slot.scene
            );
            assert_eq!(
                translated.y1, slot.atlas_rect.y,
                "y1 mismatch for {} in {:?}",
                slot.name, slot.scene
            );
            assert_eq!(
                translated.x2,
                slot.atlas_rect.x + slot.atlas_rect.width,
                "x2 mismatch for {} in {:?}",
                slot.name,
                slot.scene
            );
            assert_eq!(
                translated.y2,
                slot.atlas_rect.y + slot.atlas_rect.height,
                "y2 mismatch for {} in {:?}",
                slot.name,
                slot.scene
            );
        }
    }

    #[test]
    fn test_dimensions_match_roi_manager_1080p() {
        let roi_manager = RoiManager::new(1920, 1080);

        for slot in &ATLAS_SLOTS {
            let translated = AtlasTranslator::get_roi_for_scene(slot.name, slot.scene).unwrap();

            let original_roi = if slot.name.starts_with("diff_panel_") {
                let diff_name = slot.name.strip_prefix("diff_panel_").unwrap();
                let diff = match diff_name {
                    "NM" => Difficulty::NM,
                    "HD" => Difficulty::HD,
                    "MX" => Difficulty::MX,
                    "SC" => Difficulty::SC,
                    _ => panic!("Unknown diff: {}", diff_name),
                };
                roi_manager
                    .get_diff_panel_roi_for_scene(diff, slot.scene)
                    .unwrap()
            } else {
                roi_manager
                    .get_roi_for_scene(slot.name, slot.scene)
                    .unwrap()
            };

            assert_eq!(
                translated.width(),
                original_roi.width(),
                "Width mismatch for {} in {:?}: atlas={}, original={}",
                slot.name,
                slot.scene,
                translated.width(),
                original_roi.width()
            );
            assert_eq!(
                translated.height(),
                original_roi.height(),
                "Height mismatch for {} in {:?}: atlas={}, original={}",
                slot.name,
                slot.scene,
                translated.height(),
                original_roi.height()
            );
        }
    }

    #[test]
    fn test_diff_panel_translator_consistency() {
        for scene in [
            SceneType::Freestyle,
            SceneType::OpenMatch,
            SceneType::LadderMatch,
            SceneType::ResultFreestyle,
        ] {
            for diff in Difficulty::ALL {
                let direct = AtlasTranslator::get_diff_panel_roi_for_scene(diff, scene)
                    .unwrap_or_else(|| panic!("Diff {:?} missing for {:?}", diff, scene));

                let name = match diff {
                    Difficulty::NM => "diff_panel_NM",
                    Difficulty::HD => "diff_panel_HD",
                    Difficulty::MX => "diff_panel_MX",
                    Difficulty::SC => "diff_panel_SC",
                };
                let by_name = AtlasTranslator::get_roi_for_scene(name, scene)
                    .unwrap_or_else(|| panic!("Named diff {} missing for {:?}", name, scene));

                assert_eq!(direct, by_name);
                assert!(direct.x1 >= 0 && direct.x2 <= ATLAS_WIDTH as i32);
                assert!(direct.y1 >= 0 && direct.y2 <= ATLAS_HEIGHT as i32);
            }
        }
    }

    #[test]
    fn test_ladder_match_shares_open_match_atlas_coordinates() {
        for name in [
            "jacket",
            "rate",
            "score",
            "btn_mode",
            "max_combo_badge",
            "diff_panel_NM",
            "diff_panel_HD",
            "diff_panel_MX",
            "diff_panel_SC",
        ] {
            let open_match_roi =
                AtlasTranslator::get_roi_for_scene(name, SceneType::OpenMatch).unwrap();
            let ladder_match_roi =
                AtlasTranslator::get_roi_for_scene(name, SceneType::LadderMatch).unwrap();

            assert_eq!(
                open_match_roi, ladder_match_roi,
                "LadderMatch ROI mismatch with OpenMatch for {}",
                name
            );
        }
    }
}
