use crate::detector::roi_config::RawRoiRect;
use overmax_core::SceneType;

/// GPU 아틀라스 텍스처 규격 (512x512 RGBA8, 1 MB)
pub const ATLAS_WIDTH: u32 = 512;
pub const ATLAS_HEIGHT: u32 = 512;
pub const ATLAS_SLOT_COUNT: usize = 43;

/// 정적 아틀라스 내부의 개별 ROI 슬롯 배치 정보
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtlasSlot {
    /// 해당 ROI가 속한 게임 씬
    pub scene: SceneType,
    /// ROI 식별자 이름
    pub name: &'static str,
    /// 1080p 16:9 게임 화면 기준 원본 좌표 (x, y, w, h)
    pub src_rect: RawRoiRect,
    /// 512x512 아틀라스 텍스처 내부의 정적 배치 좌표 (x, y, w, h)
    pub atlas_rect: RawRoiRect,
}

/// 컴파일 타임에 2D MaxRects 알고리즘으로 100% 무손실 배치된 43개 정적 슬롯 테이블 (Zero Heap Allocation)
pub const ATLAS_SLOTS: [AtlasSlot; ATLAS_SLOT_COUNT] = [
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "score",
        src_rect: RawRoiRect {
            x: 759,
            y: 710,
            width: 407,
            height: 94,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 0,
            width: 407,
            height: 94,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "mode",
        src_rect: RawRoiRect {
            x: 0,
            y: 18,
            width: 340,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 94,
            width: 340,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "score",
        src_rect: RawRoiRect {
            x: 211,
            y: 753,
            width: 317,
            height: 74,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 169,
            width: 317,
            height: 74,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "score",
        src_rect: RawRoiRect {
            x: 311,
            y: 753,
            width: 320,
            height: 72,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 243,
            width: 320,
            height: 72,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "player_panel",
        src_rect: RawRoiRect {
            x: 212,
            y: 830,
            width: 316,
            height: 40,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 315,
            width: 316,
            height: 40,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "player_panel",
        src_rect: RawRoiRect {
            x: 312,
            y: 830,
            width: 316,
            height: 40,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 355,
            width: 316,
            height: 40,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 1024,
            y: 521,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 407,
            y: 0,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 437,
            y: 591,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 407,
            y: 75,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 537,
            y: 591,
            width: 75,
            height: 75,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 395,
            width: 75,
            height: 75,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "rate",
        src_rect: RawRoiRect {
            x: 891,
            y: 608,
            width: 129,
            height: 32,
        },
        atlas_rect: RawRoiRect {
            x: 0,
            y: 470,
            width: 129,
            height: 32,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 710,
            y: 533,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 340,
            y: 94,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 664,
            y: 533,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 317,
            y: 169,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 705,
            y: 14,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 75,
            y: 395,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 705,
            y: 14,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 400,
            y: 150,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "jacket",
        src_rect: RawRoiRect {
            x: 705,
            y: 14,
            width: 60,
            height: 60,
        },
        atlas_rect: RawRoiRect {
            x: 135,
            y: 395,
            width: 60,
            height: 60,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 82,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 377,
            y: 210,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 202,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 129,
            y: 455,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 322,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 195,
            y: 395,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 442,
            y: 467,
            width: 116,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 245,
            y: 426,
            width: 116,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "mode_digit",
        src_rect: RawRoiRect {
            x: 78,
            y: 28,
            width: 50,
            height: 68,
        },
        atlas_rect: RawRoiRect {
            x: 320,
            y: 229,
            width: 50,
            height: 68,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "rate",
        src_rect: RawRoiRect {
            x: 403,
            y: 673,
            width: 107,
            height: 31,
        },
        atlas_rect: RawRoiRect {
            x: 311,
            y: 395,
            width: 107,
            height: 31,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "rate",
        src_rect: RawRoiRect {
            x: 293,
            y: 673,
            width: 107,
            height: 30,
        },
        atlas_rect: RawRoiRect {
            x: 245,
            y: 457,
            width: 107,
            height: 30,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 98,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 352,
            y: 457,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 218,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 361,
            y: 426,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 338,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 370,
            y: 241,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 458,
            y: 488,
            width: 110,
            height: 28,
        },
        atlas_rect: RawRoiRect {
            x: 370,
            y: 269,
            width: 110,
            height: 28,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "score",
        src_rect: RawRoiRect {
            x: 173,
            y: 558,
            width: 104,
            height: 24,
        },
        atlas_rect: RawRoiRect {
            x: 129,
            y: 487,
            width: 104,
            height: 24,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "rate",
        src_rect: RawRoiRect {
            x: 172,
            y: 583,
            width: 104,
            height: 22,
        },
        atlas_rect: RawRoiRect {
            x: 233,
            y: 487,
            width: 104,
            height: 22,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "score",
        src_rect: RawRoiRect {
            x: 77,
            y: 558,
            width: 106,
            height: 20,
        },
        atlas_rect: RawRoiRect {
            x: 337,
            y: 487,
            width: 106,
            height: 20,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "rate",
        src_rect: RawRoiRect {
            x: 197,
            y: 558,
            width: 103,
            height: 20,
        },
        atlas_rect: RawRoiRect {
            x: 316,
            y: 315,
            width: 103,
            height: 20,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "openmatch_diff",
        src_rect: RawRoiRect {
            x: 410,
            y: 841,
            width: 106,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 320,
            y: 297,
            width: 106,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "openmatch_diff",
        src_rect: RawRoiRect {
            x: 510,
            y: 841,
            width: 106,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 316,
            y: 335,
            width: 106,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_NM",
        src_rect: RawRoiRect {
            x: 709,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 422,
            y: 315,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_HD",
        src_rect: RawRoiRect {
            x: 829,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 422,
            y: 333,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_MX",
        src_rect: RawRoiRect {
            x: 949,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 422,
            y: 351,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "diff_panel_SC",
        src_rect: RawRoiRect {
            x: 1069,
            y: 86,
            width: 90,
            height: 18,
        },
        atlas_rect: RawRoiRect {
            x: 418,
            y: 369,
            width: 90,
            height: 18,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 409,
            y: 585,
            width: 36,
            height: 36,
        },
        atlas_rect: RawRoiRect {
            x: 418,
            y: 387,
            width: 36,
            height: 36,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "max_combo_badge",
        src_rect: RawRoiRect {
            x: 398,
            y: 601,
            width: 36,
            height: 36,
        },
        atlas_rect: RawRoiRect {
            x: 454,
            y: 387,
            width: 36,
            height: 36,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultFreestyle,
        name: "mode_colorbar",
        src_rect: RawRoiRect {
            x: 60,
            y: 0,
            width: 6,
            height: 96,
        },
        atlas_rect: RawRoiRect {
            x: 493,
            y: 0,
            width: 6,
            height: 96,
        },
    },
    AtlasSlot {
        scene: SceneType::Freestyle,
        name: "btn_mode",
        src_rect: RawRoiRect {
            x: 80,
            y: 130,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 337,
            y: 507,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::OpenMatch,
        name: "btn_mode",
        src_rect: RawRoiRect {
            x: 60,
            y: 130,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 342,
            y: 507,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen3,
        name: "openmatch_mode",
        src_rect: RawRoiRect {
            x: 212,
            y: 830,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 347,
            y: 507,
            width: 5,
            height: 5,
        },
    },
    AtlasSlot {
        scene: SceneType::ResultOpen2,
        name: "openmatch_mode",
        src_rect: RawRoiRect {
            x: 312,
            y: 830,
            width: 5,
            height: 5,
        },
        atlas_rect: RawRoiRect {
            x: 352,
            y: 507,
            width: 5,
            height: 5,
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::detector::roi_config::GlobalRoiConfig;
    use overmax_core::Difficulty;

    #[test]
    fn test_all_slots_within_atlas_bounds() {
        for slot in &ATLAS_SLOTS {
            assert!(
                slot.atlas_rect.x >= 0,
                "Slot {} ({:?}) negative atlas x: {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.x
            );
            assert!(
                slot.atlas_rect.y >= 0,
                "Slot {} ({:?}) negative atlas y: {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.y
            );
            assert!(
                slot.atlas_rect.x + slot.atlas_rect.width <= ATLAS_WIDTH as i32,
                "Slot {} ({:?}) exceeds ATLAS_WIDTH: {} + {} = {} > {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.x,
                slot.atlas_rect.width,
                slot.atlas_rect.x + slot.atlas_rect.width,
                ATLAS_WIDTH
            );
            assert!(
                slot.atlas_rect.y + slot.atlas_rect.height <= ATLAS_HEIGHT as i32,
                "Slot {} ({:?}) exceeds ATLAS_HEIGHT: {} + {} = {} > {}",
                slot.name,
                slot.scene,
                slot.atlas_rect.y,
                slot.atlas_rect.height,
                slot.atlas_rect.y + slot.atlas_rect.height,
                ATLAS_HEIGHT
            );
        }
    }

    #[test]
    fn test_no_overlapping_slots_in_atlas() {
        for (i, s1) in ATLAS_SLOTS.iter().enumerate() {
            let r1 = s1.atlas_rect;
            for s2 in ATLAS_SLOTS.iter().skip(i + 1) {
                let r2 = s2.atlas_rect;

                let overlap = !(r1.x + r1.width <= r2.x
                    || r2.x + r2.width <= r1.x
                    || r1.y + r1.height <= r2.y
                    || r2.y + r2.height <= r1.y);

                assert!(
                    !overlap,
                    "Overlap detected between slot {} ({:?}) at {:?} and slot {} ({:?}) at {:?}",
                    s1.name, s1.scene, r1, s2.name, s2.scene, r2
                );
            }
        }
    }

    #[test]
    fn test_positive_slot_dimensions() {
        for slot in &ATLAS_SLOTS {
            assert!(slot.src_rect.width > 0);
            assert!(slot.src_rect.height > 0);
            assert_eq!(slot.src_rect.width, slot.atlas_rect.width);
            assert_eq!(slot.src_rect.height, slot.atlas_rect.height);
        }
    }

    #[test]
    fn test_src_rect_matches_global_roi_config() {
        let global_config = GlobalRoiConfig::default();

        for slot in &ATLAS_SLOTS {
            let scene_config = global_config
                .scenes
                .get(&slot.scene)
                .unwrap_or_else(|| panic!("Scene {:?} not found in GlobalRoiConfig", slot.scene));

            if slot.name.starts_with("diff_panel_") {
                let diff_name = slot.name.strip_prefix("diff_panel_").unwrap();
                let diff = match diff_name {
                    "NM" => Difficulty::NM,
                    "HD" => Difficulty::HD,
                    "MX" => Difficulty::MX,
                    "SC" => Difficulty::SC,
                    _ => panic!("Unknown diff: {}", diff_name),
                };
                let base_diff_rect = scene_config
                    .rois
                    .get("diff_panel")
                    .unwrap_or_else(|| panic!("diff_panel not found in {:?}", slot.scene));
                let offset = match diff {
                    Difficulty::NM => 0,
                    Difficulty::HD => 120,
                    Difficulty::MX => 240,
                    Difficulty::SC => 360,
                };
                assert_eq!(
                    slot.src_rect.x,
                    base_diff_rect.x + offset,
                    "Slot {} diff x mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.y, base_diff_rect.y,
                    "Slot {} diff y mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.width, base_diff_rect.width,
                    "Slot {} diff width mismatch",
                    slot.name
                );
                assert_eq!(
                    slot.src_rect.height, base_diff_rect.height,
                    "Slot {} diff height mismatch",
                    slot.name
                );
            } else {
                let expected = scene_config.rois.get(slot.name).unwrap_or_else(|| {
                    panic!("ROI {} not found in scene {:?}", slot.name, slot.scene)
                });
                assert_eq!(
                    slot.src_rect, *expected,
                    "Src rect mismatch for {} in {:?}",
                    slot.name, slot.scene
                );
            }
        }
    }
}
