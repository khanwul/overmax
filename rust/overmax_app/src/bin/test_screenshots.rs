use std::fs;
use std::path::Path;
use image::GenericImageView;

use overmax_app::screen_capture::CapturedFrame;
use overmax_app::roi::RoiManager;
use overmax_app::ocr_engine::OcrDetector;
use overmax_app::frame_utils::crop_roi;
use overmax_app::play_state::MIN_VALID_RATE;
use overmax_core::SceneType;

fn load_frame(path: &Path) -> CapturedFrame {
    let img = image::open(path).expect("failed to open image");
    let (w, h) = img.dimensions();
    let mut rgba = img.to_rgba8().into_raw();
    for chunk in rgba.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
    CapturedFrame {
        width: w as i32,
        height: h as i32,
        bgra: rgba,
    }
}

fn detect_scene_from_logo(frame: &CapturedFrame, ocr: &OcrDetector, rois: &RoiManager) -> SceneType {
    let logo_roi = match rois.get_roi("logo") {
        Some(roi) => roi,
        None => return SceneType::Unknown,
    };
    let logo_img = match crop_roi(frame, logo_roi) {
        Some(img) => img,
        None => return SceneType::Unknown,
    };
    let (scene, raw_text, _) = ocr.detect_logo(&logo_img);
    println!("      [Logo OCR] raw: '{}', scene: {:?}", raw_text.trim(), scene);
    scene
}

fn main() {
    let screenshots_dir = Path::new("scratch/screenshots/converted");
    if !screenshots_dir.exists() {
        eprintln!("Error: Converted screenshots directory not found.");
        return;
    }
    
    println!("--- Initializing OCR Detector ---");
    let ocr = OcrDetector::new();
    if !ocr.is_available() {
        eprintln!("Error: Windows OCR is not available on this system.");
        return;
    }
    
    let mut entries: Vec<_> = fs::read_dir(screenshots_dir)
        .expect("failed to read dir")
        .filter_map(|e| e.ok())
        .collect();
    
    // 파일명 정렬
    entries.sort_by_key(|e| e.file_name());
    
    println!("Found {} screenshots to test.", entries.len());
    
    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("png") {
            continue;
        }
        
        println!("\n==================================================");
        println!("Testing: {}", path.file_name().unwrap().to_string_lossy());
        
        let frame = load_frame(&path);
        let mut rois = RoiManager::new(frame.width, frame.height);
        
        // 1. Logo 분석을 통해 씬 판별
        let mut scene = detect_scene_from_logo(&frame, &ocr, &rois);
        println!("  - Detected Scene: {:?}", scene);
        
        if scene == SceneType::Unknown {
            // 파일명에 따라 임의 매칭 시도 (만약 로고 OCR이 실패한 경우 대비)
            let fname = path.file_name().unwrap().to_string_lossy().to_lowercase();
            if fname.contains("freestyle") {
                scene = SceneType::Freestyle;
                println!("  - (Fallback) Using Freestyle from filename");
            } else if fname.contains("open") || fname.contains("match") {
                scene = SceneType::OpenMatch;
                println!("  - (Fallback) Using OpenMatch from filename");
            } else {
                // 기본적으로 Freestyle과 OpenMatch 둘 다 테스트
                println!("  - Scene Unknown. Running testing for both Freestyle and OpenMatch:");
                for test_scene in &[SceneType::Freestyle, SceneType::OpenMatch] {
                    println!("    --- Testing as Scene: {:?} ---", test_scene);
                    rois.set_scene(*test_scene);
                    run_roi_test(&frame, &ocr, &rois, *test_scene);
                }
                continue;
            }
        }
        
        rois.set_scene(scene);
        run_roi_test(&frame, &ocr, &rois, scene);
    }
}

fn run_roi_test(frame: &CapturedFrame, ocr: &OcrDetector, rois: &RoiManager, scene: SceneType) {
    // Rate ROI 테스트
    let mut rate_val: Option<f32> = None;
    if let Some(rate_roi) = rois.get_roi("rate") {
        if let Some(rate_img) = crop_roi(frame, rate_roi) {
            let res = ocr.detect_rate(&rate_img);
            rate_val = res.0;
            println!("    Rate ROI OCR Result: {:?}", res.0);
        } else {
            println!("    Failed to crop rate ROI");
        }
    } else {
        println!("    No rate ROI found for scene {:?}", scene);
    }
    
    // Score ROI 테스트
    let mut score_val: Option<u32> = None;
    if let Some(score_roi) = rois.get_roi("score") {
        if let Some(score_img) = crop_roi(frame, score_roi) {
            let res = ocr.detect_score(&score_img);
            score_val = res;
            println!("    Score ROI OCR Result: {:?}", res);
        } else {
            println!("    Failed to crop score ROI");
        }
    } else {
        println!("    No score ROI found for scene {:?}", scene);
    }
    
    // 크로스 검증 로직 모사 및 보강 테스트
    if score_val.is_some() || rate_val.is_some() {
        let is_result = matches!(
            scene,
            SceneType::ResultFreestyle | SceneType::ResultOpen3 | SceneType::ResultOpen2
        );
        let is_song_select = matches!(scene, SceneType::Freestyle | SceneType::OpenMatch);
        
        if is_result || is_song_select {
            if let Some(s_val) = score_val {
                let calc_rate = s_val as f32 / 10000.0;
                println!("    Calculated Rate from Score: {:.4}%", calc_rate);
                
                let is_valid_range = if is_song_select {
                    (MIN_VALID_RATE..=100.0).contains(&calc_rate)
                } else {
                    (0.0..=100.0).contains(&calc_rate)
                };

                if is_valid_range {
                    match rate_val {
                        Some(r) => {
                            if let Some(final_rate) = resolve_most_plausible_rate(r, calc_rate, is_song_select) {
                                println!("    [Validation] Resolved Rate: {}%", final_rate);
                            } else {
                                println!("    [Validation] Resolution failed, keeping original rate: {}%", r);
                            }
                        }
                        None => {
                            let corrected = (calc_rate * 100.0).floor() / 100.0;
                            println!("    [Validation] Rate OCR failed. Filling with score rate: {}%", corrected);
                        }
                    }
                } else {
                    println!("    [Validation] Calculated rate {:.4}% is out of valid range, ignoring.", calc_rate);
                }
            } else {
                println!("    [Validation] Score OCR failed, keeping original rate: {:?}", rate_val);
            }
        }
    }
}

fn resolve_most_plausible_rate(rate_ocr: f32, score_rate: f32, is_song_select: bool) -> Option<f32> {
    if (rate_ocr - score_rate).abs() < 0.1 {
        return Some((score_rate * 100.0).floor() / 100.0);
    }

    let score_plaus = get_rate_plausibility(score_rate);
    let ocr_plaus = get_rate_plausibility(rate_ocr);

    if score_plaus != ocr_plaus {
        if score_plaus > ocr_plaus {
            println!("    [Plausibility] Trusting Score Rate ({:.2}%) over Rate OCR ({:.2}%)", score_rate, rate_ocr);
            return Some((score_rate * 100.0).floor() / 100.0);
        } else {
            println!("    [Plausibility] Trusting Rate OCR ({:.2}%) over Score Rate ({:.2}%)", rate_ocr, score_rate);
            return Some(rate_ocr);
        }
    }

    if is_song_select {
        println!("    [Plausibility] Tie in song select. Keeping Rate OCR: {:.2}%", rate_ocr);
        Some(rate_ocr)
    } else {
        Some((score_rate * 100.0).floor() / 100.0)
    }
}

fn get_rate_plausibility(rate: f32) -> i32 {
    if (90.0..=100.0).contains(&rate) {
        3
    } else if (70.0..=90.0).contains(&rate) {
        2
    } else if (50.0..=70.0).contains(&rate) {
        1
    } else {
        0
    }
}
